//! Both ways to one machine (docs/multi-machine/routes.md): `ssh -L` and `fibo remote dial`
//! each listen beside its `workbench.sock`, and this window listens on that socket itself,
//! handing each connection to the way chosen now. Each way's round trip is timed every
//! [`EVERY`] on a link kept open; the faster one is taken, with hysteresis, and connections
//! on a way left behind are closed here, so the window's own reconnecting moves them.
use crate::machines::Via;
use fibo_platform::local;
use std::collections::VecDeque;
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How often each way is timed.
pub(super) const EVERY: Duration = Duration::from_secs(5);
/// A probe slower than this counts as the way being down.
const PROBE_WAIT: Duration = Duration::from_secs(3);
/// How many probes in a row the other way must win by to be taken.
const RUNS: usize = 3;

/// What the window can show of a machine's ways.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// The way its connections take now.
    pub via: Via,
    /// The other way, when there is one: its last round trip, or why it is not used.
    pub other: Option<(Via, Result<Duration, String>)>,
}

#[derive(Default)]
struct State {
    /// None until a way answered.
    active: Option<Via>,
    /// The last [`RUNS`] probes of each way, by [`slot`]; None is a failed one.
    samples: [VecDeque<Option<Duration>>; 2],
    /// Why each way last failed.
    why: [Option<String>; 2],
    /// Connections handed on through the splitter, to close when their way is left.
    links: Vec<(u64, Via, local::Stream)>,
}

/// The two ways' state, kept by a forward that has both.
#[derive(Default)]
pub(super) struct Ways {
    state: Mutex<State>,
    next_id: AtomicU64,
}

pub(super) fn slot(via: Via) -> usize {
    match via {
        Via::Ssh => 0,
        Via::Relay => 1,
    }
}

/// Where a way listens beside `local` (`<root>/workbench.sock`).
pub(super) fn beside(local: &Path, via: Via) -> PathBuf {
    local.with_file_name(match via {
        Via::Ssh => "ssh.sock",
        Via::Relay => "relay.sock",
    })
}

impl Ways {
    /// The first way that answered starts as the one taken.
    pub(super) fn begin(&self, via: Via) {
        let mut s = self.state.lock().unwrap();
        s.active.get_or_insert(via);
    }
    /// Why `via` is not reached now, for the window; cleared by its next good probe.
    pub(super) fn failed(&self, via: Via, why: String) {
        self.state.lock().unwrap().why[slot(via)] = Some(why);
    }
    pub(super) fn route(&self, relay_paired: bool) -> Option<Route> {
        let s = self.state.lock().unwrap();
        let via = s.active?;
        let other = via.other();
        let state = match (other, relay_paired) {
            (Via::Relay, false) => Err(t!("没配对").to_owned()),
            _ => match s.samples[slot(other)].back() {
                Some(Some(rtt)) => Ok(*rtt),
                _ => Err(s.why[slot(other)]
                    .clone()
                    .unwrap_or_else(|| t!("连接中…").to_owned())),
            },
        };
        Some(Route {
            via,
            other: Some((other, state)),
        })
    }
    /// Records one probe of `via`; the way to take after it, when that changed, with the
    /// connections on the way left closed.
    fn probed(&self, via: Via, rtt: Option<Duration>) -> Option<Via> {
        let mut s = self.state.lock().unwrap();
        let samples = &mut s.samples[slot(via)];
        samples.push_back(rtt);
        while samples.len() > RUNS {
            samples.pop_front();
        }
        if rtt.is_some() {
            s.why[slot(via)] = None;
        }
        let Some(active) = s.active else {
            s.active = rtt.map(|_| via);
            return s.active;
        };
        let (ssh, relay) = s.samples.split_at_mut(1);
        let next = choose(active, ssh[0].make_contiguous(), relay[0].make_contiguous());
        if next == active {
            return None;
        }
        s.active = Some(next);
        for (_, _, stream) in s.links.iter().filter(|(_, v, _)| *v == active) {
            let _ = stream.shutdown(Shutdown::Both);
        }
        Some(next)
    }
}

/// The way to take after the last probes of each (oldest first): leave the one taken at
/// once when its last probe failed and the other's did not; take the other only when it won
/// the last [`RUNS`] probes each by a fifth and 5 ms; otherwise stay.
pub(crate) fn choose(active: Via, ssh: &[Option<Duration>], relay: &[Option<Duration>]) -> Via {
    let of = |via| match via {
        Via::Ssh => ssh,
        Via::Relay => relay,
    };
    let (now, other) = (of(active), of(active.other()));
    let up = |s: &[Option<Duration>]| matches!(s.last(), Some(Some(_)));
    if !up(now) {
        return if up(other) { active.other() } else { active };
    }
    let won = |(a, b): (&Option<Duration>, &Option<Duration>)| match (a, b) {
        (Some(a), Some(b)) => *b * 5 <= *a * 4 && *a >= *b + Duration::from_millis(5),
        _ => false,
    };
    let runs = now.len() >= RUNS
        && other.len() >= RUNS
        && (now.iter().rev().zip(other.iter().rev()))
            .take(RUNS)
            .all(won);
    if runs { active.other() } else { active }
}

/// One request's round trip on a link already open, opened (and greeted) when there is
/// none; the link is handed back to be kept for the next. Through the relay each new link
/// is a new session there, which it counts.
pub fn round_trip(
    socket: &Path,
    kept: Option<local::Stream>,
    wait: Duration,
) -> (Option<Duration>, Option<local::Stream>) {
    use fibo_core::protocol::{CONTROL, MAJOR, Reply, Request, receive, send};
    let open = || -> std::io::Result<local::Stream> {
        let mut stream = local::Stream::connect(socket)?;
        stream.set_read_timeout(Some(wait))?;
        send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
        receive::<Reply>(&mut stream)?;
        Ok(stream)
    };
    let was_kept = kept.is_some();
    let Some(mut stream) = kept.map_or_else(|| open().ok(), Some) else {
        return (None, None);
    };
    let at = std::time::Instant::now();
    // A terminal no one has: answered at once, whatever the host's version.
    let probe = Request::TerminalCwd { id: String::new() };
    let answered = send(&mut stream, CONTROL, &probe).and_then(|_| receive::<Reply>(&mut stream));
    match answered {
        Ok(_) => (Some(at.elapsed()), Some(stream)),
        // The host there restarted or the forward did: once more on a new link. Not after
        // a timeout: the way itself is stuck, and a new link would wait as long again.
        Err(e) if was_kept && !timed_out(&e) => round_trip(socket, None, wait),
        Err(_) => (None, None),
    }
}

fn timed_out(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// Times both ways of the forward behind `local` until `stopped`, switching as [`choose`]
/// says; `relay` is false when only ssh is there to time.
pub(super) fn time_ways(
    ways: &Ways,
    local: &Path,
    relay: bool,
    host: &str,
    stopped: &dyn Fn() -> bool,
) {
    let mut kept: [Option<local::Stream>; 2] = [None, None];
    while !stopped() {
        for via in [Via::Ssh, Via::Relay] {
            if via == Via::Relay && !relay {
                continue;
            }
            let (rtt, link) = round_trip(&beside(local, via), kept[slot(via)].take(), PROBE_WAIT);
            kept[slot(via)] = link;
            if let Some(next) = ways.probed(via, rtt) {
                eprintln!("forward {host}: now {}", next.word());
            }
        }
        std::thread::sleep(EVERY);
    }
}

/// Listens on `local` for this window's connections to that machine and hands each to the
/// way taken now, or to the other when that one does not answer, until `stopped`.
pub(super) fn split(
    ways: Arc<Ways>,
    local: PathBuf,
    relay: bool,
    stopped: Arc<dyn Fn() -> bool + Send + Sync>,
) -> std::io::Result<()> {
    if matches!(local::at_path(&local)?, local::AtPath::MySocket) {
        std::fs::remove_file(&local)?;
    }
    let listener = local::Listener::bind(&local)?;
    local::restrict(&local)?;
    std::thread::spawn(move || {
        while let Ok(inbound) = listener.accept() {
            if stopped() {
                return;
            }
            let ways = ways.clone();
            let local = local.clone();
            std::thread::spawn(move || hand_on(&ways, &local, relay, inbound));
        }
    });
    Ok(())
}

/// Wakes the splitter's accept so it sees it was stopped.
pub(super) fn wake(local: &Path) {
    let _ = local::Stream::connect(local);
}

fn hand_on(ways: &Ways, local: &Path, relay: bool, inbound: local::Stream) {
    let first = ways.state.lock().unwrap().active.unwrap_or(Via::Ssh);
    let tries = [first, first.other()];
    let Some((via, outbound)) = tries
        .into_iter()
        .filter(|via| relay || *via == Via::Ssh)
        .find_map(|via| Some((via, local::Stream::connect(beside(local, via)).ok()?)))
    else {
        return;
    };
    let id = ways.next_id.fetch_add(1, Ordering::Relaxed);
    if let Ok(kept) = inbound.try_clone() {
        ways.state.lock().unwrap().links.push((id, via, kept));
    }
    pipe(inbound, outbound);
    ways.state
        .lock()
        .unwrap()
        .links
        .retain(|(i, _, _)| *i != id);
}

/// Copies both ways until either end closes, then closes both.
fn pipe(a: local::Stream, b: local::Stream) {
    let (Ok(a2), Ok(b2)) = (a.try_clone(), b.try_clone()) else {
        return;
    };
    let back = std::thread::spawn(move || copy(&b2, &a2));
    copy(&a, &b);
    let _ = back.join();
}

fn copy(mut from: &local::Stream, mut to: &local::Stream) {
    let _ = std::io::copy(&mut from, &mut to);
    let _ = from.shutdown(Shutdown::Both);
    let _ = to.shutdown(Shutdown::Both);
}

#[cfg(test)]
mod tests;
