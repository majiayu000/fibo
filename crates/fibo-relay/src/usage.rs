//! Bytes each machine moves through the relay, so whoever runs one can see who uses how
//! much without reading anything. Everything a machine's phones send or get passes over
//! that machine's socket, so counting the socket counts the phones too; the relay's own
//! traffic is about twice the sum (in from one side, out to the other).
//!
//! A meter outlives reconnects while the relay runs. Every `usage_every` the relay prints
//! one `usage` line per machine that moved bytes since the last line, then forgets
//! machines that are gone and moved nothing. Summing the lines in the log gives a day or a
//! month; the relay itself keeps no history.
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

#[derive(Debug, Default)]
pub struct Meter {
    /// Bytes the machine sent: replies to its phones, mostly.
    pub from_machine: AtomicU64,
    /// Bytes sent to the machine: its phones' messages and the relay's frames.
    pub to_machine: AtomicU64,
}

impl Meter {
    pub fn from_machine(&self, n: usize) {
        self.from_machine.fetch_add(n as u64, Ordering::Relaxed);
    }
    pub fn to_machine(&self, n: usize) {
        self.to_machine.fetch_add(n as u64, Ordering::Relaxed);
    }
}

#[derive(Debug, Default)]
pub struct Usage {
    meters: Mutex<HashMap<String, Arc<Meter>>>,
}

impl Usage {
    /// The meter of the machine registered under `token_hash`, the same one on every
    /// reconnect while it is still known.
    pub fn meter(&self, token_hash: &str) -> Arc<Meter> {
        let mut meters = self.meters.lock().unwrap_or_else(|e| e.into_inner());
        meters.entry(token_hash.to_owned()).or_default().clone()
    }

    /// One line per machine that moved bytes since the last call, sorted; the counts
    /// start again from zero. Machines with no socket left and nothing new are dropped.
    pub fn report(&self) -> Vec<String> {
        let mut meters = self.meters.lock().unwrap_or_else(|e| e.into_inner());
        let mut lines = Vec::new();
        meters.retain(|hash, m| {
            let from = m.from_machine.swap(0, Ordering::Relaxed);
            let to = m.to_machine.swap(0, Ordering::Relaxed);
            if from + to > 0 {
                // A prefix is enough to tell machines apart in a log and says nothing more.
                let short: String = hash.chars().take(12).collect();
                lines.push(format!(
                    "usage machine={short} from_machine={from} to_machine={to}"
                ));
            }
            from + to > 0 || Arc::strong_count(m) > 1
        });
        lines.sort();
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_what_moved_since_the_last_report_then_forgets_machines_that_left() {
        let usage = Usage::default();
        let a = usage.meter("aaaaaaaaaaaaaaaaaaaa");
        a.from_machine(100);
        a.to_machine(20);
        // A reconnect gets the same meter.
        usage.meter("aaaaaaaaaaaaaaaaaaaa").to_machine(5);
        let b = usage.meter("bbbbbbbbbbbbbbbbbbbb");
        assert_eq!(
            usage.report(),
            ["usage machine=aaaaaaaaaaaa from_machine=100 to_machine=25"]
        );
        assert!(usage.report().is_empty(), "nothing new, nothing printed");

        drop(a);
        drop(b);
        // Both are gone and moved nothing since: dropped, so the map does not grow forever.
        assert!(usage.report().is_empty());
        assert!(usage.meters.lock().unwrap().is_empty());
    }

    #[test]
    fn a_machine_that_left_after_moving_bytes_is_reported_once_more() {
        let usage = Usage::default();
        usage.meter("cccccccccccccccccccc").from_machine(7);
        assert_eq!(
            usage.report(),
            ["usage machine=cccccccccccc from_machine=7 to_machine=0"]
        );
        // Kept for one more round, in case it comes back; then forgotten.
        assert!(usage.report().is_empty());
        assert!(usage.meters.lock().unwrap().is_empty());
    }
}
