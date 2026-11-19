//! One host's background refreshes in this process. The window and the menu bar each follow
//! the host's status, and each asked it for the workspace on every change: with thousands
//! of sessions every ask cost the host a snapshot and a diff, most of its CPU (11-19). Now
//! whichever of the two asks first fetches, and the other takes the same workspace: from
//! the fetch in flight, or from what this process already holds at that revision.
use super::brief::{Held, Understood};
use fibo_core::protocol::{Reply, Request};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};

/// The refreshes of the host at one socket, shared by everyone in this process that
/// follows it; edits and launches go through the same `Held`.
#[derive(Clone)]
pub struct Feed(Arc<Inner>);
struct Inner {
    socket: PathBuf,
    held: Held,
    flight: Mutex<Flight>,
    landed: Condvar,
}
#[derive(Default)]
struct Flight {
    /// Fetches on their way to the host.
    fetching: usize,
    /// Counts the fetches that came back.
    landed: u64,
    /// How the last one ended: a failure is the answer for those who waited on it too.
    failed: Option<(io::ErrorKind, String)>,
}
/// Feeds alive in this process, by socket. Weak: the last window or panel to let go of a
/// host ends its feed.
static FEEDS: Mutex<Vec<(PathBuf, Weak<Inner>)>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Feed {
    /// The feed of the host at `socket`, the one already alive in this process if any.
    pub fn of(socket: &Path) -> Self {
        let mut feeds = lock(&FEEDS);
        feeds.retain(|(_, feed)| feed.strong_count() > 0);
        if let Some(inner) = feeds
            .iter()
            .find(|(path, _)| path == socket)
            .and_then(|(_, feed)| feed.upgrade())
        {
            return Self(inner);
        }
        let inner = Arc::new(Inner {
            socket: socket.to_owned(),
            held: Held::default(),
            flight: Mutex::default(),
            landed: Condvar::new(),
        });
        feeds.push((socket.to_owned(), Arc::downgrade(&inner)));
        Self(inner)
    }
    pub fn held(&self) -> &Held {
        &self.0.held
    }
    /// The host's workspace at revision `at_least` or newer: one this process already got,
    /// the one a fetch in flight brings, or a fetch of its own. `None` always asks the host:
    /// after a dropped link the host may be a new one, and what is held says nothing.
    pub fn workspace(&self, host: Understood, at_least: Option<u64>) -> io::Result<Reply> {
        let inner = &*self.0;
        let mut flight = lock(&inner.flight);
        if let Some(revision) = at_least {
            loop {
                if let Some(w) = inner.held.newest(revision) {
                    return Ok(Reply::Workspace(w));
                }
                if flight.fetching == 0 {
                    break;
                }
                let seen = flight.landed;
                flight = inner
                    .landed
                    .wait_while(flight, |f| f.landed == seen)
                    .unwrap_or_else(PoisonError::into_inner);
                // The one waited on failed: so would this, a moment later.
                if let Some((kind, words)) = &flight.failed {
                    return Err(io::Error::new(*kind, words.clone()));
                }
                // Older than asked for, or from a host without `Brief` (nothing is held
                // then): look again, and fetch once nothing else is on its way.
            }
        }
        flight.fetching += 1;
        drop(flight);
        let result = inner
            .held
            .deliver(&inner.socket, Request::Workspace, host)
            .into_result();
        let mut flight = lock(&inner.flight);
        flight.fetching -= 1;
        flight.landed += 1;
        flight.failed = result.as_ref().err().map(|e| (e.kind(), e.to_string()));
        drop(flight);
        inner.landed.notify_all();
        result
    }
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;
