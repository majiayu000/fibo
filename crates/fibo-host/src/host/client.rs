//! One client connection: request dispatch, typed input and handing the socket to a
//! newer host.
use super::*;
use fibo_platform::local;
mod edit;
mod hook;
mod last_dir;
mod launch;
mod modes;
mod once;
mod recover_terminal;
mod stop;
mod suggestion;
mod tool_rows;
mod turn_changes;
mod typing;
mod worktrees;
pub(super) use typing::*;

/// How long writing a reply may go without a byte taken before that client is given up. A
/// write that moves at all keeps going: another machine's window reads a 3 MB workspace
/// through an ssh forward that slows to tens of KB/s and stalls for seconds at a time, and
/// 5 seconds cut it off every time (2026-11-16). A client that stops reading holds only its
/// own thread meanwhile; no lock is held while a reply is written.
pub(super) const REPLY_STALL: Duration = Duration::from_secs(30);

pub(super) fn serve_client(
    mut stream: local::Stream,
    host: Shared,
    connection: u64,
) -> io::Result<()> {
    if !stream.peer_is_this_account()? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "peer uid mismatch",
        ));
    }
    stream.set_write_timeout(Some(REPLY_STALL))?;
    match receive::<Request>(&mut stream)? {
        Request::Hello { major: MAJOR } => send(
            &mut stream,
            CONTROL,
            &Reply::Hello {
                major: MAJOR,
                instance: lock(&host)?.instance.clone(),
                build: BUILD.clone(),
            },
        )?,
        _ => {
            send(
                &mut stream,
                CONTROL,
                &Reply::Error(format!("protocol major {MAJOR} required")),
            )?;
            return Ok(());
        }
    }
    loop {
        let request = match request_frame(&mut stream)? {
            Ok(request) => request,
            Err(error) => {
                send(&mut stream, CONTROL, &Reply::Error(error))?;
                continue;
            }
        };
        if matches!(request, Request::WatchRecognition) {
            return watch_recognition(stream, host, connection);
        }
        if matches!(request, Request::Subscribe) {
            return super::events::subscribe(stream, host, connection);
        }
        if matches!(request, Request::Diagnose) {
            let window = crate::diagnostics::window_pid(&stream);
            // Copy only the live terminal facts. Sampling never holds the workspace lock.
            if let Err(error) = crate::diagnostics::serve(&mut stream, window, || {
                let host = lock(&host)?;
                Ok(crate::diagnostics::terminal_facts(
                    &host.workspace,
                    &host.recognition.terminal_agents,
                ))
            }) {
                send(&mut stream, CONTROL, &Reply::Error(error.to_string()))?;
            }
            return Ok(());
        }
        if matches!(
            request,
            Request::ScanWorktrees | Request::ReclaimWorktree { .. }
        ) {
            let result = worktrees::run(&stream, &host, request);
            if !stream.is_open() {
                return Ok(());
            }
            return send(
                &mut stream,
                CONTROL,
                &result.unwrap_or_else(|e| Reply::Error(e.to_string())),
            );
        }
        if matches!(request, Request::Handoff) {
            return hand_over(stream, &host);
        }
        if let Request::Attach {
            id,
            takeover,
            watch,
        } = request
        {
            let host_state = lock(&host)?;
            if let Some(terminal) = host_state.terminals.get(&id).cloned() {
                drop(host_state);
                return attach(
                    stream,
                    terminal,
                    connection,
                    takeover && !watch,
                    watch,
                    host,
                    id,
                );
            }
            let reason = if host_state
                .workspace
                .terminals
                .iter()
                .any(|t| t.id == id && t.state == ProcessState::Exited)
            {
                tk!("进程已退出")
            } else {
                tk!("终端不存在")
            };
            drop(host_state);
            send(&mut stream, CONTROL, &Reply::Error(reason.into()))?;
            continue;
        }
        let result = super::brief::dispatch(&host, request);
        let reply = result.unwrap_or_else(|e| Reply::Error(e.to_string()));
        send(&mut stream, CONTROL, &reply)?;
    }
}

/// A complete frame with an unsupported request is recoverable; a truncated frame is not.
pub(super) fn request_frame(stream: &mut local::Stream) -> io::Result<Result<Request, String>> {
    let (kind, bytes) = read_frame(stream)?;
    Ok(if kind == CONTROL {
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("unsupported or invalid request: {error}"))
    } else {
        Err("expected control frame".into())
    })
}

/// A newer host takes over: every running terminal goes to it as a snapshot plus its PTY
/// descriptor, and this host exits without signalling the programs. Global and terminal
/// locks stay held from the snapshots onward, so no output is fed, no state is committed
/// and no terminal is started after the snapshots are taken.
pub(super) fn hand_over(stream: local::Stream, shared: &Shared) -> io::Result<()> {
    if !term::CAN_HAND_OVER {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this system cannot hand a running terminal over",
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let host = lock(shared)?;
        let mut held = vec![];
        let mut paused = vec![];
        let mut pending = None;
        for (id, terminal) in &host.terminals {
            let live = lock(terminal)?;
            if live.exited {
                continue;
            }
            let Some(pause) = live.input.pause_if_idle()? else {
                pending = Some(live.input.clone());
                break;
            };
            paused.push(pause);
            held.push((id.clone(), live));
        }
        let Some(pending) = pending else {
            let result = hand_over_paused(stream, &mut held);
            // On failure, resume replies before the read pumps can take the terminal locks.
            drop(paused);
            return result;
        };
        // A write may be waiting for a program to finish printing. Leave the old host
        // and all its terminals free to read and reply while the input drains.
        drop(paused);
        drop(held);
        drop(host);
        pending.wait_until_idle(deadline)?;
    }
}

fn hand_over_paused(
    mut stream: local::Stream,
    held: &mut [(String, std::sync::MutexGuard<'_, Live>)],
) -> io::Result<()> {
    let terminals: Vec<_> = held
        .iter()
        .map(|(id, live)| Handed {
            id: id.clone(),
            pid: live.pid,
            cols: live.cols,
            rows: live.rows,
            title: live.title.clone(),
            cell: live.cell.get(),
        })
        .collect();
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    send(&mut stream, CONTROL, &Reply::Handoff { terminals })?;
    for (_, live) in held.iter_mut() {
        for chunk in live.vt.snapshot()?.chunks(MAX_FRAME - 1) {
            write_frame(&mut stream, SNAPSHOT, chunk)?;
        }
        write_frame(&mut stream, READY, &[])?;
    }
    let masters: Vec<_> = held.iter().map(|(_, live)| &live.master).collect();
    term::send_masters(&stream, &masters)?;
    let count = masters.len();
    // Only a confirmed takeover lets this host go: closing our copies before the new host
    // holds its own would hang up every terminal. Anything else keeps serving.
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    match read_frame(&mut stream) {
        Ok((ACK, taken)) if taken == HANDOFF_TAKEN => {
            eprintln!("handed {count} terminals to a newer host; exiting");
            std::process::exit(0);
        }
        other => Err(io::Error::other(format!(
            "handoff not confirmed, still serving: {other:?}"
        ))),
    }
}
/// What a new host answers once it holds every handed-over descriptor.
pub(super) const HANDOFF_TAKEN: &[u8] = b"taken";

/// The host's relay supervisor, taken out of the lock: it calls back into the host.
fn supervisor(shared: &Shared) -> io::Result<std::sync::Arc<super::remote::Supervisor>> {
    lock(shared)?
        .remote
        .clone()
        .ok_or_else(|| io::Error::other(t!("远程还没准备好")))
}

pub(super) fn dispatch(shared: &Shared, request: Request) -> io::Result<Reply> {
    match request {
        Request::RecognitionStatus => Ok(Reply::Recognition(recognition_status(&*lock(shared)?))),
        Request::Quotas => {
            // Plugins' files and commands are read outside the lock.
            fibo_core::quota::install();
            let plugins = fibo_core::quota::from_plugins();
            let now = fibo_core::content::now();
            let host = lock(shared)?;
            let all = fibo_core::quota::latest(&host.workspace, &plugins, now);
            Ok(Reply::Quotas(all))
        }
        Request::RefreshAgents => {
            let transcripts = {
                let host = lock(shared)?;
                if let Some(error) = host.store.subscription_error()? {
                    return Err(io::Error::other(error));
                }
                host.transcripts.clone()
            };
            refresh_agents(shared, &super::status::all_transcripts(&transcripts)?)?;
            let mut host = lock(shared)?;
            let waiting: Vec<_> = host
                .workspace
                .agent_sessions
                .iter()
                .filter(|s| s.current)
                .filter(|s| {
                    host.workspace
                        .content
                        .get(&s.id)
                        .is_some_and(|c| c.turn == fibo_core::content::Turn::Waiting)
                })
                .cloned()
                .collect();
            if !waiting.is_empty() {
                host.reconcile(shared, waiting);
            }
            Ok(Reply::Workspace(host.snapshot()))
        }
        request @ Request::Hook { .. } => hook::dispatch(shared, request),
        Request::ClaimNotification { session, key } => {
            let host = lock(shared)?;
            if !host.workspace.settings.notifications
                || !host
                    .workspace
                    .agent_sessions
                    .iter()
                    .any(|s| s.id == session)
            {
                return Ok(Reply::NotificationClaimed(false));
            }
            Ok(Reply::NotificationClaimed(
                host.store.claim_notification(&session, &key)?,
            ))
        }
        Request::ReloadPlugins => {
            fibo_core::plugin::reload();
            let home = fibo_platform::dirs::home().unwrap_or_default();
            crate::agents::mirror::sync(&home);
            let mut host = lock(shared)?;
            // A CLI that arrived or left changes which directories are read. The subscription
            // restarts from its checkpoint with the new roots, and walks the new one once.
            let roots = crate::agents::roots_after_reload(&host.index.roots, &home);
            if roots != host.index.roots {
                host.index.roots = roots.clone();
                host.new_roots = Some(roots);
                if let Some(events) = &host.log_events {
                    events.file(std::path::PathBuf::new());
                }
            }
            host.recognition.plugins_revision += 1;
            host.publish_status()?;
            Ok(Reply::Ok)
        }
        Request::ClientLeft { at } => {
            let mut host = lock(shared)?;
            let mut next = host.workspace.clone();
            next.last_left = at;
            host.commit(next)?;
            Ok(Reply::Ok)
        }
        Request::Workspace => Ok(Reply::Workspace(lock(shared)?.snapshot())),
        Request::SplitRoom {
            target,
            axis,
            share,
        } => {
            let (cols, rows) = split_room(&*lock(shared)?, &target, axis, share)?;
            Ok(Reply::Size { cols, rows })
        }
        Request::Edit { edit, request } => edit::edit(shared, edit, request),
        Request::Once { request, inner } => once::once(shared, request, *inner),
        request @ Request::Launch { .. } => launch::dispatch(shared, request),
        request @ (Request::Chat { .. }
        | Request::ToolCalls { .. }
        | Request::ToolCall { .. }
        | Request::Backfill { .. }) => tool_rows::dispatch(shared, request),
        Request::SearchMessages { query, sessions } => {
            let index = { lock(shared)?.index.forker()? };
            let (messages, more) = index.open()?.search_messages(&query, &sessions)?;
            Ok(Reply::MessageMatches { messages, more })
        }
        Request::TurnChanges { session } => turn_changes::read(shared, &session),
        Request::TurnFile {
            session,
            log,
            path,
            lines,
        } => turn_changes::read_file(shared, &session, log, &path, &lines),
        Request::ReadScreen { id, rows } => {
            let host = lock(shared)?;
            let terminal = host
                .terminals
                .get(&id)
                .ok_or_else(|| io::Error::other("terminal not found"))?;
            let text = lock(terminal)?.vt.tail_text(rows.clamp(1, 10_000))?;
            Ok(Reply::Text(text))
        }
        Request::InputSuggestion { id } => suggestion::read(shared, &id),
        Request::Modes(ask) => modes::run(shared, ask),
        Request::AssociateScreen { id } => super::screen_match::associate_pane(shared, &id),
        Request::HandingOver { id, start } => {
            super::launch_probe::handing_over(shared, &id, start).map(|()| Reply::Ok)
        }
        Request::SendText {
            id,
            text,
            submit,
            session,
        } => send_text(shared, &id, &text, submit, session.as_deref()).map(|()| Reply::Ok),
        Request::SendTurn(input) => send_turn(
            shared,
            &input.id,
            &input.text,
            input.steer,
            input.session.as_deref(),
        )
        .map(|()| Reply::Ok),
        Request::ReportTo { child, parent } => {
            let mut host = lock(shared)?;
            for id in [&child, &parent] {
                if !host.terminals.contains_key(id) {
                    return Err(io::Error::other(t!("终端 {id} 不存在", id)));
                }
            }
            if child == parent {
                return Err(io::Error::other(t!("不能让终端向自己汇报")));
            }
            let w = host.snapshot();
            host.report_back.link(&w, &child, &parent);
            Ok(Reply::Ok)
        }
        Request::AnswerApproval {
            id,
            question,
            allow,
        } => {
            answer_approval(shared, &id, &question, allow, false)?;
            Ok(Reply::Ok)
        }
        Request::AnswerAlways { id, question } => {
            answer_approval(shared, &id, &question, true, true)?;
            Ok(Reply::Ok)
        }
        Request::AnswerTrust { id, folder, trust } => {
            answer_trust(shared, &id, &folder, trust)?;
            Ok(Reply::Ok)
        }
        Request::SendKeys { id, keys } => {
            send_keys(shared, &id, &keys)?;
            Ok(Reply::Ok)
        }
        Request::SendInput { id, bytes } => {
            type_input(shared, &id, &bytes)?;
            Ok(Reply::Ok)
        }
        Request::Interrupt { id } => {
            interrupt(shared, &id)?;
            Ok(Reply::Ok)
        }
        Request::AwaitInterrupt { id } => stop::await_interrupt(shared, &id),
        Request::RemoteConfigure {
            enabled,
            relay,
            key,
            also,
        } => {
            supervisor(shared)?.configure(enabled, relay, key, also)?;
            Ok(Reply::Ok)
        }
        Request::RemotePair { name, role } => {
            let remote = supervisor(shared)?;
            let relay = remote.relay();
            if relay.is_empty() {
                return Err(io::Error::other(t!("先填中继地址")));
            }
            let role = crate::devices::parse_role(&role)?;
            let name = Some(name.trim())
                .filter(|n| !n.is_empty())
                .unwrap_or(t!("手机"));
            let alts: Vec<String> = remote.also().into_iter().map(|a| a.relay).collect();
            let (_, link) = crate::devices::pair_link(&remote.socket(), &relay, &alts, name, role)?;
            remote.refresh();
            Ok(Reply::Text(link))
        }
        Request::RemoteDevice { device, change } => {
            let remote = supervisor(shared)?;
            let socket = remote.socket();
            match change {
                RemoteChange::Role(role) => {
                    crate::devices::set_role(&socket, &device, crate::devices::parse_role(&role)?)?;
                }
                RemoteChange::Kick => {
                    crate::devices::kick(&socket, &device)?;
                }
                RemoteChange::Revoke => {
                    crate::devices::remove(&socket, &device)?;
                }
                RemoteChange::Refresh => (),
            }
            remote.refresh();
            Ok(Reply::Ok)
        }
        Request::RemoteReport(live) => {
            supervisor(shared)?.report(live);
            Ok(Reply::Ok)
        }
        Request::SaveImage { data } => {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(io::Error::other)?;
            let path = crate::pictures::save(&supervisor(shared)?.socket(), &bytes)?;
            Ok(Reply::Text(path.to_string_lossy().into_owned()))
        }
        Request::ListDirs { path } => Ok(Reply::Dirs(crate::folders::list(&path)?)),
        Request::MakeDir { path } => {
            let made = crate::folders::make_dir(&path)?;
            Ok(Reply::Text(made.to_string_lossy().into_owned()))
        }
        Request::TerminalCwd { id } => {
            let host = lock(shared)?;
            let terminal = host
                .terminals
                .get(&id)
                .ok_or_else(|| io::Error::other("terminal not found"))?;
            let live = lock(terminal)?;
            if live.exited {
                return Err(io::Error::other("terminal has exited"));
            }
            // What the shell reports (OSC 7) is the truth for it; the process directory
            // is the fallback for a shell without the integration.
            if let Some(path) = live.vt.pwd(&hostname()).filter(|p| Path::new(p).is_dir()) {
                return Ok(Reply::Cwd(path));
            }
            let pid = host
                .workspace
                .terminals
                .iter()
                .find(|t| t.id == id)
                .and_then(|t| t.pid)
                .ok_or_else(|| io::Error::other("terminal PID missing"))?;
            // Explicit split action only, querying this managed shell, never polling.
            let path = fibo_platform::inspect::cwd(pid).ok_or_else(io::Error::last_os_error)?;
            Ok(Reply::Cwd(path))
        }
        Request::StopAgent { id } => {
            let host = lock(shared)?;
            let session = host
                .workspace
                .current_agent(&id)
                .ok_or_else(|| io::Error::other(t!("没有已识别的活动 Agent")))?;
            let pid = host
                .agent_pids
                .iter()
                .find(|(_, sid)| *sid == &session.id)
                .map(|(pid, _)| *pid)
                .ok_or_else(|| io::Error::other(t!("Agent 进程身份未确认，未发送停止信号")))?;
            let processes = crate::recognition::processes(&host.workspace)?;
            if !processes
                .iter()
                .any(|p| p.pid == pid && p.runtime == Some(session.runtime))
                || crate::recognition::terminal_for_process(pid, &processes, &host.workspace)
                    .as_deref()
                    != Some(id.as_str())
            {
                return Err(io::Error::other(t!("Agent 进程身份已变化，未发送停止信号")));
            }
            // A freshly verified Agent PID only; never kill the PTY shell or group.
            fibo_platform::process::terminate(pid as i32)?;
            Ok(Reply::Ok)
        }
        Request::RecoverTerminal { id, place } => recover_terminal::run(shared, &id, place),
        Request::Stop { id } => {
            let terminal = lock(shared)?
                .terminals
                .get(&id)
                .cloned()
                .ok_or_else(|| io::Error::other("terminal not found"))?;
            stop_terminal(&terminal)?;
            Ok(Reply::Ok)
        }
        _ => Err(io::Error::other("operation requires an attached terminal")),
    }
}

/// Room for both halves of a split of `target`, at the size it is shown now, and the
/// new pane's size, so it starts close to how it will show. A terminal this host does
/// not run has no size to judge; the edit refuses it.
fn split_room(
    host: &Host,
    target: &str,
    axis: fibo_core::layout::Axis,
    share: u16,
) -> io::Result<(u16, u16)> {
    let Some(terminal) = host.terminals.get(target) else {
        return Err(io::Error::other(t!("终端不存在")));
    };
    let (cols, rows) = {
        let live = lock(terminal)?;
        (live.cols, live.rows)
    };
    fibo_core::layout::Layout::split_room(cols, rows, axis, share).map_err(io::Error::other)
}
