//! Comparing a thread's newest prompt with terminal screens, for the log watch and for a
//! user asking which conversation a pane is running.
use super::*;
use crate::recognition::{ScreenTerminal, ScreenThread, Transcript};

/// A pane that may take a thread: its Agent's pid and its terminal, and the session a
/// screen alone tied to it, if any.
pub(super) type Pane = (u32, LiveTerminal, String, Agent, Option<AgentSession>);

pub(super) fn panes(host: &Host) -> Vec<Pane> {
    let mut panes = vec![];
    for (pid, (terminal, runtime)) in &host.unbound_pids {
        if host.workspace.current_agent(terminal).is_none() {
            panes.push((*pid, terminal.clone(), *runtime, None));
        }
    }
    // A session only the screen tied to its pane can move to another thread in the
    // same TUI; one its command line, hook or open file names cannot.
    for (pid, id) in &host.agent_pids {
        if let Some(s) = host.workspace.agent_sessions.iter().find(|s| {
            &s.id == id
                && s.current
                && s.terminal.is_some()
                && s.binding.is_none_or(|b| b == Binding::Screen)
        }) {
            panes.push((
                *pid,
                s.terminal.clone().unwrap(),
                s.runtime,
                Some(s.clone()),
            ));
        }
    }
    panes.retain(|(_, t, ..)| host.terminals.contains_key(t));
    panes
        .into_iter()
        .map(|(pid, t, runtime, s)| (pid, host.terminals[&t].clone(), t, runtime, s))
        .collect()
}
pub(super) fn holder<'a>(workspace: &'a Workspace, t: &Transcript) -> Option<&'a AgentSession> {
    workspace
        .agent_sessions
        .iter()
        .find(|s| s.current && s.runtime == t.runtime && s.cli_session_id == t.cli_id)
}
/// A thread as evidence, or None while its log holds no prompt.
pub(super) fn thread<'a>(
    index: &crate::log_index::Index,
    t: &'a Transcript,
) -> io::Result<Option<ScreenThread<'a>>> {
    let Some((text, at)) = index.latest_user(&t.path)? else {
        return Ok(None);
    };
    Ok(Some(ScreenThread {
        transcript: t,
        key: crate::recognition::screen_key(&text),
        at: Some(at),
        began: index.turn_began(&t.path, at)?,
        cwd: mooring_platform::fs::canonical(std::path::Path::new(&t.cwd))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| t.cwd.clone()),
    }))
}
type Submits = HashMap<String, std::collections::VecDeque<u64>>;
fn presses(submits: &Submits, terminal: &str) -> Vec<u64> {
    submits
        .get(terminal)
        .map(|s| s.iter().copied().collect())
        .unwrap_or_default()
}
/// Still on its own pane (both TUIs can follow one thread): it stays there.
pub(super) fn stays(
    shared: &Shared,
    thread: &ScreenThread,
    holder: Option<&AgentSession>,
    submits: &Submits,
) -> io::Result<bool> {
    let Some(pane) = holder.and_then(|s| s.terminal.as_ref()) else {
        return Ok(false);
    };
    let live = lock(shared)?.terminals.get(pane).cloned();
    let screen = match live {
        Some(live) => lock(&live)?.vt.tail_text(200)?,
        None => String::new(),
    };
    Ok(crate::recognition::prompt_evidence(
        thread,
        &crate::recognition::screen_text(&screen),
        &presses(submits, pane),
    ))
}
/// A terminal is one screen however many of its processes look like the CLI: a helper caught
/// between fork and exec still has the name, and stays counted until it exits. Listed twice, the
/// screen would be two panes fitting one thread, which binds nothing. The oldest stands for it.
fn one_per_terminal<T>(items: &[T], of: impl Fn(&T) -> (u32, &str)) -> Vec<&T> {
    let mut oldest: HashMap<&str, &T> = HashMap::new();
    for item in items {
        let (pid, terminal) = of(item);
        oldest
            .entry(terminal)
            .and_modify(|kept| {
                if pid < of(kept).0 {
                    *kept = item;
                }
            })
            .or_insert(item);
    }
    oldest.into_values().collect()
}
pub(super) fn terminals(
    index: &crate::log_index::Index,
    panes: &[Pane],
    submits: &Submits,
) -> io::Result<Vec<ScreenTerminal>> {
    let mut terminals = vec![];
    for (pid, live, terminal, runtime, current) in
        one_per_terminal(panes, |(pid, _, terminal, ..)| (*pid, terminal))
    {
        let screen = lock(live)?.vt.tail_text(200)?;
        let current = match current {
            Some(s) => Some((
                s.cli_session_id.clone(),
                s.updated,
                index
                    .latest_user_text(&s.transcript)?
                    .and_then(|p| crate::recognition::screen_key(&p)),
            )),
            None => None,
        };
        terminals.push(ScreenTerminal {
            terminal: terminal.clone(),
            runtime: *runtime,
            cwd: mooring_platform::inspect::cwd(*pid),
            screen: crate::recognition::screen_text(&screen),
            current,
            submits: presses(submits, terminal),
        });
    }
    Ok(terminals)
}
/// Ties each thread to its pane. Only a screen-tied thread gets here while bound: the
/// evidence ended that link.
pub(super) fn commit(
    shared: &Shared,
    bindings: Vec<(String, &Transcript)>,
    panes: &[Pane],
    cached: &[Transcript],
) -> io::Result<()> {
    if bindings.is_empty() {
        return Ok(());
    }
    let mut host = lock(shared)?;
    let mut next = host.workspace.clone();
    let mut bound = vec![];
    let mut moved = false;
    for (terminal, t) in bindings {
        if let Some(old) = next
            .agent_sessions
            .iter_mut()
            .find(|s| s.current && s.runtime == t.runtime && s.cli_session_id == t.cli_id)
            .filter(|s| s.terminal.as_ref() != Some(&terminal))
        {
            old.current = false;
        }
        if next
            .observe_agent(crate::recognition::bound(
                t,
                terminal.clone(),
                Binding::Screen,
            ))
            .is_ok()
        {
            bound.push(terminal);
        }
    }
    if next.agent_sessions == host.workspace.agent_sessions {
        return Ok(());
    }
    host.commit(next)?;
    // The TUI pid now stands for the bound session: its exit clears `current`.
    for terminal in bound {
        let Some(id) = host
            .workspace
            .current_agent(&terminal)
            .map(|s| s.id.clone())
        else {
            continue;
        };
        let pids: Vec<_> = panes
            .iter()
            .filter(|(_, _, t, ..)| t == &terminal)
            .map(|(pid, ..)| *pid)
            .collect();
        // The pane it left no longer stands for it: that TUI's exit must not end this
        // session, and it is an unbound Agent again.
        let before = host.agent_pids.len();
        host.agent_pids
            .retain(|pid, session| session != &id || pids.contains(pid));
        moved |= host.agent_pids.len() != before;
        for pid in pids {
            host.unbound_pids.remove(&pid);
            host.agent_pids.insert(pid, id.clone());
        }
        host.unhint(&terminal);
    }
    host.publish_status()?;
    drop(host);
    if moved {
        refresh_agents(shared, cached)?;
    }
    Ok(())
}
/// Both are whole seconds, so a log last written in the second its TUI started may be from
/// before it: that is a guess, and an association never guesses. The thread a TUI really runs
/// is written again when its prompt arrives, later than that.
fn touched_since(updated: u64, started: u64) -> bool {
    updated > started
}
/// `Request::AssociateScreen`: which conversation an unbound pane shows, tied to it when the
/// screen says so beyond doubt. Only a thread touched since this TUI started counts: an older
/// one whose words merely appear on the screen is not what it runs. Never a guess: a thread
/// fitting two panes, or a pane fitting two threads, answers None.
pub(super) fn associate_pane(shared: &Shared, id: &str) -> io::Result<Reply> {
    let (panes, workspace, submits, index) = {
        let host = lock(shared)?;
        (
            panes(&host),
            host.workspace.clone(),
            host.submits.clone(),
            host.index.fork_current()?,
        )
    };
    let answer = |session: Option<String>| Ok(Reply::ScreenAssociated { session });
    if let Some(s) = workspace.current_agent(id) {
        return answer(Some(s.id.clone()));
    }
    let Some((pid, _, _, runtime, _)) = panes.iter().find(|(_, _, t, ..)| t == id) else {
        return answer(None);
    };
    let Some(started) = mooring_platform::inspect::started(*pid) else {
        return answer(None);
    };
    let cached = index.cached()?;
    let mut threads = vec![];
    for t in cached
        .iter()
        .filter(|t| t.runtime == *runtime && touched_since(t.updated, started))
    {
        let held = holder(&workspace, t);
        if !crate::recognition::screen_candidate(true, false, held, true) {
            continue;
        }
        if let Some(thread) = thread(&index, t)? {
            if !stays(shared, &thread, held, &submits)? {
                threads.push(thread);
            }
        }
    }
    let terminals = terminals(&index, &panes, &submits)?;
    let bindings = crate::recognition::screen_bindings(&threads, &terminals)
        .into_iter()
        .filter(|(terminal, _)| terminal == id)
        .collect();
    commit(shared, bindings, &panes, &cached)?;
    answer(
        lock(shared)?
            .workspace
            .current_agent(id)
            .map(|s| s.id.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::touched_since;

    #[test]
    fn a_log_written_in_the_second_its_tui_started_is_not_evidence() {
        assert!(!touched_since(100, 101));
        assert!(!touched_since(100, 100));
        assert!(touched_since(101, 100));
    }
}

#[cfg(test)]
mod one_per_terminal_tests {
    #[test]
    fn a_terminal_listed_twice_comes_back_once_with_its_oldest_process() {
        let panes = [(9, "a"), (3, "a"), (5, "b"), (7, "a")];
        let mut one = super::one_per_terminal(&panes, |(pid, t)| (*pid, t));
        one.sort();
        assert_eq!(one, [&(3, "a"), &(5, "b")]);
    }
}
