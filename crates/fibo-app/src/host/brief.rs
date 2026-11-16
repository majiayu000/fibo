//! The window's side of [`Request::Brief`]: a refresh, an edit or a launch asks the host
//! for only what changed since the workspace this window last got, and gets the whole
//! workspace back here, so nothing past the store sees the difference.
use fibo_core::delivery::{PATIENCE, Settled, deliver};
use fibo_core::model::Workspace;
use fibo_core::protocol::{Reply, Request};
use fibo_core::workspace_delta::{Delta, apply};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A refresh and an action in flight at once are each taken against the same workspace;
/// whichever comes back second still needs it.
const KEPT: usize = 3;

/// The workspaces this window got lately, by stamp, as the host sent them (the window
/// edits its own copy).
#[derive(Clone, Default)]
pub struct Held(Arc<Mutex<VecDeque<Stamped>>>);
type Stamped = (u64, Arc<Workspace>);

impl Held {
    /// Delivers `request`, asking for a delta when `brief` (the host understands it).
    pub fn deliver(&self, socket: &Path, request: Request, brief: bool) -> Settled {
        let briefed = matches!(
            request,
            Request::Workspace | Request::Edit { .. } | Request::Launch { .. }
        );
        if !(brief && briefed) {
            return deliver(socket, &request, PATIENCE);
        }
        let since = self.latest();
        let settled = deliver(
            socket,
            &Request::Brief(since, Box::new(request.clone())),
            PATIENCE,
        );
        let Settled::Confirmed(Reply::WorkspaceDelta(delta)) = settled else {
            return settled;
        };
        match self.take(*delta) {
            Ok(w) => Settled::Confirmed(Reply::Workspace(w)),
            // The one it was taken against is gone here: the resend is once as the first
            // was, and comes back whole.
            Err(_) => match deliver(socket, &Request::Brief(None, Box::new(request)), PATIENCE) {
                Settled::Confirmed(Reply::WorkspaceDelta(delta)) => match self.take(*delta) {
                    Ok(w) => Settled::Confirmed(Reply::Workspace(w)),
                    Err(e) => Settled::Refused(e),
                },
                other => other,
            },
        }
    }
    fn latest(&self) -> Option<u64> {
        self.0.lock().ok()?.back().map(|(stamp, _)| *stamp)
    }
    fn take(&self, delta: Delta) -> Result<Workspace, String> {
        let base = delta.base.and_then(|stamp| {
            let held = self.0.lock().ok()?;
            held.iter()
                .find(|(s, _)| *s == stamp)
                .map(|(_, w)| Arc::clone(w))
        });
        let stamp = delta.stamp;
        let w = apply(delta, base.as_deref())?;
        if let Ok(mut held) = self.0.lock() {
            held.retain(|(s, _)| *s != stamp);
            if held.len() == KEPT {
                held.pop_front();
            }
            held.push_back((stamp, Arc::new(w.clone())));
        }
        Ok(w)
    }
}

#[cfg(test)]
#[path = "brief_tests.rs"]
mod tests;
