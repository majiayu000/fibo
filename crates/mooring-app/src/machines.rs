//! Other machines the window shows beside this one (docs/multi-machine/spec.md, step 1:
//! look only). Each runs `ssh <host> <mooring> remote --stdio` and speaks the remote
//! protocol, the same one the phone does. A supervisor thread per machine keeps its list
//! of terminals fresh from `events` and reconnects after a drop. Nothing runs when no
//! machine is configured.
use mooring_remote::client::Client;
use mooring_remote::protocol::{Card, Hello, List, Machine, Op, Space, StreamBody};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REPLY: Duration = Duration::from_secs(10);
/// Events in a burst are one fresh list.
const SETTLE: Duration = Duration::from_millis(150);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// One machine as the user wrote it down, kept in `<config>/machines.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    /// What `ssh` connects to: a name from `~/.ssh/config`, or `user@host`.
    pub host: String,
    /// `mooring` on that machine; unset tries `mooring` on its PATH, then the app bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Spec {
    pub fn command(&self) -> Vec<String> {
        let run = match &self.path {
            Some(path) => format!("exec {} remote --stdio", quote(path)),
            // On its PATH, else the app bundle in ~/Applications, else /Applications.
            None => [
                "p=$(command -v mooring)",
                "[ -n \"$p\" ] || p=\"$HOME/Applications/Mooring.app/Contents/MacOS/mooring\"",
                "[ -x \"$p\" ] || p=/Applications/Mooring.app/Contents/MacOS/mooring",
                "exec \"$p\" remote --stdio",
            ]
            .join("; "),
        };
        [
            "ssh",
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=2",
            &self.host,
            &run,
        ]
        .map(str::to_owned)
        .to_vec()
    }
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn file() -> Option<PathBuf> {
    mooring_core::agent::config_dir().map(|d| d.join("machines.json"))
}

pub fn load() -> Vec<Spec> {
    file()
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(specs: &[Spec]) -> io::Result<()> {
    let f = file().ok_or_else(|| io::Error::other("找不到配置目录"))?;
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = f.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(specs).map_err(io::Error::other)?,
    )?;
    std::fs::rename(tmp, f)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Connecting,
    Online,
    /// Why it is not connected; it keeps retrying.
    Offline(String),
}

/// What the window draws for one machine.
#[derive(Debug, Clone)]
pub struct View {
    pub spec: Spec,
    pub link: Link,
    /// Its own name and id, once it said hello.
    pub machine: Option<Machine>,
    pub spaces: Vec<Space>,
    pub terminals: Vec<Card>,
}

/// Where a machine's live connection is, if it has one now.
pub type Slot = Arc<Mutex<Option<Arc<Client>>>>;

/// One machine's connection, kept running by its supervisor until dropped.
pub struct Remote {
    pub view: Arc<Mutex<View>>,
    client: Slot,
    stop: Arc<AtomicBool>,
}

impl Remote {
    /// Starts the supervisor. `connect` makes a fresh client each attempt; `changed` is
    /// called after every change to `view`, from the supervisor's thread.
    pub fn start(
        spec: Spec,
        connect: impl Fn(&Spec) -> io::Result<Client> + Send + 'static,
        changed: impl Fn() + Send + 'static,
    ) -> Self {
        let view = Arc::new(Mutex::new(View {
            spec: spec.clone(),
            link: Link::Connecting,
            machine: None,
            spaces: Vec::new(),
            terminals: Vec::new(),
        }));
        let client = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (v, c, s) = (Arc::clone(&view), Arc::clone(&client), Arc::clone(&stop));
        std::thread::spawn(move || supervise(spec, connect, changed, v, c, s));
        Self { view, client, stop }
    }

    /// Where its live connection is, for a terminal's screen.
    pub fn slot(&self) -> Slot {
        Arc::clone(&self.client)
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Dropping the last handle kills the ssh child, which ends the supervisor's wait.
        self.client.lock().unwrap().take();
    }
}

fn supervise(
    spec: Spec,
    connect: impl Fn(&Spec) -> io::Result<Client>,
    changed: impl Fn(),
    view: Arc<Mutex<View>>,
    slot: Arc<Mutex<Option<Arc<Client>>>>,
    stop: Arc<AtomicBool>,
) {
    let mut backoff = Duration::from_secs(1);
    while !stop.load(Ordering::Relaxed) {
        let why = match run_once(&spec, &connect, &changed, &view, &slot, &stop) {
            Ok(()) => return,
            Err(e) => e.to_string(),
        };
        slot.lock().unwrap().take();
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let was_online = matches!(view.lock().unwrap().link, Link::Online);
        if was_online {
            backoff = Duration::from_secs(1);
        }
        view.lock().unwrap().link = Link::Offline(why);
        changed();
        // Sleep in small steps so a removed machine stops promptly.
        let mut slept = Duration::ZERO;
        while slept < backoff && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
            slept += Duration::from_millis(100);
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
        if !stop.load(Ordering::Relaxed) {
            view.lock().unwrap().link = Link::Connecting;
            changed();
        }
    }
}

/// One connection, until it drops (an error) or the machine is removed (`Ok`).
fn run_once(
    spec: &Spec,
    connect: &impl Fn(&Spec) -> io::Result<Client>,
    changed: &impl Fn(),
    view: &Mutex<View>,
    slot: &Mutex<Option<Arc<Client>>>,
    stop: &AtomicBool,
) -> io::Result<()> {
    let client = Arc::new(connect(spec)?);
    let hello: Hello = parse(client.request(Op::Hello { version: Some(1) }, REPLY)?)?;
    let list: List = parse(client.request(Op::List, REPLY).map_err(not_running)?)?;
    let (tx, rx) = channel();
    let ends = tx.clone();
    client.stream(
        Op::Events,
        move |body| {
            let _ = match body {
                StreamBody::End { reason } => ends.send(Some(reason)),
                _ => ends.send(None),
            };
        },
        REPLY,
    )?;
    *slot.lock().unwrap() = Some(Arc::clone(&client));
    apply(view, Some(hello.machine), list);
    changed();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Some(reason)) => return Err(io::Error::other(reason)),
            Ok(None) => {
                // Let the burst settle, then one list for all of it.
                let deadline = std::time::Instant::now() + SETTLE;
                while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
                    match rx.recv_timeout(left) {
                        Ok(Some(reason)) => return Err(io::Error::other(reason)),
                        Ok(None) => continue,
                        Err(_) => break,
                    }
                }
                let list: List = parse(client.request(Op::List, REPLY)?)?;
                apply(view, None, list);
                changed();
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(why) = client.ended() {
                    return Err(io::Error::other(why));
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other("事件流断了"));
            }
        }
    }
}

/// The machine answered but its host is not there to ask: Mooring is not open on it.
fn not_running(e: io::Error) -> io::Error {
    if e.to_string().starts_with("RPC connect") {
        io::Error::other("那台电脑上的 Mooring 没打开")
    } else {
        e
    }
}

fn apply(view: &Mutex<View>, machine: Option<Machine>, list: List) {
    let mut v = view.lock().unwrap();
    v.link = Link::Online;
    v.machine = machine.or(Some(list.machine));
    v.spaces = list.spaces;
    v.terminals = list.terminals;
}

fn parse<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> io::Result<T> {
    serde_json::from_value(v).map_err(|e| io::Error::other(format!("对方的回复读不懂：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc::Receiver;

    struct Fake {
        /// Terminals the next `list` returns.
        terminals: Arc<Mutex<Vec<Value>>>,
        /// Pushes an event line into the open events stream.
        events: Arc<Mutex<Option<(u64, UnixStream)>>>,
    }

    fn card(id: &str, status: &str) -> Value {
        json!({ "id": id, "space": "s", "tab": "t", "cwd": "/w", "phase": "working",
            "agent": "claude", "session": null, "title": id, "approval": null,
            "waiting": null, "status": status, "status_label": status, "needs_you": false })
    }

    fn fake() -> (Fake, impl Fn(&Spec) -> io::Result<Client>) {
        let terminals = Arc::new(Mutex::new(vec![card("a", "running")]));
        let events = Arc::new(Mutex::new(None));
        let fake = Fake {
            terminals: Arc::clone(&terminals),
            events: Arc::clone(&events),
        };
        let connect = move |_: &Spec| {
            let (ours, theirs) = UnixStream::pair()?;
            let (terminals, events) = (Arc::clone(&terminals), Arc::clone(&events));
            let mut out = theirs.try_clone()?;
            std::thread::spawn(move || {
                for line in BufReader::new(theirs).lines().map_while(Result::ok) {
                    let r: Value = serde_json::from_str(&line).unwrap();
                    let id = r["id"].as_u64().unwrap();
                    let ok = match r["op"].as_str().unwrap() {
                        "hello" => json!({ "version": 1, "machine": { "id": "m2", "name": "mac2" },
                            "ops": ["list", "events", "watch"], "role": "full" }),
                        "list" => json!({ "machine": { "id": "m2", "name": "mac2" }, "spaces": [],
                            "terminals": terminals.lock().unwrap().clone() }),
                        "events" => {
                            *events.lock().unwrap() = Some((id, out.try_clone().unwrap()));
                            json!({ "stream": id })
                        }
                        _ => json!({}),
                    };
                    writeln!(out, "{}", json!({ "id": id, "ok": ok })).unwrap();
                }
            });
            Ok(Client::new(ours.try_clone()?, ours))
        };
        (fake, connect)
    }

    fn wait(rx: &Receiver<()>, view: &Mutex<View>, until: impl Fn(&View) -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !until(&view.lock().unwrap()) {
            assert!(
                std::time::Instant::now() < deadline,
                "{:?}",
                view.lock().unwrap()
            );
            let _ = rx.recv_timeout(Duration::from_millis(100));
        }
    }

    #[test]
    fn lists_the_machine_and_follows_its_events() {
        let (fake, connect) = fake();
        let (tx, rx) = channel();
        let spec = Spec {
            host: "mac2".into(),
            path: None,
        };
        let r = Remote::start(spec, connect, move || {
            let _ = tx.send(());
        });
        wait(&rx, &r.view, |v| v.link == Link::Online);
        assert_eq!(
            r.view.lock().unwrap().machine.as_ref().unwrap().name,
            "mac2"
        );
        assert_eq!(r.view.lock().unwrap().terminals[0].id, "a");
        fake.terminals.lock().unwrap().push(card("b", "waiting"));
        let (stream, mut out) = fake.events.lock().unwrap().take().unwrap();
        writeln!(
            out,
            "{}",
            json!({ "stream": stream, "type": "event", "event": { "event": "waiting", "terminal": "b" } })
        )
        .unwrap();
        wait(&rx, &r.view, |v| v.terminals.len() == 2);
        assert_eq!(
            r.view.lock().unwrap().terminals[1].status.as_deref(),
            Some("waiting")
        );
    }

    #[test]
    fn a_failed_connection_shows_why_and_retries() {
        let (tx, rx) = channel();
        let tries = Arc::new(Mutex::new(0));
        let counted = Arc::clone(&tries);
        let spec = Spec {
            host: "mac2".into(),
            path: None,
        };
        let r = Remote::start(
            spec,
            move |_| {
                *counted.lock().unwrap() += 1;
                Err(io::Error::other("ssh: Could not resolve hostname mac2"))
            },
            move || {
                let _ = tx.send(());
            },
        );
        wait(
            &rx,
            &r.view,
            |v| matches!(&v.link, Link::Offline(w) if w.contains("resolve")),
        );
        wait(&rx, &r.view, |_| *tries.lock().unwrap() >= 2);
        drop(r);
    }

    #[test]
    fn the_command_runs_mooring_through_ssh() {
        let named = Spec {
            host: "mac2".into(),
            path: Some("/opt/My Apps/mooring".into()),
        };
        let argv = named.command();
        assert_eq!(argv[0], "ssh");
        assert!(argv.contains(&"BatchMode=yes".to_owned()));
        assert_eq!(argv[argv.len() - 2], "mac2");
        assert_eq!(
            argv.last().unwrap(),
            "exec '/opt/My Apps/mooring' remote --stdio"
        );
        let unnamed = Spec {
            host: "mac2".into(),
            path: None,
        };
        let probe = unnamed.command().last().unwrap().clone();
        assert!(probe.contains("command -v mooring"));
        assert!(probe.contains("$HOME/Applications/Mooring.app"));
        assert!(probe.contains("p=/Applications/Mooring.app"));
    }
}
