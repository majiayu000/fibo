use crate::remote::{Activity, Observer};
use fibo_core::protocol::{
    rpc, RelayLive, RemoteAct, RemoteLive, RemotePhone, RemoteState, Request,
};
use fibo_host::devices;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Each relay's state and why it is not connected, and the phones on by relay and session.
#[derive(Default)]
struct Now {
    /// In the order [`Live::scoped`] first named them: the setting's order.
    relays: Vec<(String, RemoteState, String)>,
    phones: BTreeMap<(String, String), RemotePhone>,
}

/// Who is on now. With `--report` every change goes to the host, a burst at a time,
/// and the host shows it in the app. Each relay's connection writes through its own
/// [`Live::scoped`] copy: session ids are the relay's, so two relays may hand out the same.
#[derive(Clone)]
pub(super) struct Live {
    inner: Arc<Mutex<Now>>,
    report: Option<(PathBuf, std_mpsc::Sender<()>)>,
    scope: String,
}

impl Live {
    pub(super) fn new(socket: Option<PathBuf>) -> Self {
        let inner = Arc::new(Mutex::new(Now::default()));
        let report = socket.map(|socket| {
            let (wake, woken) = std_mpsc::channel::<()>();
            let (inner, at) = (inner.clone(), socket.clone());
            thread::spawn(move || {
                while woken.recv().is_ok() {
                    // Changes in the next moment go out together.
                    thread::sleep(Duration::from_millis(100));
                    while woken.try_recv().is_ok() {}
                    let _ = rpc(&at, &Request::RemoteReport(snapshot(&inner)));
                }
            });
            (socket, wake)
        });
        Self {
            inner,
            report,
            scope: String::new(),
        }
    }
    /// The same, for the connection to `relay`; it shows as connecting until it says.
    pub(super) fn scoped(&self, relay: &str) -> Self {
        if let Ok(mut i) = self.inner.lock() {
            if i.relays.iter().all(|(r, ..)| r != relay) {
                i.relays
                    .push((relay.to_owned(), RemoteState::Connecting, String::new()));
            }
        }
        Self {
            scope: relay.to_owned(),
            ..self.clone()
        }
    }
    #[cfg(test)]
    pub(super) fn connected(&self) -> bool {
        snapshot(&self.inner).state == RemoteState::Connected
    }
    fn change(&self, f: impl FnOnce(&mut Now)) {
        if let Ok(mut i) = self.inner.lock() {
            f(&mut i);
        }
        if let Some((_, wake)) = &self.report {
            let _ = wake.send(());
        }
    }
    pub(super) fn state(&self, state: RemoteState, detail: String) {
        let scope = self.scope.clone();
        self.change(|i| {
            if state != RemoteState::Connected {
                i.phones.retain(|(relay, _), _| *relay != scope);
            }
            match i.relays.iter_mut().find(|(r, ..)| *r == scope) {
                Some(r) => (r.1, r.2) = (state, detail),
                None => i.relays.push((scope, state, detail)),
            }
        });
    }
    /// Sends the last state now, before the process ends.
    pub(super) fn flush(&self) {
        if let Some((socket, _)) = &self.report {
            let _ = rpc(socket, &Request::RemoteReport(snapshot(&self.inner)));
        }
    }
    pub(super) fn joined(&self, session: &str, device: &str) {
        let phone = RemotePhone {
            device: device.to_owned(),
            since: devices::now_ms(),
            watching: vec![],
            last: None,
        };
        let key = (self.scope.clone(), session.to_owned());
        self.change(|i| {
            i.phones.insert(key, phone);
        });
    }
    pub(super) fn left(&self, session: &str) {
        let key = (self.scope.clone(), session.to_owned());
        self.change(|i| {
            i.phones.remove(&key);
        });
    }
    pub(super) fn observer(&self, session: &str) -> Observer {
        let (live, key) = (self.clone(), (self.scope.clone(), session.to_owned()));
        Arc::new(move |activity| {
            live.change(|i| {
                let Some(p) = i.phones.get_mut(&key) else {
                    return;
                };
                match activity {
                    Activity::Watching(t) => p.watching.push(t),
                    Activity::Unwatched(t) => {
                        if let Some(at) = p.watching.iter().position(|w| *w == t) {
                            p.watching.remove(at);
                        }
                    }
                    Activity::Acted { op, terminal } => {
                        p.last = Some(RemoteAct {
                            op,
                            terminal,
                            at: devices::now_ms(),
                        })
                    }
                }
            })
        })
    }
}

/// One relay up is enough for phones to reach the machine, so that is the state shown;
/// with more than one relay, the details name the relays that are not up.
fn snapshot(inner: &Mutex<Now>) -> RemoteLive {
    let i = inner.lock().unwrap_or_else(|e| e.into_inner());
    let mut online: Vec<_> = i.phones.values().cloned().collect();
    online.sort_by_key(|p| p.since);
    let states = || i.relays.iter().map(|(_, s, _)| *s);
    let state = [RemoteState::Connected, RemoteState::Connecting]
        .into_iter()
        .find(|want| states().any(|s| s == *want))
        .unwrap_or(if i.relays.is_empty() {
            RemoteState::Connecting
        } else {
            RemoteState::Failed
        });
    let many = i.relays.len() > 1;
    let detail = (i.relays.iter())
        .filter(|(_, s, d)| *s != RemoteState::Connected && !d.is_empty())
        .map(|(relay, _, d)| match many {
            true => format!("{}：{d}", host(relay)),
            false => d.clone(),
        })
        .collect::<Vec<_>>()
        .join("；");
    let relays = (i.relays.iter())
        .map(|(relay, state, detail)| RelayLive {
            relay: relay.clone(),
            state: *state,
            detail: detail.clone(),
        })
        .collect();
    RemoteLive {
        state,
        detail,
        online,
        relays,
    }
}

fn host(relay: &str) -> &str {
    relay.split_once("://").map_or(relay, |(_, rest)| rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_relay_up_is_up_and_sessions_of_two_relays_do_not_collide() {
        let live = Live::new(None);
        let (a, b) = (live.scoped("wss://a"), live.scoped("wss://b"));
        assert_eq!(snapshot(&live.inner).state, RemoteState::Connecting);
        a.state(RemoteState::Connected, String::new());
        b.state(RemoteState::Connected, String::new());
        a.joined("1", "phone-a");
        b.joined("1", "phone-b");
        assert_eq!(snapshot(&live.inner).online.len(), 2);
        b.state(RemoteState::Connecting, "超时，2 秒后重连".into());
        let now = snapshot(&live.inner);
        assert_eq!(now.state, RemoteState::Connected);
        assert_eq!(now.online.len(), 1);
        assert_eq!(now.online[0].device, "phone-a");
        assert_eq!(now.detail, "b：超时，2 秒后重连");
        a.state(RemoteState::Failed, "密钥不对".into());
        let now = snapshot(&live.inner);
        assert_eq!(now.state, RemoteState::Connecting);
        assert_eq!(now.detail, "a：密钥不对；b：超时，2 秒后重连");
        b.state(RemoteState::Failed, "版本太旧".into());
        assert_eq!(snapshot(&live.inner).state, RemoteState::Failed);
    }

    #[test]
    fn a_single_relay_reads_as_before() {
        let live = Live::new(None).scoped("wss://a");
        live.state(RemoteState::Connecting, "超时".into());
        assert_eq!(snapshot(&live.inner).detail, "超时");
    }
}
