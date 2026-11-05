//! `Request::ToolCalls` and `ToolCall`: rows on a connection of their own, so a log's many
//! records never wait on the host's lock nor hold up the terminals behind it.
use super::*;

/// A conversation's raw log, and a connection to read its rows on once the lock is let go.
fn tool_log(shared: &Shared, session: &str) -> io::Result<(ToolLog, crate::log_index::Index)> {
    let host = lock(shared)?;
    let s = (host.workspace.agent_sessions.iter())
        .find(|s| s.id == session)
        .ok_or_else(|| io::Error::other("会话不存在"))?;
    let log = ToolLog {
        path: s.transcript.clone(),
        runtime: s.runtime,
    };
    Ok((log, host.index.fork_current()?))
}

pub(super) fn dispatch(shared: &Shared, request: Request) -> io::Result<Reply> {
    match request {
        Request::ToolCalls {
            session,
            seq,
            after,
            limit,
        } => {
            let (log, index) = tool_log(shared, &session)?;
            let (tools, rows, next) = index.tool_calls(&log.path, seq, after, limit)?;
            Ok(Reply::ToolCalls {
                log,
                tools,
                rows,
                next,
            })
        }
        Request::ToolCall { session, call } => {
            let (log, index) = tool_log(shared, &session)?;
            let row = index
                .tool_call(&log.path, &call)?
                .ok_or_else(|| io::Error::other("这条工具调用不在记录里"))?;
            Ok(Reply::ToolCall { log, row })
        }
        other => Err(io::Error::other(format!("not a tool request: {other:?}"))),
    }
}
