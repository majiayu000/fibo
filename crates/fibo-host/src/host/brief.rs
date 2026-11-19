//! [`Request::Brief`] and [`Request::BriefAll`]: a request whose workspace reply goes as
//! what changed since the one the client holds. The host remembers what it sent last, a
//! few per client.
use super::Shared;
use fibo_core::protocol::{Reply, Request};
use fibo_core::workspace_delta::{diff_with, Hashes, Index};
use std::collections::VecDeque;
use std::io;
use std::sync::Mutex;

/// Workspaces sent lately: enough for this machine's window and another's, each with a
/// refresh and an action in flight. Named by what they hold, so one kept across hosts in
/// the same process (tests) still describes the same workspace.
const KEPT: usize = 16;
static SENT: Mutex<VecDeque<Index>> = Mutex::new(VecDeque::new());
/// Content hashes from the last reply, for every client: each refreshes on the same change.
static HASHES: std::sync::LazyLock<Mutex<Hashes>> = std::sync::LazyLock::new(Mutex::default);

pub(super) fn dispatch(shared: &Shared, request: Request) -> io::Result<Reply> {
    let (since, request, fields) = match request {
        Request::Brief(since, request) => (since, request, false),
        Request::BriefAll(since, request) => (since, request, true),
        request => return super::client::dispatch(shared, request),
    };
    // A refresh, by far the most frequent, is worked out against the host's own content
    // under its lock; an edit or a launch replies with the workspace it left. The host's
    // lock is taken before the two here, as an edit takes it before them too.
    let from = match *request {
        Request::Workspace => Source::Host(super::lock(shared)?),
        request => match super::client::dispatch(shared, request)? {
            Reply::Workspace(w) => Source::Reply(Box::new(w)),
            other => return Ok(other),
        },
    };
    let mut sent = SENT
        .lock()
        .map_err(|_| io::Error::other("brief lock poisoned"))?;
    let base = since.and_then(|s| sent.iter().find(|i| i.stamp == s));
    let mut hashes = HASHES
        .lock()
        .map_err(|_| io::Error::other("brief lock poisoned"))?;
    let (delta, index) = match from {
        Source::Host(host) => host.brief(base, fields, &mut hashes),
        Source::Reply(w) => diff_with(*w, base, fields, &mut hashes),
    }
    .map_err(io::Error::other)?;
    drop(hashes);
    if !sent.iter().any(|i| i.stamp == index.stamp) {
        if sent.len() == KEPT {
            sent.pop_front();
        }
        sent.push_back(index);
    }
    Ok(Reply::WorkspaceDelta(Box::new(delta)))
}

enum Source<'a> {
    Host(std::sync::MutexGuard<'a, super::Host>),
    Reply(Box<fibo_core::model::Workspace>),
}
