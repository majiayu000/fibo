use mooring_core::{model::*, protocol::*};
use std::{
    fs,
    io::{self, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) fn env_num(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
pub(crate) fn wall_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

/// Agent-like output: a stamped line every millisecond (or flat out), and a title
/// spinner the way Claude Code animates its tab title.
pub(crate) const PRODUCER: &str = r#"
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

pub(crate) struct Fixture {
    pub(crate) root: PathBuf,
    pub(crate) runtime: PathBuf,
    pub(crate) host: crate::tied::Tied,
}
impl Fixture {
    pub(crate) fn socket(&self) -> PathBuf {
        self.runtime.join("workbench.sock")
    }
    pub(crate) fn call(&self, request: &Request) -> io::Result<Reply> {
        rpc(&self.socket(), request)
    }
    pub(crate) fn workspace(&self) -> Workspace {
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

pub(crate) fn host_env(home: &Path) -> Vec<(String, String)> {
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
pub(crate) struct Conversation {
    pub(crate) terminal: String,
    pub(crate) agent: Agent,
    pub(crate) cli_id: String,
    pub(crate) path: PathBuf,
    pub(crate) cwd: String,
}
impl Conversation {
    pub(crate) fn header(&self) -> String {
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
    pub(crate) fn batch(&self, n: u64) -> String {
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
    pub(crate) fn hook_payload(&self, event: &str) -> String {
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
pub(crate) fn spawn_hook(
    rt: &Path,
    home: &Path,
    c: &Conversation,
    event: &str,
) -> io::Result<Child> {
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
pub(crate) struct Samples {
    /// (arrival wall ns, arrival − stamp ns) per stamped line.
    pub(crate) latency: Vec<(u64, u64)>,
    /// (arrival wall ns, ns since the previous OUTPUT frame) per frame.
    pub(crate) gaps: Vec<(u64, u64)>,
    /// Frames that came as a resync after the watcher lagged.
    pub(crate) resyncs: Vec<u64>,
}

/// Watches one terminal and stamps every line as it arrives.
/// No read timeout: a stall of any length is a sample, not a failure. The stream is shut
/// down from outside when the run ends.
pub(crate) fn watch(
    mut stream: UnixStream,
    id: String,
    out: Arc<Mutex<Samples>>,
    stop: Arc<AtomicBool>,
) {
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
pub(crate) struct Stamps {
    out: Arc<Mutex<Samples>>,
    local: Samples,
    last: u64,
    digits: Option<u32>,
    value: u64,
    flushed: Instant,
}
impl Stamps {
    pub(crate) fn new(out: Arc<Mutex<Samples>>) -> Self {
        Self {
            out,
            local: Samples::default(),
            last: 0,
            digits: None,
            value: 0,
            flushed: Instant::now(),
        }
    }
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
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
    pub(crate) fn flush(&mut self) {
        let mut o = self.out.lock().unwrap();
        o.latency.append(&mut self.local.latency);
        o.gaps.append(&mut self.local.gaps);
        o.resyncs.append(&mut self.local.resyncs);
        self.flushed = Instant::now();
    }
}

pub(crate) fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}
pub(crate) fn ms(ns: u64) -> f64 {
    ns as f64 / 1e6
}
