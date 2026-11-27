//! The limits another machine's host reads there (its plugins' files and commands), for the
//! window attached to it: asked over the forward at most every `EVERY`, in the background,
//! and the last answer shown meanwhile. A host from before `Request::Quotas` answers with
//! an error, and the window shows its sessions' own readings as before.
use super::Host;
use fibo_core::agent::Agent;
use fibo_core::protocol::{Reply, Request, rpc};
use fibo_core::quota::Quota;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const EVERY: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Asked {
    at: Option<Instant>,
    readings: Vec<(Agent, Quota)>,
}
static ASKED: Mutex<Option<HashMap<PathBuf, Asked>>> = Mutex::new(None);

impl Host {
    /// That machine's plugin readings, as its host last gave them; none for this machine,
    /// whose window reads its own.
    pub fn quotas(&self) -> Vec<(Agent, Quota)> {
        let Some(f) = &self.0 else { return vec![] };
        let mut asked = ASKED.lock().unwrap_or_else(|e| e.into_inner());
        let seen = asked.get_or_insert_with(HashMap::new);
        let seen = seen.entry(f.local.clone()).or_default();
        let due = seen.at.is_none_or(|at| at.elapsed() >= EVERY);
        if due {
            seen.at = Some(Instant::now());
            let socket = f.local.clone();
            std::thread::spawn(move || {
                let Ok(Reply::Quotas(readings)) = rpc(&socket, &Request::Quotas) else {
                    return;
                };
                let mut asked = ASKED.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(seen) = asked.as_mut().and_then(|a| a.get_mut(&socket)) {
                    seen.readings = readings;
                }
            });
        }
        seen.readings.clone()
    }
}
