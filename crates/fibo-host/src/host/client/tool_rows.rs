//! A conversation's rows for a phone. `Request::Chat` reads them under the host's lock;
//! `ToolCalls`, `ToolCall` and `Backfill` on a connection of their own, so a log's many
//! records never wait on the host's lock nor hold up the terminals behind it.
use super::*;

/// One log read back at a time, whoever asks: a second phone waits, it does not double the
/// disk and the writes.
static READING_BACK: Mutex<()> = Mutex::new(());

/// The calling thread gives way to everything else. On a thread of its own, so nothing else
/// that runs on a client's thread is slowed by it afterwards.
fn give_way() {
    fibo_platform::process::give_way();
}

/// A conversation's raw log, and a connection to read its rows on once the lock is let go.
fn tool_log(
    shared: &Shared,
    session: &str,
) -> io::Result<(ToolLog, String, crate::log_index::Index)> {
    let host = lock(shared)?;
    let s = (host.workspace.agent_sessions.iter())
        .find(|s| s.id == session)
        .ok_or_else(|| io::Error::other(t!("会话不存在")))?;
    let log = ToolLog {
        path: s.transcript.clone(),
        runtime: s.runtime,
    };
    let id = s.cli_session_id.clone();
    let index = host.index.forker()?;
    drop(host);
    Ok((log, id, index.open()?))
}

pub(super) fn dispatch(shared: &Shared, request: Request) -> io::Result<Reply> {
    match request {
        Request::Chat {
            session,
            after,
            before,
            limit,
            tools,
        } => {
            let host = lock(shared)?;
            let s = host
                .workspace
                .agent_sessions
                .iter()
                .find(|s| s.id == session)
                .ok_or_else(|| io::Error::other(t!("会话不存在")))?;
            let page =
                host.index
                    .read_for(s.runtime, &s.cli_session_id, &s.transcript, |index| {
                        let card = host.workspace.content.get(&session);
                        let floor = match card {
                            Some(c) if c.partial_history => {
                                index.read_from(&s.transcript, c.from_offset)?
                            }
                            _ => 0,
                        };
                        let (messages, more) =
                            index.chat(&s.transcript, floor, after, before, limit, tools)?;
                        let partial = floor > 0
                            || card.is_some_and(|c| c.partial_history && c.from_offset == 0);
                        Ok(Reply::Chat {
                            messages,
                            generation: index.chat_generation(&s.transcript)?,
                            more,
                            partial,
                        })
                    })?;
            Ok(page.unwrap_or_else(|| Reply::Chat {
                messages: Vec::new(),
                generation: String::new(),
                more: false,
                partial: false,
            }))
        }
        Request::ToolCalls {
            session,
            seq,
            after,
            limit,
            last,
            stopped,
        } => {
            let (log, id, index) = tool_log(shared, &session)?;
            let (tools, rows, next) = index
                .read_for(log.runtime, &id, &log.path, |index| {
                    let seq = match seq {
                        None if stopped => index.stopped_at(&log.path)?,
                        seq => seq,
                    };
                    match last {
                        Some(n) => index.last_tool_calls(&log.path, seq, n),
                        None => index.tool_calls(&log.path, seq, after, limit),
                    }
                })?
                .unwrap_or_default();
            Ok(Reply::ToolCalls {
                log,
                tools,
                rows,
                next,
            })
        }
        Request::ToolCall { session, call } => {
            let (log, id, index) = tool_log(shared, &session)?;
            let row = index
                .read_for(log.runtime, &id, &log.path, |index| {
                    index.tool_call(&log.path, &call)
                })?
                .flatten()
                .ok_or_else(|| io::Error::other(t!("这条工具调用不在记录里")))?;
            Ok(Reply::ToolCall { log, row })
        }
        Request::Backfill { session } => {
            let (log, id, index) = tool_log(shared, &session)?;
            let _one = READING_BACK.lock().unwrap_or_else(|e| e.into_inner());
            let read = std::thread::spawn(move || {
                give_way();
                index.backfill(log.runtime, &id, &log.path)
            })
            .join()
            .map_err(|_| io::Error::other(t!("读更早的消息时出错")))??;
            Ok(Reply::Backfilled {
                added: read.added,
                done: read.done,
            })
        }
        other => Err(io::Error::other(format!("not a tool request: {other:?}"))),
    }
}
