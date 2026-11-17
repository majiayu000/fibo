//! A CLI Fibo was never compiled to know, added by a declaration file alone: the
//! daemon recognises it, binds its transcript to the terminal and can stop it, or, with no
//! transcript declared, says so on the terminal. See docs/architecture/PLUGIN_SYSTEM.md.
#[path = "support/tied.rs"]
mod tied;
use fibo_core::{model::*, protocol::*};
use std::{
    fs, io,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tied::SpawnTied;

struct Host {
    child: tied::Tied,
    root: PathBuf,
}
impl Host {
    /// A daemon whose config holds `declaration` as `agents/fake.json`.
    fn start(declaration: impl FnOnce(&Path) -> String) -> Self {
        Self::start_with(Some(declaration))
    }
    /// A daemon that knows only the built-ins until a plugin says otherwise.
    fn start_bare() -> Self {
        Self::start_with(None::<fn(&Path) -> String>)
    }
    fn start_with(declaration: Option<impl FnOnce(&Path) -> String>) -> Self {
        let root = std::env::temp_dir().join(format!("kl-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(root.join("agents")).unwrap();
        // The daemon only serves from a private runtime directory.
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir_all(root.join("logs/fake")).unwrap();
        if let Some(declaration) = declaration {
            fs::write(root.join("agents/fake.json"), declaration(&root)).unwrap();
        }
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "daemon_fixture", "--nocapture"])
            .env("MOORING_FIXTURE_RUNTIME", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn_tied()
            .unwrap();
        let h = Self { child, root };
        let until = Instant::now() + Duration::from_secs(30);
        while !h.root.join("workbench.sock").exists() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(20));
        }
        h.call(Request::Edit {
            request: None,
            edit: Edit::AddSpace(Space {
                id: "s".into(),
                name: "Tests".into(),
                cwd: "/tmp".into(),
            }),
        })
        .unwrap();
        h
    }
    fn call(&self, request: Request) -> io::Result<Reply> {
        rpc(&self.root.join("workbench.sock"), &request)
    }
    fn workspace(&self) -> Workspace {
        match self.call(Request::Workspace).unwrap() {
            Reply::Workspace(w) => w,
            x => panic!("{x:?}"),
        }
    }
    fn recognition(&self) -> RecognitionStatus {
        match self.call(Request::RecognitionStatus).unwrap() {
            Reply::Recognition(s) => s,
            x => panic!("{x:?}"),
        }
    }
    /// Runs `script` in a terminal as the declared CLI: `/bin/sh` under its name.
    fn run_cli(&self, id: &str, script: &str) {
        self.run_started(id, script, None)
    }
    /// `run_cli`, the host told what it starts the CLI with (as a phone's 接着做 tells it).
    fn run_started(&self, id: &str, script: &str, start: Option<AgentStart>) {
        self.run_resuming(id, script, "", start)
    }
    /// `run_started`, `cli_id` on the CLI's own command line after its script, as the host's
    /// resume command (`fake-cli --resume {id}`) puts it there.
    fn run_resuming(&self, id: &str, script: &str, cli_id: &str, start: Option<AgentStart>) {
        let cli = self.root.join("fake-cli");
        if !cli.exists() {
            std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
        }
        self.call(Request::Launch {
            id: id.into(),
            space: "s".into(),
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                format!(
                    "{} -c {} {}; read -r line",
                    shell_quote(&cli.to_string_lossy()),
                    shell_quote(script),
                    shell_quote(cli_id)
                ),
            ],
            cwd: "/tmp".into(),
            cols: 80,
            rows: 24,
            scheme: Default::default(),
            colors: None,
            start,
        })
        .unwrap();
    }
    /// Presses Enter in a terminal, which is what prompts a process scan.
    fn enter(&self, id: &str) -> UnixStream {
        let mut stream = UnixStream::connect(self.root.join("workbench.sock")).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
        receive::<Reply>(&mut stream).unwrap();
        send(
            &mut stream,
            CONTROL,
            &Request::Attach {
                id: id.into(),
                takeover: false,
                watch: false,
            },
        )
        .unwrap();
        receive::<Reply>(&mut stream).unwrap();
        while read_frame(&mut stream).unwrap().0 != READY {}
        send(
            &mut stream,
            CONTROL,
            &Request::Input {
                bytes: b"\n".to_vec(),
                lossy: false,
            },
        )
        .unwrap();
        stream
    }
    fn wait(&self, what: &str, ok: impl Fn(&Self) -> bool) {
        let until = Instant::now() + Duration::from_secs(30);
        while !ok(self) {
            assert!(Instant::now() < until, "{what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if let Ok(Reply::Workspace(w)) = self.call(Request::Workspace) {
            for t in w.terminals {
                let _ = self.call(Request::Stop { id: t.id });
            }
        }
        self.child.kill_tree();
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
#[ignore = "subprocess daemon entry for integration fixtures"]
fn daemon_fixture() {
    let root = PathBuf::from(
        std::env::var_os("MOORING_FIXTURE_RUNTIME").expect("fixture subprocess only"),
    );
    assert!(fibo_core::agent::install_from(&root.join("agents"), &root));
    // Plugins live under the same directory, so a reload can read new ones.
    fibo_core::plugin::install(&root);
    let fake = Agent::from_key("fake");
    let declared = fake.spec().is_some();
    let mut roots = if declared {
        vec![(fake, root.join("logs/fake"))]
    } else {
        vec![]
    };
    // The built-ins keep roots of their own here, so nothing of the user's is read.
    for agent in Agent::all().into_iter().filter(|a| *a != fake) {
        roots.push((agent, root.join("logs").join(agent.key())));
    }
    fibo_host::serve_with_roots(&root, roots).unwrap();
}

fn declaration(transcripts: bool) -> impl FnOnce(&Path) -> String {
    move |root| {
        let t = format!(
            r#","transcripts":{{"root":{},"session":"/sid","cwd":"/cwd","role":"/role","text":"/content"}}"#,
            serde_json::to_string(&root.join("logs/fake")).unwrap()
        );
        format!(
            r#"{{"api":1,"id":"fake","name":"Fake CLI","executable":"fake-cli","start":"fake-cli","resume":"fake-cli --resume {{id}}"{}}}"#,
            if transcripts { t.as_str() } else { "" }
        )
    }
}

#[test]
fn a_declared_cli_binds_its_transcript_and_stop_agent_ends_only_it() {
    let h = Host::start(declaration(true));
    let log = h.root.join("logs/fake/one.jsonl");
    let row =
        serde_json::json!({"sid":"fake-1","cwd":"/tmp","role":"user","content":"整理一下发布说明"});
    let pid_file = h.root.join("cli-pid");
    // The CLI holds its transcript open, as a real one does while it runs.
    h.run_cli(
        "pane",
        &format!(
            "exec 3>>{}; echo {} >&3; echo $$ > {}; read -r line",
            shell_quote(&log.to_string_lossy()),
            shell_quote(&row.to_string()),
            shell_quote(&pid_file.to_string_lossy())
        ),
    );
    h.wait(
        "the declared CLI's transcript never bound to its terminal",
        |h| {
            h.workspace()
                .current_agent("pane")
                .is_some_and(|s| s.runtime.key() == "fake" && s.cli_session_id == "fake-1")
        },
    );
    let w = h.workspace();
    let session = w.current_agent("pane").unwrap();
    assert_eq!(w.content[&session.id].title, "整理一下发布说明");

    let pid: i32 = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    h.call(Request::StopAgent { id: "pane".into() }).unwrap();
    h.wait("Stop Agent did not end the declared CLI", |h| {
        h.workspace().current_agent("pane").is_none()
    });
    assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "the CLI is still alive");
    let w = h.workspace();
    assert_eq!(
        w.terminals[0].state,
        ProcessState::Running,
        "the shell stays"
    );
    // Stored under its declared id, and read back as the same Agent.
    let db = rusqlite::Connection::open(h.root.join("workbench.sqlite")).unwrap();
    let runtime: String = db
        .query_row("SELECT runtime FROM agent_sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(runtime, "fake");
}

/// No restart: a plugin that declares a CLI takes effect on `ReloadPlugins`, and its
/// transcripts are read and bound; taken away the same way, a new one no longer is.
#[test]
fn an_agent_plugin_takes_effect_and_leaves_without_a_restart() {
    let h = Host::start_bare();
    let before = h.recognition().plugins_revision;
    let row =
        |sid: &str| serde_json::json!({"sid":sid,"cwd":"/tmp","role":"user","content":"热重载"});
    let writes = |log: &Path, sid: &str| {
        format!(
            "exec 3>>{}; echo {} >&3; read -r line",
            shell_quote(&log.to_string_lossy()),
            shell_quote(&row(sid).to_string()),
        )
    };
    let plugin = h.root.join("plugins/fake");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(
        plugin.join("plugin.json"),
        r#"{"api":1,"id":"fake","name":"Fake","provides":["agent"]}"#,
    )
    .unwrap();
    fs::write(plugin.join("agent.json"), declaration(true)(&h.root)).unwrap();
    assert!(matches!(h.call(Request::ReloadPlugins).unwrap(), Reply::Ok));
    assert!(
        h.recognition().plugins_revision > before,
        "windows are told"
    );

    h.run_cli(
        "pane",
        &writes(&h.root.join("logs/fake/one.jsonl"), "fake-1"),
    );
    h.wait("a plugin's CLI was not bound without a restart", |h| {
        h.workspace()
            .current_agent("pane")
            .is_some_and(|s| s.runtime.key() == "fake" && s.cli_session_id == "fake-1")
    });
    fs::remove_dir_all(&plugin).unwrap();
    assert!(matches!(h.call(Request::ReloadPlugins).unwrap(), Reply::Ok));
    h.run_cli(
        "gone",
        &writes(&h.root.join("logs/fake/two.jsonl"), "fake-2"),
    );
    std::thread::sleep(Duration::from_secs(3));
    let w = h.workspace();
    assert!(
        w.current_agent("gone").is_none(),
        "no longer declared, not bound"
    );
    // The session it already had reads back, as any Agent no longer declared does.
    assert_eq!(
        w.current_agent("pane").map(|s| s.runtime.key()),
        Some("fake")
    );
}

#[test]
fn a_declared_cli_without_transcripts_is_named_on_its_terminal_until_it_exits() {
    let h = Host::start(declaration(false));
    // Two lines: the first is the Enter that prompts a scan, the second ends the CLI.
    // Pressed once it runs: a scan comes of the Enter alone, and sees what is there by then.
    let started = h.root.join("cli-started");
    let script = format!(
        ": > {}; read -r a; read -r b",
        shell_quote(&started.to_string_lossy())
    );
    h.run_cli("pane", &script);
    h.wait("the CLI never started", |_| started.exists());
    let mut stream = h.enter("pane");
    h.wait("no note for a CLI that keeps no transcript", |h| {
        h.recognition()
            .terminal_notes
            .get("pane")
            .is_some_and(|n| n.note == "fake-cli · 没有对话记录")
    });
    assert!(h.workspace().current_agent("pane").is_none());
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: b"\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    h.wait("the note outlives the CLI", |h| {
        let r = h.recognition();
        !r.terminal_notes.contains_key("pane") && !r.terminal_hints.contains_key("pane")
    });
}

#[test]
fn a_declared_extension_keeps_indexing_after_initial_import() {
    use std::io::Write;
    let row = |text: &str| {
        serde_json::json!({
            "sid":"fake-ndjson", "cwd":"/tmp", "role":"user", "content":text
        })
    };
    let h = Host::start(|root| {
        let mut spec: serde_json::Value = serde_json::from_str(&declaration(true)(root)).unwrap();
        spec["transcripts"]["extension"] = "ndjson".into();
        fs::write(
            root.join("logs/fake/one.ndjson"),
            format!("{}\n", row("first")),
        )
        .unwrap();
        spec.to_string()
    });
    let log = h.root.join("logs/fake/one.ndjson").canonicalize().unwrap();
    let indexed = |h: &Host, expected| {
        let db = rusqlite::Connection::open(h.root.join("workbench.sqlite")).unwrap();
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM session_records WHERE path=?1 AND kind='user'",
                [log.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .unwrap();
        count == expected
    };
    h.wait("custom transcript was not imported", |h| indexed(h, 1));
    for (expected, text) in [(2, "second"), (3, "third")] {
        let mut file = fs::OpenOptions::new().append(true).open(&log).unwrap();
        writeln!(file, "{}", row(text)).unwrap();
        file.flush().unwrap();
        drop(file);
        h.wait("ordinary append to the ndjson file was not indexed", |h| {
            indexed(h, expected)
        });
    }
}

/// OpenCode resumed with `--session <id>` names no conversation Fibo reads from its command
/// line, and writes nothing to its log until it is spoken to; nor does a phone press Enter.
/// The host started it with that conversation, so it ties the terminal before any message,
/// and the phone's chat has a conversation to send that first message to (RC3 walk). Not
/// before the CLI shows its input: OpenCode 1.19.5 hides the cursor, turns on the alternate
/// screen and loads, and drops what is typed until its prompt is focused and shows the cursor;
/// tied earlier, the phone's first message was accepted and lost (#403 review). Each step
/// waits on a file and nothing else looks at the terminal meanwhile, so undoing the start
/// evidence, the wait for the input or the host's watch for it fails this every time.
#[test]
fn a_declared_cli_the_host_resumed_binds_once_it_shows_its_input() {
    let h = Host::start(declaration(true));
    let row = serde_json::json!({"sid":"fake-old","cwd":"/tmp","role":"user","content":"整理一下发布说明"});
    fs::write(h.root.join("logs/fake/old.jsonl"), format!("{row}\n")).unwrap();
    h.wait("the earlier conversation is known", |h| {
        h.workspace()
            .agent_sessions
            .iter()
            .any(|s| s.cli_session_id == "fake-old")
    });
    // Whatever that log's write sets off is over before the terminal starts.
    std::thread::sleep(Duration::from_secs(3));
    let fake = Agent::from_key("fake");
    let (loading, focused) = (h.root.join("loading"), h.root.join("focused"));
    let wait = |f: &Path| {
        format!(
            "while [ ! -e {} ]; do sleep 0.05; done; ",
            shell_quote(&f.to_string_lossy())
        )
    };
    let script = format!(
        "{}printf '\\033[?25l\\033[?1049hRESUMING'; {}printf '\\033[2J\\033[HRESUMED\\033[?25h'; read -r line",
        wait(&loading),
        wait(&focused)
    );
    h.run_resuming(
        "resumed",
        &script,
        "fake-old",
        Some(AgentStart::resuming(fake, "fake-old")),
    );
    // Found as an Agent with no conversation yet, or (wrongly) tied to it already.
    h.wait("the host never found the CLI it started", |h| {
        h.recognition().terminal_agents.contains_key("resumed")
            || h.workspace().current_agent("resumed").is_some()
    });
    let untied = |what: &str| {
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            assert!(h.workspace().current_agent("resumed").is_none(), "{what}");
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    untied("tied before its CLI drew anything");
    fs::write(&loading, "").unwrap();
    untied("tied while its CLI loads with no input focused");
    fs::write(&focused, "").unwrap();
    h.wait("never tied once its CLI showed its input", |h| {
        h.workspace()
            .current_agent("resumed")
            .is_some_and(|s| s.cli_session_id == "fake-old")
    });
    let w = h.workspace();
    assert_eq!(
        w.current_agent("resumed").unwrap().binding,
        Some(Binding::Screen)
    );
    // Another terminal started resuming it while it runs there takes nothing, input or not.
    h.run_resuming(
        "again",
        "printf '\\033[?1049hRESUMED'; read -r line",
        "fake-old",
        Some(AgentStart::resuming(fake, "fake-old")),
    );
    std::thread::sleep(Duration::from_secs(3));
    let w = h.workspace();
    assert!(w.current_agent("again").is_none());
    assert!(w
        .current_agent("resumed")
        .is_some_and(|s| s.cli_session_id == "fake-old"));
}

/// The CLI the host started resuming a conversation exits before it ties anything (it could
/// not open that conversation, say). Its start is over: kept, any CLI of that kind started in
/// the shell left behind could be tied to the conversation, whatever it runs (#403 review).
#[test]
fn a_resume_start_ends_with_the_cli_it_started() {
    let h = Host::start(declaration(true));
    let row = serde_json::json!({"sid":"fake-old","cwd":"/tmp","role":"user","content":"整理一下发布说明"});
    fs::write(h.root.join("logs/fake/old.jsonl"), format!("{row}\n")).unwrap();
    h.wait("the earlier conversation is known", |h| {
        h.workspace()
            .agent_sessions
            .iter()
            .any(|s| s.cli_session_id == "fake-old")
    });
    let quit = h.root.join("quit");
    h.run_resuming(
        "resumed",
        &format!(
            "while [ ! -e {} ]; do sleep 0.05; done",
            shell_quote(&quit.to_string_lossy())
        ),
        "fake-old",
        Some(AgentStart::resuming(Agent::from_key("fake"), "fake-old")),
    );
    h.wait("the host never found the CLI it started", |h| {
        h.recognition().terminal_agents.contains_key("resumed")
    });
    assert!(h.workspace().agent_starts.contains_key("resumed"));
    fs::write(&quit, "").unwrap();
    h.wait("the start outlived the CLI it started", |h| {
        !h.workspace().agent_starts.contains_key("resumed")
    });
    let w = h.workspace();
    assert!(w.current_agent("resumed").is_none());
    assert!(w
        .terminals
        .iter()
        .any(|t| t.id == "resumed" && t.state == ProcessState::Running));
}
