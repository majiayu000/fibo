//! What the phone v2 protocol tests share: a host of their own from the test binary
//! (`common::daemon_fixture`) and `mooring remote --stdio` speaking to it.
#![allow(dead_code)]
use mooring_core::{model::*, protocol::*};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::{fs, io};

/// A host on its own runtime and log roots, from this binary (`daemon_fixture`).
pub struct Host {
    child: Child,
    pub root: PathBuf,
}
impl Host {
    pub fn start() -> Self {
        let root = std::env::temp_dir().join(format!("kl-{}", uuid::Uuid::new_v4().simple()));
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "common::daemon_fixture",
                "--nocapture",
            ])
            .env("MOORING_FIXTURE_RUNTIME", &root)
            .env("MOORING_CLAUDE_AGENTS", root.join("claude-agents"))
            .env("MOORING_CODEX_CONTROL", root.join("codex-control.sock"))
            .env("GROK_HOME", root.join("grok-home"))
            .env("MOORING_REMOTE_EXE", env!("CARGO_BIN_EXE_mooring"))
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let h = Self { child, root };
        let until = Instant::now() + Duration::from_secs(30);
        while rpc(&h.socket(), &Request::Workspace).is_err() {
            assert!(Instant::now() < until, "host never listened");
            std::thread::sleep(Duration::from_millis(20));
        }
        let space = Edit::AddSpace(Space {
            id: "s".into(),
            name: "Tests".into(),
            cwd: "/tmp".into(),
        });
        h.call(Request::Edit {
            request: None,
            edit: space,
        })
        .unwrap();
        h
    }
    pub fn socket(&self) -> PathBuf {
        self.root.join("workbench.sock")
    }
    pub fn call(&self, request: Request) -> io::Result<Reply> {
        rpc(&self.socket(), &request)
    }
    pub fn workspace(&self) -> Workspace {
        match self.call(Request::Workspace).unwrap() {
            Reply::Workspace(w) => w,
            x => panic!("{x:?}"),
        }
    }
    pub fn launch(&self, id: &str, cwd: &Path, cmd: &str) {
        self.call(Request::Launch {
            id: id.into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), cmd.into()],
            cwd: cwd.to_string_lossy().into_owned(),
            cols: 100,
            rows: 30,
            scheme: Default::default(),
            colors: None,
        })
        .unwrap();
    }
    pub fn screen(&self, id: &str) -> String {
        match self.call(Request::ReadScreen {
            id: id.into(),
            rows: 30,
        }) {
            Ok(Reply::Text(t)) => t,
            other => format!("{other:?}"),
        }
    }
    pub fn wait_screen(&self, id: &str, text: &str) {
        let until = Instant::now() + Duration::from_secs(20);
        while !self.screen(id).contains(text) {
            assert!(
                Instant::now() < until,
                "{text} never showed: {}",
                self.screen(id)
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    /// A fake Claude in `terminal`, bound by its explicit id, that writes `log` lines and
    /// then runs `script` (sh). Returns the conversation's log path.
    pub fn claude(&self, terminal: &str, cwd: &Path, rows: &[Value], script: &str) -> PathBuf {
        let logs = self.root.join("logs/claude");
        fs::create_dir_all(&logs).unwrap();
        let cli = self.root.join("claude");
        if !cli.exists() {
            std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
        }
        let id = uuid::Uuid::new_v4().to_string();
        let path = logs.join(format!("{id}.jsonl"));
        let seed = self.root.join(format!("seed-{terminal}"));
        let mut lines = String::new();
        for row in rows {
            let mut row = row.clone();
            row["sessionId"] = id.clone().into();
            row["timestamp"] = mooring_core::content::now().into();
            lines += &format!("{row}\n");
        }
        fs::write(&seed, lines).unwrap();
        let body = format!(
            "LOG={1}; cat {0} > {1}; {script}",
            shell_quote(&seed.to_string_lossy()),
            shell_quote(&path.to_string_lossy()),
        );
        self.launch(
            terminal,
            cwd,
            &format!(
                "exec {} -c {} fixture --session-id {id}",
                shell_quote(&cli.to_string_lossy()),
                shell_quote(&body),
            ),
        );
        let until = Instant::now() + Duration::from_secs(30);
        while self.workspace().current_agent(terminal).is_none() {
            assert!(Instant::now() < until, "explicit ID must bind");
            std::thread::sleep(Duration::from_millis(50));
        }
        path
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if let Ok(Reply::Workspace(w)) = self.call(Request::Workspace) {
            for t in w.terminals {
                let _ = self.call(Request::Stop { id: t.id });
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
#[ignore = "subprocess daemon entry for these tests"]
pub fn daemon_fixture() {
    let root = PathBuf::from(std::env::var_os("MOORING_FIXTURE_RUNTIME").expect("fixture only"));
    let roots = vec![
        (Agent::Codex, root.join("logs/codex")),
        (Agent::Claude, root.join("logs/claude")),
        (Agent::Grok, root.join("grok-home/sessions")),
    ];
    mooring_host::serve_with_roots(&root, roots).unwrap();
}

/// `mooring remote --stdio` on a host, greeted with `hello`.
pub struct Remote {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<Value>,
    n: u64,
    pub hello: Value,
}
impl Remote {
    pub fn open(h: &Host, role: Option<&str>, env: &[(&str, &Path)]) -> Self {
        let mut c = Command::new(env!("CARGO_BIN_EXE_mooring"));
        c.args(["remote", "--stdio", "--runtime"]).arg(&h.root);
        if let Some(role) = role {
            c.args(["--role", role]);
        }
        for (k, v) in env {
            c.env(k, v);
        }
        let mut child = c
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let out = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                if tx.send(serde_json::from_str(&line).unwrap()).is_err() {
                    break;
                }
            }
        });
        let mut r = Self {
            child,
            stdin,
            lines,
            n: 0,
            hello: Value::Null,
        };
        r.hello = r.ask(json!({ "op": "hello", "client": "phone-1" }));
        assert!(r.hello["ok"].is_object(), "{}", r.hello);
        r
    }
    pub fn ask(&mut self, mut v: Value) -> Value {
        self.n += 1;
        v["id"] = self.n.into();
        writeln!(self.stdin, "{v}").unwrap();
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(left).expect("no reply in time");
            if line["id"] == self.n {
                return line;
            }
        }
    }
    pub fn ok(&mut self, v: Value) -> Value {
        let reply = self.ask(v.clone());
        assert!(reply.get("error").is_none(), "{v} → {reply}");
        reply["ok"].clone()
    }
    pub fn error(&mut self, v: Value) -> String {
        let reply = self.ask(v.clone());
        reply["error"]
            .as_str()
            .unwrap_or_else(|| panic!("{v} should fail: {reply}"))
            .to_owned()
    }
    pub fn ops(&self) -> Vec<String> {
        serde_json::from_value(self.hello["ok"]["ops"].clone()).unwrap()
    }
}
impl Drop for Remote {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}
pub fn scratch(h: &Host, name: &str) -> PathBuf {
    let dir = h.root.join(name);
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub trait AddLines {
    fn add_lines(&self, n: usize) -> String;
}
impl AddLines for String {
    fn add_lines(&self, n: usize) -> String {
        (0..n).map(|i| format!("{i}{self}\n")).collect()
    }
}
