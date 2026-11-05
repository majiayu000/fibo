use crate::remote::{Activity, Observer};
use mooring_core::protocol::{rpc, RemoteAct, RemoteLive, RemotePhone, RemoteState, Request};
use mooring_host::devices;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

/// The connection's state, why it is not connected, and the phones on by session.
type Now = (RemoteState, String, BTreeMap<String, RemotePhone>);

/// Who is on now. With `--report` every change goes to the host, a burst at a time,
/// and the host shows it in the app.
#[derive(Clone)]
pub(super) struct Live {
    inner: Arc<Mutex<Now>>,
    report: Option<(PathBuf, std_mpsc::Sender<()>)>,
}

impl Live {
    pub(super) fn new(socket: Option<PathBuf>) -> Self {
        let inner = Arc::new(Mutex::new((
            RemoteState::Connecting,
            String::new(),
            BTreeMap::new(),
        )));
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
        Self { inner, report }
    }
    #[cfg(test)]
    pub(super) fn connected(&self) -> bool {
        self.inner.lock().unwrap().0 == RemoteState::Connected
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
        self.change(|i| {
            i.0 = state;
            i.1 = detail;
            if state != RemoteState::Connected {
                i.2.clear();
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
        self.change(|i| {
            i.2.insert(session.to_owned(), phone);
        });
    }
    pub(super) fn left(&self, session: &str) {
        self.change(|i| {
            i.2.remove(session);
        });
    }
    pub(super) fn observer(&self, session: &str) -> Observer {
        let (live, session) = (self.clone(), session.to_owned());
        Arc::new(move |activity| {
            live.change(|i| {
                let Some(p) = i.2.get_mut(&session) else {
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

fn snapshot(inner: &Mutex<Now>) -> RemoteLive {
    let i = inner.lock().unwrap_or_else(|e| e.into_inner());
    let mut online: Vec<_> = i.2.values().cloned().collect();
    online.sort_by_key(|p| p.since);
    RemoteLive {
        state: i.0,
        detail: i.1.clone(),
        online,
    }
}
