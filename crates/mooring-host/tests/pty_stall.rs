//! PTY read stalls under load (docs/plan.md, step 0): how long a terminal's output can
//! sit in the host before an attached client gets it, while Agent logs are appended in
//! batches and hooks arrive at 50 per second.
//!
//! Runs the release host exactly as installed (`mooring serve`) in a private HOME and
//! runtime; nothing in the product is instrumented. Each terminal runs a producer that
//! writes `@<wall-clock ns>` lines (Agent-like output, a title spinner); a watcher attached
//! to every terminal stamps each line on arrival. Delivery latency = arrival − stamp, so a
//! pump that cannot read (or cannot hand off what it read) shows up as latency. Hooks are
//! the real `mooring hook` CLI with a Claude / Codex payload on stdin; logs are Codex and
//! Claude JSONL under the private HOME, bound to the terminals by those hooks. An app-like
//! client re-reads the workspace on every recognition status, as the window does.
//!
//! Four phases of STALL_SECONDS each: baseline (terminals only), logs, hooks, logs+hooks.
//! `pty_floor_without_host` runs the same producers on bare PTYs read by the harness
//! itself: the machine's own floor, to read the host's numbers against.
//!
//!   DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
//!   cargo test --release -p mooring-host --test pty_stall -- --ignored --nocapture --test-threads 1
//!
//! The reports also land in target/tmp/pty-stall-report.txt and pty-floor-report.txt.
//! Knobs (env): STALL_SECONDS (20), STALL_TERMINALS (20), STALL_HOOKS_PER_SEC (50),
//! STALL_LOG_EVERY_MS (100), STALL_TITLE_MS (100, 0 = no title spinner),
//! STALL_PRODUCER (`ticks`: a line per ms; `flood`: write as fast as the PTY takes),
//! STALL_PHASES (e.g. `0,3`), STALL_KEEP (keep the private root and host.log),
//! STALL_SAMPLE (run macOS `sample` on the host during each window into sample-<n>.txt,
//! with STALL_KEEP; `sample` suspends the host while it walks stacks, so a sampled run
//! is for attribution only, never for the numbers).
#[path = "support/tied.rs"]
mod tied;
use mooring_core::{model::*, protocol::*};
use std::{
    fs,
    io::{self, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tied::SpawnTied;

fn env_num(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
fn wall_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

/// Agent-like output: a stamped line every millisecond (or flat out), and a title
/// spinner the way Claude Code animates its tab title.
const PRODUCER: &str = r#"
import os, sys, time
title_ms, flood = int(sys.argv[1]), sys.argv[2] == 'flood'
pad = ('working on the task, reading files and running tests ' * 2)[:90]
spin = '✳✶✻✽'
next_title = time.monotonic(); k = 0
while True:
    now = time.monotonic()
    if title_ms and now >= next_title:
        os.write(1, ('\x1b]0;%s pty stall\x07' % spin[k % 4]).encode()); k += 1
        next_title = now + title_ms / 1000
    if flood:
        os.write(1, ''.join('@%d %s\n' % (time.time_ns(), pad) for _ in range(40)).encode())
    else:
        os.write(1, ('@%d %s\n' % (time.time_ns(), pad)).encode())
        time.sleep(0.001)
"#;

struct Fixture {
    root: PathBuf,
    runtime: PathBuf,
    host: tied::Tied,
}
impl Fixture {
    fn socket(&self) -> PathBuf {
        self.runtime.join("workbench.sock")
    }
    fn call(&self, request: &Request) -> io::Result<Reply> {
        rpc(&self.socket(), request)
    }
    fn workspace(&self) -> Workspace {
        match self.call(&Request::Workspace).unwrap() {
            Reply::Workspace(w) => w,
            x => panic!("{x:?}"),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.host.kill_tree();
        if std::env::var_os("STALL_KEEP").is_none() {
            let _ = fs::remove_dir_all(&self.root);
        } else {
            eprintln!("kept {}", self.root.display());
        }
    }
}

fn host_env(home: &Path) -> Vec<(String, String)> {
    let h = |p: &str| home.join(p).to_string_lossy().into_owned();
    vec![
        ("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()),
        ("HOME".into(), home.to_string_lossy().into_owned()),
        ("CODEX_HOME".into(), h(".codex")),
        ("CLAUDE_CONFIG_DIR".into(), h(".claude")),
        ("GROK_HOME".into(), h(".grok")),
        ("LANG".into(), "en_US.UTF-8".into()),
    ]
}

/// One synthetic conversation per terminal: even terminals Codex, odd ones Claude.
struct Conversation {
    terminal: String,
    agent: Agent,
    cli_id: String,
    path: PathBuf,
    cwd: String,
}
impl Conversation {
    fn header(&self) -> String {
        let now = mooring_core::content::now();
        match self.agent {
            Agent::Codex => [
                serde_json::json!({"timestamp": now, "type": "session_meta", "payload": {"id": self.cli_id, "cwd": self.cwd, "source": "cli", "originator": "codex-tui", "thread_source": "user"}}),
                serde_json::json!({"timestamp": now, "type": "event_msg", "payload": {"type": "user_message", "message": format!("压测 {}", self.terminal)}}),
                serde_json::json!({"timestamp": now, "type": "event_msg", "payload": {"type": "task_started", "turn_id": format!("{}-turn", self.cli_id)}}),
            ]
            .map(|v| format!("{v}\n"))
            .concat(),
            _ => format!(
                "{}\n",
                serde_json::json!({"type": "user", "sessionId": self.cli_id, "uuid": format!("u-{}", self.cli_id), "cwd": self.cwd, "entrypoint": "cli", "userType": "external", "timestamp": now, "message": {"content": format!("压测 {}", self.terminal)}})
            ),
        }
    }
    /// One batch: a tool call, its output and a message, as an Agent writes them.
    fn batch(&self, n: u64) -> String {
        let now = mooring_core::content::now();
        let call = format!("{}-{n}", self.cli_id);
        match self.agent {
            Agent::Codex => [
                serde_json::json!({"timestamp": now, "type": "response_item", "payload": {"type": "function_call", "name": "exec_command", "call_id": call, "arguments": "{\"cmd\":\"cargo test\"}"}}),
                serde_json::json!({"timestamp": now, "type": "response_item", "payload": {"type": "function_call_output", "call_id": call, "output": "Process exited with code 0\ntest result: ok. 4 passed; 0 failed"}}),
                serde_json::json!({"timestamp": now, "type": "event_msg", "payload": {"type": "agent_message", "message": format!("第 {n} 步完成")}}),
            ],
            _ => [
                serde_json::json!({"type": "assistant", "sessionId": self.cli_id, "timestamp": now, "message": {"content": [{"type": "tool_use", "id": call, "name": "Bash", "input": {"command": "cargo test"}}]}}),
                serde_json::json!({"type": "user", "sessionId": self.cli_id, "timestamp": now, "message": {"content": [{"type": "tool_result", "tool_use_id": call, "content": "test result: ok. 4 passed"}]}}),
                serde_json::json!({"type": "assistant", "sessionId": self.cli_id, "timestamp": now, "message": {"content": [{"type": "text", "text": format!("第 {n} 步完成")}]}}),
            ],
        }
        .map(|v| format!("{v}\n"))
        .concat()
    }
    /// What `mooring hook` gets on stdin from the CLI.
    fn hook_payload(&self, event: &str) -> String {
        serde_json::json!({
            "session_id": self.cli_id,
            "transcript_path": self.path,
            "cwd": self.cwd,
            "hook_event_name": event,
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test"},
        })
        .to_string()
    }
}

/// Runs the real hook CLI as an Agent's hook does, from inside the terminal's environment.
fn spawn_hook(rt: &Path, home: &Path, c: &Conversation, event: &str) -> io::Result<Child> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["hook", "--agent", c.agent.executable()])
        .env_clear()
        .envs(host_env(home))
        .env(RUNTIME_ENV, rt)
        .env(TERMINAL_ENV, &c.terminal)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(c.hook_payload(event).as_bytes())?;
    Ok(child)
}

#[derive(Default)]
struct Samples {
    /// (arrival wall ns, arrival − stamp ns) per stamped line.
    latency: Vec<(u64, u64)>,
    /// (arrival wall ns, ns since the previous OUTPUT frame) per frame.
    gaps: Vec<(u64, u64)>,
    /// Frames that came as a resync after the watcher lagged.
    resyncs: Vec<u64>,
}

/// Watches one terminal and stamps every line as it arrives.
/// No read timeout: a stall of any length is a sample, not a failure. The stream is shut
/// down from outside when the run ends.
fn watch(mut stream: UnixStream, id: String, out: Arc<Mutex<Samples>>, stop: Arc<AtomicBool>) {
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    assert!(matches!(
        receive::<Reply>(&mut stream).unwrap(),
        Reply::Hello { .. }
    ));
    send(
        &mut stream,
        CONTROL,
        &Request::Attach {
            id: id.clone(),
            takeover: false,
            watch: true,
        },
    )
    .unwrap();
    assert!(matches!(
        receive::<Reply>(&mut stream).unwrap(),
        Reply::Attached { .. }
    ));
    let mut ready = false;
    let mut stamps = Stamps::new(out);
    while !stop.load(Ordering::Relaxed) {
        let (kind, bytes) = match read_frame(&mut stream) {
            Ok(f) => f,
            Err(_) if stop.load(Ordering::Relaxed) => break,
            Err(e) => panic!("terminal {id} stream: {e}"),
        };
        match kind {
            READY => ready = true,
            SNAPSHOT if ready => stamps.local.resyncs.push(wall_ns()),
            OUTPUT => stamps.feed(&bytes),
            _ => {}
        }
    }
    stamps.flush();
}

/// Reads `@<ns>` stamps out of a byte stream as it arrives, across chunk boundaries.
struct Stamps {
    out: Arc<Mutex<Samples>>,
    local: Samples,
    last: u64,
    digits: Option<u32>,
    value: u64,
    flushed: Instant,
}
impl Stamps {
    fn new(out: Arc<Mutex<Samples>>) -> Self {
        Self {
            out,
            local: Samples::default(),
            last: 0,
            digits: None,
            value: 0,
            flushed: Instant::now(),
        }
    }
    fn feed(&mut self, bytes: &[u8]) {
        let at = wall_ns();
        if self.last != 0 {
            self.local.gaps.push((at, at - self.last));
        }
        self.last = at;
        for &b in bytes {
            match (b, self.digits) {
                (b'@', _) => (self.digits, self.value) = (Some(0), 0),
                (b'0'..=b'9', Some(n)) => {
                    self.value = self.value * 10 + u64::from(b - b'0');
                    self.digits = Some(n + 1);
                }
                (_, Some(n)) => {
                    if n >= 18 {
                        self.local.latency.push((at, at.saturating_sub(self.value)));
                    }
                    self.digits = None;
                }
                _ => {}
            }
        }
        if self.flushed.elapsed() > Duration::from_millis(500) {
            self.flush();
        }
    }
    fn flush(&mut self) {
        let mut o = self.out.lock().unwrap();
        o.latency.append(&mut self.local.latency);
        o.gaps.append(&mut self.local.gaps);
        o.resyncs.append(&mut self.local.resyncs);
        self.flushed = Instant::now();
    }
}

/// The floor without Mooring: the same producers on plain PTYs, each read by its own
/// thread with poll + read, as the host's pump does, and stamped right there. Whatever
/// latency this shows is the machine and the measurement, not the host.
#[test]
#[ignore = "load measurement, run on demand: see the file header"]
fn pty_floor_without_host() {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    let seconds = env_num("STALL_SECONDS", 20);
    let terminals = env_num("STALL_TERMINALS", 20) as usize;
    let title_ms = env_num("STALL_TITLE_MS", 100);
    let producer = std::env::var("STALL_PRODUCER").unwrap_or_else(|_| "ticks".into());
    let stop = Arc::new(AtomicBool::new(false));
    let mut children = vec![];
    let mut readers = vec![];
    let samples: Vec<_> = (0..terminals)
        .map(|_| Arc::new(Mutex::new(Samples::default())))
        .collect();
    for s in &samples {
        let (mut master, mut slave) = (-1, -1);
        let mut size = libc::winsize {
            ws_row: 40,
            ws_col: 120,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut size,
                )
            },
            0
        );
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        let mut command = Command::new("/usr/bin/python3");
        command
            .args(["-c", PRODUCER, &title_ms.to_string(), &producer])
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                libc::signal(libc::SIGHUP, libc::SIG_DFL);
                libc::ioctl(0, libc::TIOCSCTTY as libc::c_ulong, 0);
                Ok(())
            });
        }
        children.push(command.spawn().unwrap());
        let (s, stop) = (s.clone(), stop.clone());
        readers.push(thread::spawn(move || {
            use std::io::Read;
            let mut file = fs::File::from(master);
            let mut bytes = vec![0; 64 * 1024];
            let mut stamps = Stamps::new(s);
            while !stop.load(Ordering::Relaxed) {
                let mut poll = libc::pollfd {
                    fd: file.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                if unsafe { libc::poll(&mut poll, 1, 200) } <= 0 {
                    continue;
                }
                match file.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => stamps.feed(&bytes[..n]),
                }
            }
            stamps.flush();
        }));
    }
    thread::sleep(Duration::from_secs(3));
    let start = wall_ns();
    thread::sleep(Duration::from_secs(seconds));
    let end = wall_ns();
    // Readers keep draining while the producers die: macOS holds a closing terminal open
    // until its output is read.
    for c in &mut children {
        let _ = c.kill();
    }
    for mut c in children {
        let _ = c.wait();
    }
    stop.store(true, Ordering::Relaxed);
    for r in readers {
        let _ = r.join();
    }
    let (mut latency, mut gaps) = (vec![], vec![]);
    for s in &samples {
        let s = s.lock().unwrap();
        let inside = |at: &u64| (start..end).contains(at);
        latency.extend(
            s.latency
                .iter()
                .filter(|(at, _)| inside(at))
                .map(|(_, l)| *l),
        );
        gaps.extend(s.gaps.iter().filter(|(at, _)| inside(at)).map(|(_, g)| *g));
    }
    latency.sort_unstable();
    gaps.sort_unstable();
    let line = format!(
        "不经过 Mooring 的对照（{terminals} 个裸 PTY，{producer}，标题每 {title_ms}ms，{seconds}s）：延迟 p50 {:.2} / p99 {:.2} / p99.9 {:.2} / 最大 {:.2}（{} 行）；间隔 p99 {:.2} / 最大 {:.2}",
        ms(percentile(&latency, 0.5)),
        ms(percentile(&latency, 0.99)),
        ms(percentile(&latency, 0.999)),
        ms(latency.last().copied().unwrap_or(0)),
        latency.len(),
        ms(percentile(&gaps, 0.99)),
        ms(gaps.last().copied().unwrap_or(0)),
    );
    eprintln!("{line}");
    fs::write(
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("pty-floor-report.txt"),
        line + "\n",
    )
    .unwrap();
    assert!(!latency.is_empty(), "no output was read");
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}
fn ms(ns: u64) -> f64 {
    ns as f64 / 1e6
}

#[test]
#[ignore = "load measurement, run on demand: see the file header"]
fn pty_read_stall_under_load() {
    let seconds = env_num("STALL_SECONDS", 20);
    let terminals = env_num("STALL_TERMINALS", 20) as usize;
    let hooks_per_sec = env_num("STALL_HOOKS_PER_SEC", 50);
    let log_every = Duration::from_millis(env_num("STALL_LOG_EVERY_MS", 100));
    let title_ms = env_num("STALL_TITLE_MS", 100);
    let producer = std::env::var("STALL_PRODUCER").unwrap_or_else(|_| "ticks".into());

    // Short path: the socket must fit SUN_LEN. The host creates the runtime itself (0700).
    let root = PathBuf::from(format!("/tmp/mstall-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let home = root.join("home");
    let runtime = root.join("rt");
    let project = root.join("project");
    fs::create_dir_all(home.join(".codex/sessions/2026/11/03")).unwrap();
    fs::create_dir_all(home.join(".claude/projects/-tmp-mstall")).unwrap();
    fs::create_dir_all(&project).unwrap();
    let host = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["serve", "--runtime"])
        .arg(&runtime)
        .env_clear()
        .envs(host_env(&home))
        .stdout(Stdio::null())
        .stderr(fs::File::create(root.join("host.log")).unwrap())
        .spawn_tied()
        .unwrap();
    let mut f = Fixture {
        root: root.clone(),
        runtime: runtime.clone(),
        host,
    };
    let until = Instant::now() + Duration::from_secs(30);
    while !f.socket().exists() {
        assert!(f.host.try_wait().unwrap().is_none(), "host exited");
        assert!(Instant::now() < until, "host never listened");
        thread::sleep(Duration::from_millis(50));
    }
    f.call(&Request::Edit {
        request: None,
        edit: Edit::AddSpace(Space {
            id: "s".into(),
            name: "压测".into(),
            cwd: project.to_string_lossy().into_owned(),
        }),
    })
    .unwrap();

    let cwd = project.to_string_lossy().into_owned();
    let conversations: Vec<Conversation> = (0..terminals)
        .map(|i| {
            let agent = if i % 2 == 0 {
                Agent::Codex
            } else {
                Agent::Claude
            };
            let cli_id = format!("{:08x}-0000-4000-8000-{:012x}", 0x57a11u32 + i as u32, i);
            let path = match agent {
                Agent::Codex => home.join(format!(
                    ".codex/sessions/2026/11/03/rollout-2026-11-03T00-00-00-{cli_id}.jsonl"
                )),
                _ => home.join(format!(".claude/projects/-tmp-mstall/{cli_id}.jsonl")),
            };
            Conversation {
                terminal: format!("t{i}"),
                agent,
                cli_id,
                path,
                cwd: cwd.clone(),
            }
        })
        .collect();
    for c in &conversations {
        fs::write(&c.path, c.header()).unwrap();
        let reply = f
            .call(&Request::Launch {
                id: c.terminal.clone(),
                space: "s".into(),
                argv: vec![
                    "/usr/bin/python3".into(),
                    "-c".into(),
                    PRODUCER.into(),
                    title_ms.to_string(),
                    producer.clone(),
                ],
                cwd: cwd.clone(),
                cols: 120,
                rows: 40,
                scheme: Default::default(),
                colors: None,
            })
            .unwrap();
        assert!(!matches!(reply, Reply::Error(_)), "{reply:?}");
    }
    // Bind every conversation to its terminal the way a starting Agent does.
    thread::sleep(Duration::from_secs(1));
    let mut bind = vec![];
    for c in &conversations {
        let started = Instant::now();
        let out = spawn_hook(&runtime, &home, c, "SessionStart")
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert!(
            out.status.success(),
            "bind {}: {}",
            c.terminal,
            String::from_utf8_lossy(&out.stderr)
        );
        bind.push(started.elapsed().as_nanos() as u64);
    }
    bind.sort_unstable();
    let bound = f
        .workspace()
        .agent_sessions
        .iter()
        .filter(|s| s.current && s.terminal.is_some())
        .count();
    assert_eq!(bound, terminals, "every terminal has its conversation");

    let stop = Arc::new(AtomicBool::new(false));
    let samples: Vec<_> = conversations
        .iter()
        .map(|_| Arc::new(Mutex::new(Samples::default())))
        .collect();
    let mut streams = vec![];
    let watchers: Vec<_> = conversations
        .iter()
        .zip(&samples)
        .map(|(c, s)| {
            let stream = UnixStream::connect(f.socket()).unwrap();
            streams.push(stream.try_clone().unwrap());
            let (id, s, stop) = (c.terminal.clone(), s.clone(), stop.clone());
            thread::spawn(move || watch(stream, id, s, stop))
        })
        .collect();

    // The window: re-reads the workspace on every recognition status.
    let app_reads = Arc::new(AtomicU64::new(0));
    {
        let (socket, reads, stop) = (f.socket(), app_reads.clone(), stop.clone());
        thread::spawn(move || {
            let mut s = UnixStream::connect(&socket).unwrap();
            send(&mut s, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
            receive::<Reply>(&mut s).unwrap();
            send(&mut s, CONTROL, &Request::WatchRecognition).unwrap();
            while !stop.load(Ordering::Relaxed) {
                if receive::<Reply>(&mut s).is_err() {
                    return;
                }
                if rpc(&socket, &Request::Workspace).is_ok() {
                    reads.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    let logs_on = Arc::new(AtomicBool::new(false));
    let hooks_on = Arc::new(AtomicBool::new(false));
    let conversations = Arc::new(conversations);
    let log_batches = Arc::new(AtomicU64::new(0));
    {
        let (cs, on, stop, batches) = (
            conversations.clone(),
            logs_on.clone(),
            stop.clone(),
            log_batches.clone(),
        );
        thread::spawn(move || {
            let mut next = Instant::now();
            let mut n = 0u64;
            while !stop.load(Ordering::Relaxed) {
                next += log_every;
                thread::sleep(next.saturating_duration_since(Instant::now()));
                if !on.load(Ordering::Relaxed) {
                    continue;
                }
                n += 1;
                for c in cs.iter() {
                    let mut file = fs::OpenOptions::new().append(true).open(&c.path).unwrap();
                    file.write_all(c.batch(n).as_bytes()).unwrap();
                }
                batches.fetch_add(1, Ordering::Relaxed);
            }
        });
    }
    // (sent wall ns, run time ns, ok)
    let hook_runs = Arc::new(Mutex::new(Vec::<(u64, u64, bool)>::new()));
    let hook_errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let hooks_sent = Arc::new(Mutex::new(Vec::<u64>::new()));
    {
        let (tx, rx) = mpsc::channel::<(u64, Instant, Child)>();
        let (runs, errors) = (hook_runs.clone(), hook_errors.clone());
        thread::spawn(move || {
            // One waiter per hook: a slow one must not hold back the others' timings.
            for (sent, started, child) in rx {
                let (runs, errors) = (runs.clone(), errors.clone());
                thread::spawn(move || {
                    let out = child.wait_with_output().unwrap();
                    let took = started.elapsed().as_nanos() as u64;
                    if !out.status.success() {
                        errors
                            .lock()
                            .unwrap()
                            .push(String::from_utf8_lossy(&out.stderr).into_owned());
                    }
                    runs.lock()
                        .unwrap()
                        .push((sent, took, out.status.success()));
                });
            }
        });
        let sent_log = hooks_sent.clone();
        let (cs, on, stop, rt, home) = (
            conversations.clone(),
            hooks_on.clone(),
            stop.clone(),
            runtime.clone(),
            home.clone(),
        );
        thread::spawn(move || {
            let every = Duration::from_nanos(1_000_000_000 / hooks_per_sec.max(1));
            let events = [
                "PreToolUse",
                "PostToolUse",
                "UserPromptSubmit",
                "Notification",
            ];
            let mut next = Instant::now();
            let mut n = 0usize;
            while !stop.load(Ordering::Relaxed) {
                next += every;
                thread::sleep(next.saturating_duration_since(Instant::now()));
                if !on.load(Ordering::Relaxed) {
                    continue;
                }
                let c = &cs[n % cs.len()];
                let event = events[(n / cs.len()) % events.len()];
                n += 1;
                let started = Instant::now();
                let sent = wall_ns();
                if let Ok(child) = spawn_hook(&rt, &home, c, event) {
                    sent_log.lock().unwrap().push(sent);
                    let _ = tx.send((sent, started, child));
                }
            }
        });
    }

    // How many `ps` the host has running at once (one per hook in flight), twice a second.
    let in_flight = Arc::new(Mutex::new(Vec::<(u64, usize)>::new()));
    {
        let (out, stop, host) = (in_flight.clone(), stop.clone(), f.host.id());
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let n = Command::new("/usr/bin/pgrep")
                    .args(["-P", &host.to_string(), "-x", "ps"])
                    .output()
                    .map_or(0, |o| {
                        o.stdout
                            .split(|b| *b == b'\n')
                            .filter(|l| !l.is_empty())
                            .count()
                    });
                out.lock().unwrap().push((wall_ns(), n));
                thread::sleep(Duration::from_millis(500));
            }
        });
    }
    // Let the bind settle and the producers warm up before the first window.
    thread::sleep(Duration::from_secs(3));
    let phases = [
        ("baseline（只有终端输出）", false, false),
        ("日志批量写", true, false),
        ("Hook", false, true),
        ("日志 + Hook", true, true),
    ];
    // STALL_PHASES=0,2 runs only those phases (0 baseline, 1 logs, 2 hooks, 3 both).
    let only: Option<Vec<usize>> = std::env::var("STALL_PHASES")
        .ok()
        .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect());
    let mut windows = vec![];
    let mut notes = vec![];
    for (n, (name, logs, hooks)) in phases.into_iter().enumerate() {
        if only.as_ref().is_some_and(|o| !o.contains(&n)) {
            continue;
        }
        // Hooks and batches from the previous phase drain before the window opens: a
        // backlog of hooks shows as `ps` children of the host (one per hook).
        logs_on.store(false, Ordering::Relaxed);
        hooks_on.store(false, Ordering::Relaxed);
        thread::sleep(Duration::from_secs(2));
        let drain = Instant::now();
        while Command::new("/usr/bin/pgrep")
            .args(["-P", &f.host.id().to_string(), "-x", "ps"])
            .output()
            .is_ok_and(|o| !o.stdout.is_empty())
            && drain.elapsed() < Duration::from_secs(120)
        {
            thread::sleep(Duration::from_millis(200));
        }
        if drain.elapsed() > Duration::from_millis(500) {
            notes.push(format!(
                "（{name} 开始前等上一阶段积压的 Hook 清空：{:.1}s）",
                drain.elapsed().as_secs_f64()
            ));
        }
        logs_on.store(logs, Ordering::Relaxed);
        hooks_on.store(hooks, Ordering::Relaxed);
        let start = wall_ns();
        let batches = log_batches.load(Ordering::Relaxed);
        let reads = app_reads.load(Ordering::Relaxed);
        // Where the host's threads wait, without touching the host: macOS `sample`.
        let sampler = std::env::var_os("STALL_SAMPLE").map(|_| {
            Command::new("/usr/bin/sample")
                .arg(f.host.id().to_string())
                .arg(seconds.saturating_sub(1).max(1).to_string())
                .arg("-file")
                .arg(root.join(format!("sample-{n}.txt")))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
        });
        thread::sleep(Duration::from_secs(seconds));
        if let Some(mut s) = sampler {
            let _ = s.wait();
        }
        let end = wall_ns();
        windows.push((
            name,
            start,
            end,
            log_batches.load(Ordering::Relaxed) - batches,
            app_reads.load(Ordering::Relaxed) - reads,
        ));
        assert!(f.host.try_wait().unwrap().is_none(), "host exited");
    }
    logs_on.store(false, Ordering::Relaxed);
    hooks_on.store(false, Ordering::Relaxed);
    thread::sleep(Duration::from_secs(2));
    stop.store(true, Ordering::Relaxed);
    for s in &streams {
        let _ = s.shutdown(std::net::Shutdown::Both);
    }
    for w in watchers {
        w.join().unwrap();
    }

    let mut report = String::new();
    macro_rules! say {
        ($($t:tt)*) => {{
            let line = format!($($t)*);
            eprintln!("{line}");
            report.push_str(&line);
            report.push('\n');
        }};
    }
    say!(
        "\nPTY 停顿压测：{terminals} 个终端（{producer}，标题每 {title_ms}ms 换一次），每阶段 {seconds}s，Hook {hooks_per_sec}/s，日志每 {}ms 一批（每个对话 3 行）",
        log_every.as_millis()
    );
    say!(
        "绑定用的 SessionStart Hook 逐个跑（无并发）：p50 {:.1} / 最大 {:.1}",
        ms(percentile(&bind, 0.5)),
        ms(*bind.last().unwrap())
    );
    for note in &notes {
        say!("{note}");
    }
    say!("延迟 = 输出写入 PTY 到 attach 的客户端收到；间隔 = 同一终端相邻两次收到输出；单位 ms");
    // Every hook sent has finished (or timed out in the CLI) before counting.
    let settle = Instant::now();
    while hook_runs.lock().unwrap().len() < hooks_sent.lock().unwrap().len()
        && settle.elapsed() < Duration::from_secs(30)
    {
        thread::sleep(Duration::from_millis(100));
    }
    let runs = hook_runs.lock().unwrap().clone();
    let sent_at = hooks_sent.lock().unwrap().clone();
    let mut summary = vec![];
    for (name, start, end, batches, reads) in &windows {
        let inside = |at: &u64| (*start..*end).contains(at);
        let mut latency = vec![];
        let mut gaps = vec![];
        let mut resyncs = 0;
        let mut worst = (0u64, String::new());
        for (c, s) in conversations.iter().zip(&samples) {
            let s = s.lock().unwrap();
            for (at, l) in s.latency.iter().filter(|(at, _)| inside(at)) {
                latency.push(*l);
                if *l > worst.0 {
                    worst = (
                        *l,
                        format!("{} @{:.1}s", c.terminal, (*at - start) as f64 / 1e9),
                    );
                }
            }
            gaps.extend(s.gaps.iter().filter(|(at, _)| inside(at)).map(|(_, g)| *g));
            resyncs += s.resyncs.iter().filter(|at| inside(at)).count();
        }
        latency.sort_unstable();
        gaps.sort_unstable();
        let mut hooks: Vec<_> = runs.iter().filter(|(at, ..)| inside(at)).collect();
        hooks.sort_by_key(|(_, took, _)| *took);
        let hook_failed = hooks.iter().filter(|(.., ok)| !ok).count();
        let sent = sent_at.iter().filter(|at| inside(at)).count();
        let ps_max = in_flight
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| inside(at))
            .map(|(_, n)| *n)
            .max()
            .unwrap_or(0);
        let took: Vec<u64> = hooks.iter().map(|(_, t, _)| *t).collect();
        say!(
            "{name}: 延迟 p50 {:.2} / p99 {:.2} / p99.9 {:.2} / 最大 {:.2}（{}，{} 行）；间隔 p99 {:.2} / 最大 {:.2}；重同步 {resyncs}；日志批 {batches}；Hook 发出 {sent} 个、结束 {} 个（失败 {hook_failed}，耗时 p50 {:.1} / p99 {:.1} / 最大 {:.1}）；宿主同时在跑的 ps 最多 {ps_max} 个；窗口重读工作区 {reads} 次",
            ms(percentile(&latency, 0.5)),
            ms(percentile(&latency, 0.99)),
            ms(percentile(&latency, 0.999)),
            ms(latency.last().copied().unwrap_or(0)),
            worst.1,
            latency.len(),
            ms(percentile(&gaps, 0.99)),
            ms(gaps.last().copied().unwrap_or(0)),
            hooks.len(),
            ms(percentile(&took, 0.5)),
            ms(percentile(&took, 0.99)),
            ms(took.last().copied().unwrap_or(0)),
        );
        summary.push((latency.len(), hooks.len(), hook_failed));
    }
    let errors = hook_errors.lock().unwrap();
    if !errors.is_empty() {
        say!(
            "Hook 失败 {} 个，第一个：{}",
            errors.len(),
            errors[0].trim()
        );
    }
    let log = fs::read_to_string(root.join("host.log")).unwrap_or_default();
    if !log.trim().is_empty() {
        say!(
            "host.log 末尾：\n{}",
            log.lines().rev().take(10).collect::<Vec<_>>().join("\n")
        );
    }
    drop(errors);
    let saved = Path::new(env!("CARGO_TARGET_TMPDIR")).join("pty-stall-report.txt");
    fs::write(&saved, &report).unwrap();
    eprintln!("报告另存：{}", saved.display());
    let _ = &mut f;
    // The measurement must have measured: output flowed in every window. Failed hooks
    // are a finding (the host could not keep up), reported above, not a harness error.
    for (lines, _, _) in &summary {
        assert!(*lines > 0, "no output reached the watchers");
    }
}
