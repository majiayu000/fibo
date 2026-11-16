//! Handles the hook request.
use super::*;

pub(super) fn dispatch(shared: &Shared, request: Request) -> io::Result<Reply> {
    match request {
        Request::Hook {
            runtime,
            pid,
            cli_session_id,
            transcript,
            ended,
            event,
            facts,
            terminal,
            cwd,
        } => {
            let (workspace, mut index) = {
                let host = lock(shared)?;
                (host.workspace.clone(), host.index.fork_current()?)
            };
            let processes = crate::recognition::processes(&workspace)?;
            // Reject unrelated callers before opening any supplied transcript. A hook whose
            // caller already exited still names its terminal through the environment.
            let named = terminal.as_deref().filter(|id| {
                workspace
                    .terminals
                    .iter()
                    .any(|t| &t.id == id && t.state == ProcessState::Running)
            });
            if crate::recognition::terminal_for_process(pid, &processes, &workspace).is_none()
                && named.is_none()
                && crate::recognition::continued_terminal(&workspace, runtime, &cli_session_id)
                    .is_none()
            {
                return Err(io::Error::other(
                    "Hook process is not a descendant of a managed terminal",
                ));
            }
            let path = if transcript.is_empty() {
                let matches = index.paths_for(runtime, &cli_session_id)?;
                if matches.len() != 1 {
                    return Err(io::Error::other(
                        "Hook transcript absent or ambiguous in index",
                    ));
                }
                std::path::PathBuf::from(&matches[0])
            } else {
                fibo_platform::file_watch::normalized(Path::new(&transcript))?
            };
            // Grok reads Claude's settings and runs its hooks as `--agent claude`; such a
            // hook would bind a Grok conversation as Claude and stamp Claude's events on it.
            if let Some(owner) = crate::recognition::transcript_owner(&path, &index.roots)
                .filter(|owner| *owner != runtime)
            {
                return Err(io::Error::other(format!(
                    "Hook for {} names a {} transcript",
                    runtime.name(),
                    owner.name()
                )));
            }
            // A new Claude announces itself before its first prompt creates the transcript:
            // the hook's own identity binds it now, and the index fills it in once written.
            let written = path.exists();
            let observed = if written {
                index
                    .update(runtime, &path, true)?
                    .ok_or_else(|| io::Error::other("transcript has no CLI identity"))?
            } else if !cwd.is_empty() && !transcript.is_empty() {
                crate::recognition::Transcript {
                    runtime,
                    cli_id: cli_session_id.clone(),
                    path: path.to_string_lossy().into_owned(),
                    cwd: cwd.clone(),
                    updated: fibo_core::content::now(),
                }
            } else {
                return Err(io::Error::other("Hook transcript not written yet"));
            };
            let mut host = lock(shared)?;
            let mut next = host.workspace.clone();
            crate::recognition::apply_hook(
                &mut next,
                runtime,
                pid,
                named,
                &cli_session_id,
                &observed,
                &processes,
                ended,
            )?;
            if let Some(event) = event.filter(|_| written) {
                index.hook(&observed.path, &event, &facts)?;
            }
            if let Some(s) = next
                .agent_sessions
                .iter()
                .find(|s| s.runtime == runtime && s.cli_session_id == cli_session_id)
            {
                if let Some(card) = index.card(&observed.path)? {
                    next.content.insert(s.id.clone(), card);
                }
                if written {
                    host.shell_watch.listed(&s.id, &facts);
                }
            }
            host.commit(next)?;
            let mut ancestor = pid;
            let mut hook_files = HashMap::new();
            while let Some(process) = processes.iter().find(|p| p.pid == ancestor) {
                if process.runtime == Some(runtime) {
                    hook_files.insert(ancestor, vec![observed.path.clone()]);
                    break;
                }
                if ancestor == process.parent {
                    break;
                }
                ancestor = process.parent;
            }
            // A hook run in the background has left its CLI's process tree; the terminal
            // it names then has to hold exactly one such CLI for its exit to be watched.
            if hook_files.is_empty() && !ended {
                let mut runs = processes.iter().filter(|p| {
                    p.runtime == Some(runtime)
                        && !processes
                            .iter()
                            .any(|c| c.parent == p.pid && c.runtime == p.runtime)
                        && named.is_some_and(|named| {
                            crate::recognition::terminal_for_process(p.pid, &processes, &workspace)
                                .as_deref()
                                == Some(named)
                        })
                });
                if let (Some(only), None) = (runs.next(), runs.next()) {
                    hook_files.insert(only.pid, vec![observed.path.clone()]);
                }
            }
            host.observe_processes(&processes, &hook_files)?;
            if let Some(events) = &host.log_events {
                events.file(path);
            }
            Ok(Reply::Workspace(host.snapshot()))
        }
        _ => unreachable!("request routed by client::dispatch"),
    }
}
