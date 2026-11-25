//! The phone's side of [`Request::BriefAll`]: every read of the workspace here (a card
//! refresh every 15 seconds and on each burst of events, and one-off lookups such as which
//! conversation a terminal runs) asked the host for the whole workspace, about 3.7 MB on
//! a busy machine (#527). Asked as `BriefAll`, the host sends what changed since the one
//! this process last got, and it is put back together here, so callers still get the
//! whole workspace as the host has it now.
//!
//! A host before `BriefAll` drops a connection that sends one: whether it understands is
//! read from its status, and a failed brief falls back to the whole workspace and reads
//! the status again next time (an update may have swapped the host).
use fibo_core::model::Workspace;
use fibo_core::protocol::{rpc, RecognitionStatus, Reply, Request};
use fibo_core::workspace_delta::{apply, Delta, Kept};
use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

/// A refresh and a lookup on another thread can each be taken against the same one.
const KEPT: usize = 3;
static HELD: Mutex<VecDeque<(u64, Arc<Kept>)>> = Mutex::new(VecDeque::new());

const UNKNOWN: u8 = 0;
const YES: u8 = 1;
const NO: u8 = 2;
static UNDERSTOOD: AtomicU8 = AtomicU8::new(UNKNOWN);

/// What a status read for other reasons says about the host, so a refresh needs no
/// extra request to know.
pub(super) fn note(status: &RecognitionStatus) {
    let understood = if status.brief_all { YES } else { NO };
    UNDERSTOOD.store(understood, Ordering::Relaxed);
}

/// The whole workspace as the host has it now.
pub(super) fn workspace(socket: &Path) -> io::Result<Workspace> {
    if UNDERSTOOD.load(Ordering::Relaxed) == UNKNOWN {
        if let Ok(Reply::Recognition(status)) = rpc(socket, &Request::RecognitionStatus) {
            note(&status);
        }
    }
    if UNDERSTOOD.load(Ordering::Relaxed) == YES {
        match briefed(socket) {
            Ok(w) => return Ok(w),
            Err(_) => UNDERSTOOD.store(UNKNOWN, Ordering::Relaxed),
        }
    }
    match rpc(socket, &Request::Workspace)? {
        Reply::Workspace(w) => Ok(w),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

fn briefed(socket: &Path) -> io::Result<Workspace> {
    let latest = HELD.lock().ok().and_then(|h| h.back().map(|(s, _)| *s));
    match take(ask(socket, latest)?) {
        Ok(w) => Ok(w),
        // The one it was taken against is gone here: asked again, it comes back whole.
        Err(_) => take(ask(socket, None)?).map_err(io::Error::other),
    }
}

fn ask(socket: &Path, since: Option<u64>) -> io::Result<Delta> {
    match rpc(
        socket,
        &Request::BriefAll(since, Box::new(Request::Workspace)),
    )? {
        Reply::WorkspaceDelta(delta) => Ok(*delta),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

fn take(delta: Delta) -> Result<Workspace, String> {
    let base = delta.base.and_then(|stamp| {
        let held = HELD.lock().ok()?;
        (held.iter().find(|(s, _)| *s == stamp)).map(|(_, kept)| Arc::clone(kept))
    });
    let stamp = delta.stamp;
    let kept = apply(delta, base.as_deref())?;
    let w = kept.workspace.clone();
    if let Ok(mut held) = HELD.lock() {
        held.retain(|(s, _)| *s != stamp);
        if held.len() == KEPT {
            held.pop_front();
        }
        held.push_back((stamp, Arc::new(kept)));
    }
    Ok(w)
}

#[cfg(test)]
#[path = "held_tests.rs"]
mod tests;
