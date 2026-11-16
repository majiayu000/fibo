//! [`Request::Brief`] and [`Request::BriefAll`]: a request whose workspace reply goes as
//! what changed since the one the client holds. The host remembers what it sent last, a
//! few per client.
use super::Shared;
use fibo_core::protocol::{Reply, Request};
use fibo_core::workspace_delta::{diff, Index};
use std::collections::VecDeque;
use std::io;
use std::sync::Mutex;

/// Workspaces sent lately: enough for this machine's window and another's, each with a
/// refresh and an action in flight. Named by what they hold, so one kept across hosts in
/// the same process (tests) still describes the same workspace.
const KEPT: usize = 16;
static SENT: Mutex<VecDeque<Index>> = Mutex::new(VecDeque::new());

pub(super) fn dispatch(shared: &Shared, request: Request) -> io::Result<Reply> {
    let (since, request, fields) = match request {
        Request::Brief(since, request) => (since, request, false),
        Request::BriefAll(since, request) => (since, request, true),
        request => return super::client::dispatch(shared, request),
    };
    Ok(match super::client::dispatch(shared, *request)? {
        Reply::Workspace(w) => {
            let mut sent = SENT
                .lock()
                .map_err(|_| io::Error::other("brief lock poisoned"))?;
            let base = since.and_then(|s| sent.iter().find(|i| i.stamp == s));
            let (delta, index) = diff(w, base, fields).map_err(io::Error::other)?;
            if !sent.iter().any(|i| i.stamp == index.stamp) {
                if sent.len() == KEPT {
                    sent.pop_front();
                }
                sent.push_back(index);
            }
            Reply::WorkspaceDelta(Box::new(delta))
        }
        other => other,
    })
}
