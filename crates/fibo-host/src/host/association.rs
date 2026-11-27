//! Which transcript belongs to which terminal: log-event subscription, recovery of
//! missed log writes and following an associated session's log.
use super::*;
use crate::recognition::Transcript;
use fibo_platform::file_watch::flag;
use std::collections::HashSet;

// Building temporary lookups costs more than one or a few direct scans. The
// full log cache at startup and large repairs are the batches that amortize it.
const INDEX_MIN_ROWS: usize = 128;

type SessionPositions<'a> = HashMap<Agent, HashMap<&'a str, Option<usize>>>;

fn session_positions<'a>(
    transcripts: &'a [Transcript],
    sessions: &[AgentSession],
) -> SessionPositions<'a> {
    // Borrow the keys from the batch, so observing a transcript can update the
    // workspace in place. Keep the first position, as the former .find() did.
    let mut positions: SessionPositions<'a> = HashMap::new();
    for t in transcripts {
        positions
            .entry(t.runtime)
            .or_default()
            .insert(&t.cli_id, None);
    }
    for (i, session) in sessions.iter().enumerate() {
        if let Some(position) = positions
            .get_mut(&session.runtime)
            .and_then(|rows| rows.get_mut(session.cli_session_id.as_str()))
        {
            position.get_or_insert(i);
        }
    }
    positions
}

fn session_position(
    positions: Option<&SessionPositions<'_>>,
    sessions: &[AgentSession],
    transcript: &Transcript,
) -> Option<usize> {
    match positions {
        Some(positions) => positions
            .get(&transcript.runtime)
            .and_then(|rows| rows.get(transcript.cli_id.as_str()))
            .copied()
            .flatten(),
        None => sessions
            .iter()
            .position(|s| s.runtime == transcript.runtime && s.cli_session_id == transcript.cli_id),
    }
}

#[derive(Default)]
pub(super) struct Association {
    pub(super) attempted: std::collections::HashSet<(String, String)>,
    pub(super) inventory: Vec<u32>,
    /// (log, newest prompt) → screen comparisons so far. The TUI may paint the prompt a
    /// little after the log records it, so a few appends may retry; then it waits for a new prompt.
    pub(super) screened: HashMap<(String, String), u8>,
}
impl Association {
    pub(super) fn apply_removals(
        shared: &Shared,
        index: &crate::log_index::Index,
    ) -> io::Result<()> {
        index.publish_removals(|paths| {
            let paths: HashSet<&str> = paths.iter().map(String::as_str).collect();
            let mut host = lock(shared)?;
            let mut next = None;
            for (position, session) in host.workspace.agent_sessions.iter().enumerate() {
                if !paths.contains(session.transcript.as_str()) {
                    continue;
                }
                // The same path may already have been recreated for a different session.
                // Release the stale pointer so the previous session cannot read its records.
                if !index
                    .paths_for(session.runtime, &session.cli_session_id)?
                    .contains(&session.transcript)
                {
                    let workspace = next.get_or_insert_with(|| host.workspace.clone());
                    workspace.agent_sessions[position].transcript.clear();
                    workspace.content.remove(&session.id);
                }
            }
            if let Some(next) = next {
                host.commit(next)?;
            }
            Ok(())
        })
    }

    pub(super) fn apply(
        &mut self,
        shared: &Shared,
        index: &crate::log_index::Index,
        transcripts: &[Transcript],
    ) -> io::Result<()> {
        if transcripts.is_empty() {
            return Ok(());
        }
        let (running, unbound) = {
            let host = lock(shared)?;
            let workspace = &host.workspace;
            let running: Vec<_> = workspace
                .terminals
                .iter()
                .filter(|t| t.state == ProcessState::Running)
                .filter_map(|t| t.pid)
                .collect();
            let sessions = &workspace.agent_sessions;
            let mut unbound = Vec::new();
            if !running.is_empty() {
                let bound = (transcripts.len() >= INDEX_MIN_ROWS
                    && sessions.len() >= INDEX_MIN_ROWS)
                    .then(|| {
                        sessions
                            .iter()
                            .filter(|s| s.current)
                            .map(|s| (s.runtime, s.cli_session_id.as_str(), s.transcript.as_str()))
                            .collect::<HashSet<_>>()
                    });
                unbound.extend(transcripts.iter().filter(|t| match &bound {
                    Some(bound) => {
                        !bound.contains(&(t.runtime, t.cli_id.as_str(), t.path.as_str()))
                    }
                    None => !sessions.iter().any(|s| {
                        s.current
                            && s.runtime == t.runtime
                            && s.cli_session_id == t.cli_id
                            && s.transcript == t.path
                    }),
                }));
            }
            (running, unbound)
        };
        // A new identity/file (or a changed managed tree) gets one probe. An
        // unresolved manual Claude appending the same file is not a polling clock.
        let needs_probe = unbound.iter().any(|t| {
            running != self.inventory
                || !self.attempted.contains(&(t.path.clone(), t.cli_id.clone()))
        });
        // Known files only advance metadata. Never enumerate processes or query the whole index here.
        {
            let mut host = lock(shared)?;
            let mut next = host.workspace.clone();
            let mut positions = (transcripts.len() >= INDEX_MIN_ROWS
                && next.agent_sessions.len() >= INDEX_MIN_ROWS)
                .then(|| session_positions(transcripts, &next.agent_sessions));
            for t in transcripts {
                if session_position(positions.as_ref(), &next.agent_sessions, t).is_none()
                    && index.recent(t.updated)
                {
                    let position = next.agent_sessions.len();
                    next.observe_agent(crate::recognition::observation(t, None))
                        .map_err(io::Error::other)?;
                    if let Some(known) = &mut positions {
                        known
                            .entry(t.runtime)
                            .or_default()
                            .insert(&t.cli_id, Some(position));
                    }
                }
            }
            let before_reconcile = next.agent_sessions.len();
            crate::recognition::reconcile(&mut next, transcripts, &[], &HashMap::new())?;
            // Reconciliation keeps the rows in order and their identities intact.
            // Following a continued conversation can append an older identity.
            if let Some(positions) = positions
                .as_mut()
                .filter(|_| next.agent_sessions.len() != before_reconcile)
            {
                *positions = session_positions(transcripts, &next.agent_sessions);
            }
            // Only cards read here change content: compare those, not the whole table.
            let mut touched = vec![];
            for t in transcripts {
                if let Some(position) =
                    session_position(positions.as_ref(), &next.agent_sessions, t)
                {
                    let s = &mut next.agent_sessions[position];
                    if let Some(card) = index.card(&t.path)? {
                        if next.content.get(&s.id).is_some_and(|old| {
                            !card.turn_key.is_empty() && card.turn_key != old.turn_key
                        }) && s.outcome != fibo_core::model::Outcome::Open
                        {
                            next.outcome_history.push(fibo_core::model::OutcomeChange {
                                session: s.id.clone(),
                                before: s.outcome.clone(),
                                after: fibo_core::model::Outcome::Open,
                                at: card.turn_started,
                                reason: "新回合".into(),
                            });
                            s.outcome = fibo_core::model::Outcome::Open;
                        }
                        s.updated = s.updated.max(card.updated);
                        touched.push(s.id.clone());
                        next.content.insert(s.id.clone(), std::sync::Arc::new(card));
                    }
                }
            }
            // A `continued-in` line read just now moves its terminal to the new session.
            crate::recognition::follow_continued(&mut next, transcripts)?;
            if next.agent_sessions != host.workspace.agent_sessions
                || (touched.iter()).any(|id| next.content.get(id) != host.workspace.content.get(id))
            {
                host.commit(next)?;
            }
        }
        if needs_probe {
            self.inventory = running;
            for t in unbound {
                self.attempted.insert((t.path.clone(), t.cli_id.clone()));
            }
            refresh_agents(shared, &index.cached()?)?;
        }
        self.associate_screens(shared, index, transcripts)?;
        Ok(())
    }
    /// The last resort for any CLI: shared-daemon Codex runs its thread outside the PTY, and a
    /// hand-started Claude names no session and closes its transcript, so neither process nor
    /// open-file evidence exists. Bind only when a thread's newest prompt is on exactly one
    /// managed screen of an unbound Agent and that screen shows no other candidate thread.
    /// A thread only a screen tied to another pane can move (`codex resume` in a new pane):
    /// the new pane has the evidence for its newest prompt and the old one has none.
    pub(super) fn associate_screens(
        &mut self,
        shared: &Shared,
        index: &crate::log_index::Index,
        batch: &[crate::recognition::Transcript],
    ) -> io::Result<()> {
        let window = fibo_core::content::now().saturating_sub(600);
        let unbound = |workspace: &Workspace,
                       panes: &[super::screen_match::Pane],
                       released: &std::collections::HashSet<String>,
                       t: &crate::recognition::Transcript| {
            crate::recognition::screen_candidate(
                t.updated >= window,
                released.contains(&t.path),
                super::screen_match::holder(workspace, t),
                panes
                    .iter()
                    .any(|(.., runtime, current)| current.is_none() && *runtime == t.runtime),
            )
        };
        // Picked under the lock from the workspace itself: most batches stop at `fresh`
        // below, and a copy of the whole workspace made for them was thrown away.
        let (released, candidates) = {
            let mut host = lock(shared)?;
            let released = std::mem::take(&mut host.released_logs);
            let panes = super::screen_match::panes(&host);
            if panes.is_empty() {
                return Ok(());
            }
            let candidates: Vec<_> = (batch.iter())
                .filter(|t| unbound(&host.workspace, &panes, &released, t))
                .collect();
            (released, candidates)
        };
        // Only a prompt with comparisons left justifies reading screens.
        let mut fresh = !released.is_empty();
        for t in candidates {
            if let Some((text, at)) = index.latest_user(&t.path)? {
                let key = crate::recognition::screen_key(&text);
                let tries = self
                    .screened
                    .entry((t.path.clone(), key.unwrap_or_else(|| at.to_string())))
                    .or_default();
                if *tries < 5 {
                    *tries += 1;
                    fresh = true;
                }
            }
        }
        if !fresh {
            return Ok(());
        }
        let cached = index.cached()?;
        // Panes and workspace taken together again, as one consistent view.
        let (panes, workspace, submits) = {
            let host = lock(shared)?;
            let panes = super::screen_match::panes(&host);
            if panes.is_empty() {
                return Ok(());
            }
            (panes, host.workspace.clone(), host.submits.clone())
        };
        let unbound =
            |t: &crate::recognition::Transcript| unbound(&workspace, &panes, &released, t);
        let terminals = super::screen_match::terminals(index, &workspace, &panes, &submits)?;
        // What a thread began with is read only when a pane's Agent was started with it.
        let first = terminals.iter().any(|t| t.launched.is_some());
        let mut threads = vec![];
        for t in cached.iter().filter(|t| unbound(t)) {
            if let Some(thread) = super::screen_match::thread(index, &workspace, t, first)? {
                let held = super::screen_match::holder(&workspace, t);
                if !super::screen_match::stays(shared, &thread, held, &submits)? {
                    threads.push(thread);
                }
            }
        }
        let bindings = crate::recognition::screen_bindings(&threads, &terminals);
        super::screen_match::commit(shared, bindings, &panes, &cached)
    }
}
pub(super) fn recover_logs(
    shared: &Shared,
    index: &mut crate::log_index::Index,
    association: &mut Association,
    scopes: &[std::path::PathBuf],
) -> io::Result<()> {
    if scopes.is_empty() {
        return Ok(());
    }
    // Persist scope before repairing: restart resumes only this unfinished subtree.
    index.queue_recovery(scopes)?;
    publish_recognition(
        shared,
        index,
        RecognitionPhase::Interrupted,
        "识别中断：文件事件不完整",
        scopes,
    )?;
    publish_recognition(
        shared,
        index,
        RecognitionPhase::Recovering,
        "识别中断，正在补扫",
        scopes,
    )?;
    let mut recovery = crate::log_index::Recovery::new(index, scopes);
    loop {
        let (done, transcripts) = recovery.step(index)?;
        if let Err(error) = association.apply(shared, index, &transcripts) {
            eprintln!("Agent association deferred after subtree repair: {error}");
        }
        if done {
            break;
        }
        thread::yield_now();
    }
    // Asked first: finishing forgets what was in a folder that is gone.
    let lost = index.lost_logs(scopes)?;
    index.finish_recovery(scopes)?;
    let touched = index.take_touched();
    association.apply(shared, index, &touched)?;
    // A folder that held logs and went away is repaired once its logs are forgotten:
    // recognition works, so the phase is Healthy and the detail says what went. Left
    // Interrupted, nothing ever cleared it (no later recovery runs for a gone folder),
    // and each batch republished it for good.
    let detail = if lost {
        "识别正常；有记录过会话的日志目录被删除，记录已清理"
    } else {
        "识别正常"
    };
    publish_recognition(shared, index, RecognitionPhase::Healthy, detail, &[])
}
pub(super) fn event_scopes(
    batch: &fibo_platform::file_watch::Batch,
    roots: &[std::path::PathBuf],
    scopes: &mut Vec<std::path::PathBuf>,
) -> bool {
    let mut restart = false;
    for event in &batch.events {
        if event.flags & flag::IDS_WRAPPED != 0 {
            // IDs wrapped: the journal cannot identify a narrower lost range.
            for root in roots {
                fibo_platform::file_watch::add_scope(scopes, root, roots);
            }
            restart = true;
        } else if event.flags & flag::SCAN != 0 {
            let path = if event.path.extension().is_some_and(|e| e == "jsonl") {
                event.path.parent().unwrap_or(&event.path)
            } else {
                &event.path
            };
            fibo_platform::file_watch::add_scope(scopes, path, roots);
            restart |= event.flags & flag::ROOT_CHANGED != 0;
        } else if event.flags & flag::IS_DIR != 0
            && event.flags & (flag::CREATED | flag::RENAMED) != 0
        {
            // A new/renamed populated directory may produce only a directory event.
            fibo_platform::file_watch::add_scope(scopes, &event.path, roots);
        }
    }
    restart
}
// Bootstrap can establish ownership without any queued FSEvents batch. Install
// vnode listeners before the first blocking receive, too: open writers need not
// close their files to deliver FSEvents. Register then read closes the append race.
pub(super) fn follow_associated(
    shared: &Shared,
    index: &mut crate::log_index::Index,
    association: &mut Association,
    watch: &fibo_platform::file_watch::Watch,
) -> io::Result<()> {
    let (mut associated, unbound) = {
        let host = lock(shared)?;
        let associated = host
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| s.current)
            .map(|s| std::path::PathBuf::from(&s.transcript))
            .collect::<Vec<_>>();
        // As `screen_match::panes`: a pid whose terminal some evidence has bound since
        // stays listed until it exits, and screen association no longer looks at it.
        let unbound = (host.unbound_pids.iter())
            .filter(|(_, (terminal, _))| host.workspace.current_agent(terminal).is_none())
            .map(|(pid, _)| *pid)
            .collect::<Vec<_>>();
        (associated, unbound)
    };
    // A shared daemon (Codex's) keeps an unbound thread's log open, so FSEvents would hold
    // its prompt back until exit and screen association would never see it. While a managed
    // Agent is unbound, also follow recent unassociated logs of such Agents from its cwd.
    if !unbound.is_empty() {
        let cwds: Vec<_> = unbound
            .iter()
            .filter_map(|pid| fibo_platform::inspect::cwd(*pid))
            .collect();
        let window = fibo_core::content::now().saturating_sub(600);
        let mut candidates: Vec<_> = index
            .cached_since(window)?
            .into_iter()
            .filter(|t| {
                crate::agents::adapter(t.runtime).outside_pty()
                    && !associated.iter().any(|p| p.as_os_str() == t.path.as_str())
                    && fibo_platform::fs::canonical(std::path::Path::new(&t.cwd))
                        .is_ok_and(|c| cwds.iter().any(|cwd| c.as_os_str() == cwd.as_str()))
            })
            .collect();
        candidates.sort_by_key(|t| std::cmp::Reverse(t.updated));
        associated.extend(
            candidates
                .into_iter()
                .take(16)
                .map(|t| std::path::PathBuf::from(t.path)),
        );
    }
    // The same holds for a bound session's subagents: their logs stay open too.
    let parents: Vec<_> = {
        let host = lock(shared)?;
        host.workspace
            .agent_sessions
            .iter()
            .filter(|s| s.current && crate::agents::adapter(s.runtime).links_children())
            .map(|s| (s.runtime, s.cli_session_id.clone()))
            .collect()
    };
    if !parents.is_empty() {
        associated.extend(
            index
                .child_logs(&parents, 32)?
                .into_iter()
                .map(std::path::PathBuf::from),
        );
    }
    // Register first, then close the read/register race using the existing cursor.
    for path in watch.follow(&associated)? {
        if let Some(runtime) = index.runtime(&path) {
            let changed = index.update(runtime, &path, true)?;
            let mut found = index.take_touched();
            if let Some(t) = changed {
                found.insert(0, t);
            }
            Association::apply_removals(shared, index)?;
            association.apply(shared, index, &found)?;
        }
    }
    Ok(())
}
pub(super) fn start_subscription(shared: Shared, mut index: crate::log_index::Index) {
    watch_registries(&shared, &index.roots);
    thread::spawn(move || {
        // Any error restarts the subscription from the durable checkpoint, like a daemon
        // restart would; `initialize` clears the stored error once it is running again.
        let mut retry = SUBSCRIPTION_RETRY_MIN;
        loop {
            let started = Instant::now();
            let result = (|| -> io::Result<()> {
                if let Some(roots) = lock(&shared)?.new_roots.take() {
                    index.roots = roots;
                }
                let mut checkpoint = index.initialize()?;
                let roots = index
                    .roots
                    .iter()
                    .map(|(_, p)| p.clone())
                    .collect::<Vec<_>>();
                let mut watch = fibo_platform::file_watch::Watch::start(&roots, checkpoint)?;
                lock(&shared)?.log_events = Some(watch.notifier());
                index.bootstrap()?;
                Association::apply_removals(&shared, &index)?;
                publish_recognition(&shared, &index, RecognitionPhase::Healthy, "识别正常", &[])?;
                let mut association = Association::default();
                association.apply(&shared, &index, &index.cached()?)?;
                // A probe only follows new logs and panes. A host that just took over has
                // to find the processes of the conversations it inherited, too: a quiet
                // Agent sends no hook that would tell it.
                refresh_agents(&shared, &index.cached()?)?;
                follow_associated(&shared, &mut index, &mut association, &watch)?;
                loop {
                    // Asked before it waits as well as after: a reload that came while this
                    // subscription was starting had no notifier to wake it with.
                    if let Some(roots) = lock(&shared)?.new_roots.take() {
                        index.flush()?;
                        index.roots = roots;
                        return Ok(());
                    }
                    // Held cards wait at most a second for the database. With nothing held the
                    // subscriber blocks on events alone, so an idle host still never wakes.
                    let mut batch = loop {
                        let Some(due) = index.flush_due() else {
                            break watch.rx.recv().map_err(io::Error::other)?;
                        };
                        match watch.rx.recv_timeout(due) {
                            Ok(batch) => break batch,
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => index.flush()?,
                            Err(e) => return Err(io::Error::other(e)),
                        }
                    };
                    // Plugins changed which CLIs exist: start over with their directories. The
                    // checkpoint replays whatever this batch held.
                    if let Some(roots) = lock(&shared)?.new_roots.take() {
                        index.flush()?;
                        index.roots = roots;
                        return Ok(());
                    }
                    // A single bounded burst window starts ONLY after a native event arrives.
                    // This matches FSEvents' 200 ms latency for eager vnode writes too. It never
                    // wakes an idle subscriber, and continuous writers cannot extend the deadline.
                    let deadline = std::time::Instant::now() + Duration::from_millis(200);
                    for _ in 0..63 {
                        match watch.rx.recv_timeout(
                            deadline.saturating_duration_since(std::time::Instant::now()),
                        ) {
                            Ok(next) => batch.events.extend(next.events),
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                            Err(e) => return Err(io::Error::other(e)),
                        }
                    }
                    let mut scopes = watch.take_overflow();
                    let overflowed = !scopes.is_empty();
                    let restart = event_scopes(&batch, &roots, &mut scopes) || overflowed;
                    let wrapped = batch
                        .events
                        .iter()
                        .any(|e| e.flags & flag::IDS_WRAPPED != 0);
                    if restart {
                        // Watch before repairing. Replay from the last committed checkpoint, not 'now',
                        // so restarting one stream cannot discard queued events for the other CLI root.
                        if wrapped {
                            checkpoint = fibo_platform::file_watch::Watch::current_id();
                            index.reset_checkpoint(checkpoint)?;
                        }
                        watch = fibo_platform::file_watch::Watch::start(&roots, checkpoint)?;
                        lock(&shared)?.log_events = Some(watch.notifier());
                    }
                    if !scopes.is_empty() {
                        let repaired = recover_logs(&shared, &mut index, &mut association, &scopes);
                        Association::apply_removals(&shared, &index)?;
                        if let Err(error) = repaired {
                            index.fail(&error)?;
                            publish_recognition(
                                &shared,
                                &index,
                                RecognitionPhase::Interrupted,
                                &format!("补扫未完成：{error}；等待后续文件事件重试"),
                                &scopes,
                            )?;
                            // Leave the durable scope worklist. No busy retry or permanent error latch.
                            continue;
                        }
                    }
                    let mut paths = std::collections::BTreeSet::new();
                    let mut event_id = checkpoint;
                    for event in batch.events {
                        if !wrapped {
                            event_id = event_id.max(event.id);
                        }
                        // The index's adapter identifies logs and subagent metadata;
                        // a declared CLI need not use the built-ins' extension.
                        if event.flags & flag::HISTORY_DONE == 0 {
                            paths.insert(event.path);
                        }
                    }
                    let transcripts = index.update_many(paths, event_id)?;
                    // Unlike metadata association, a failed removal must wake the
                    // existing subscription retry even if no more events arrive.
                    Association::apply_removals(&shared, &index)?;
                    if let Err(error) = association.apply(&shared, &index, &transcripts) {
                        eprintln!("Agent association deferred: {error}");
                    }
                    follow_associated(&shared, &mut index, &mut association, &watch)?;
                    // Only a real event drives retries of an earlier incomplete subtree.
                    let pending = index.recovery_scopes()?;
                    if !pending.is_empty() {
                        let repaired =
                            recover_logs(&shared, &mut index, &mut association, &pending);
                        Association::apply_removals(&shared, &index)?;
                        if let Err(error) = repaired {
                            index.fail(&error)?;
                            publish_recognition(
                                &shared,
                                &index,
                                RecognitionPhase::Interrupted,
                                &format!("补扫未完成：{error}"),
                                &pending,
                            )?;
                            continue;
                        }
                    }
                    if event_id > checkpoint {
                        // Checkpoint is atomic with this batch of content/cursor writes.
                        checkpoint = event_id;
                    }
                    let status = lock(&shared)?.recognition.clone();
                    publish_recognition(
                        &shared,
                        &index,
                        status.phase,
                        &status.detail,
                        &status
                            .scopes
                            .iter()
                            .map(std::path::PathBuf::from)
                            .collect::<Vec<_>>(),
                    )?;
                }
            })();
            let Err(error) = result else {
                retry = SUBSCRIPTION_RETRY_MIN;
                continue;
            };
            if started.elapsed() > SUBSCRIPTION_RETRY_MAX {
                retry = SUBSCRIPTION_RETRY_MIN;
            }
            if let Err(persist) = index.fail(&error) {
                eprintln!("persist subscription failure: {persist}");
            }
            let _ = publish_recognition(
                &shared,
                &index,
                RecognitionPhase::Interrupted,
                &format!("识别中断：{error}；{} 秒后自动重试", retry.as_secs()),
                &[],
            );
            eprintln!("log subscription failed, retrying in {retry:?}: {error}");
            thread::sleep(retry);
            retry = (retry * 2).min(SUBSCRIPTION_RETRY_MAX);
        }
    });
}

/// Backoff for restarting a failed log subscription or registry watch.
pub(super) const SUBSCRIPTION_RETRY_MIN: Duration = Duration::from_secs(1);
pub(super) const SUBSCRIPTION_RETRY_MAX: Duration = Duration::from_secs(60);
