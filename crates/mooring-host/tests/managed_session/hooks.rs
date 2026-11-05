use super::*;

#[test]
fn a_grok_tui_is_bound_through_its_own_registry_and_its_stream_drives_the_card() {
    let h = Host::start();
    h.setup();
    let cli = h.root.join("grok");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let script = format!("echo $$ > {}; sleep 30", h.root.join("grok-pid").display());
    h.call(h.launch(
        "g",
        &format!(
            "{} -c {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    let pid: u32 = wait_file(&h.root.join("grok-pid")).trim().parse().unwrap();
    let home = h.root.join("grok-home");
    let session = home.join("sessions/%2FUsers%2Fme%2Frepo/01a1-live");
    fs::create_dir_all(&session).unwrap();
    fs::write(
        session.join("summary.json"),
        r#"{"info":{"id":"01a1-live"}}"#,
    )
    .unwrap();
    // A crashed process's entry with this pid but another start time is not this one.
    fs::write(
        home.join("active_sessions.json"),
        serde_json::json!([
            {"session_id": "01a0-stale", "pid": pid, "pid_start": 1, "cwd": "/old"},
            {"session_id": "01a1-live", "pid": pid, "cwd": "/Users/me/repo"},
        ])
        .to_string(),
    )
    .unwrap();
    let record = |at: u64, update: serde_json::Value| {
        serde_json::json!({"timestamp": at, "method": "session/update",
            "params": {"sessionId": "01a1-live", "update": update}})
        .to_string()
            + "\n"
    };
    let now = mooring_core::content::now();
    let transcript = session.join("updates.jsonl");
    fs::write(
        &transcript,
        record(
            now,
            serde_json::json!({"sessionUpdate": "user_message_chunk",
                "content": {"type": "text", "text": "给 Mooring 接上 Grok"}}),
        ),
    )
    .unwrap();
    let card = |turn: mooring_core::content::Turn| {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let w = h.workspace();
            if let Some(s) = w.current_agent("g") {
                if w.content.get(&s.id).is_some_and(|c| c.turn == turn) {
                    return (s.clone(), w.content[&s.id].clone());
                }
            }
            assert!(
                Instant::now() < until,
                "no {turn:?} Grok card: {:?}",
                w.agent_sessions
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    };
    let (s, c) = card(mooring_core::content::Turn::Running);
    assert_eq!(s.runtime, Agent::Grok);
    assert_eq!(s.cli_session_id, "01a1-live");
    assert_eq!(s.cwd, "/Users/me/repo");
    assert_eq!(c.title, "给 Mooring 接上 Grok");
    assert_eq!(c.hidden_reason(), None);
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap();
    use std::io::Write;
    file.write_all(
        (record(
            now + 1,
            serde_json::json!({"sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "已经接好了。"}}),
        ) + &record(
            now + 2,
            serde_json::json!({"sessionUpdate": "turn_completed", "stop_reason": "end_turn"}),
        ))
            .as_bytes(),
    )
    .unwrap();
    let (_, c) = card(mooring_core::content::Turn::Completed);
    assert_eq!(c.last_reply, "已经接好了。");
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    let until = Instant::now() + Duration::from_secs(5);
    while h.workspace().current_agent("g").is_some() {
        assert!(
            Instant::now() < until,
            "Grok exit left the conversation current"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    // `grok --resume` writes the transcript about a second before it registers itself.
    let script = format!("echo $$ > {}; sleep 30", h.root.join("grok-pid2").display());
    h.call(h.launch(
        "g2",
        &format!(
            "{} -c {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    let pid: u32 = wait_file(&h.root.join("grok-pid2")).trim().parse().unwrap();
    file.write_all(
        record(
            now + 3,
            serde_json::json!({"sessionUpdate": "hook_execution", "event_name": "session_start"}),
        )
        .as_bytes(),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        h.workspace().current_agent("g2").is_none(),
        "unregistered process must not be identified"
    );
    fs::write(
        home.join("active_sessions.json"),
        serde_json::json!([{"session_id": "01a1-live", "pid": pid, "cwd": "/Users/me/repo"}])
            .to_string(),
    )
    .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let w = h.workspace();
        if let Some(s) = w.current_agent("g2") {
            assert_eq!(s.cli_session_id, "01a1-live");
            assert_eq!(
                w.agent_sessions.len(),
                1,
                "resumed, not a second conversation"
            );
            break;
        }
        assert!(
            Instant::now() < until,
            "registering the resumed process must identify it"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// What `h.workspace()` says the pane runs: its CLI id and the evidence that bound it.
fn bound_in(h: &Host, pane: &str) -> Option<(String, Option<mooring_core::model::Binding>)> {
    h.workspace()
        .current_agent(pane)
        .map(|s| (s.cli_session_id.clone(), s.binding))
}
fn wait_bound(h: &Host, pane: &str, secs: u64) -> (String, Option<mooring_core::model::Binding>) {
    let until = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(bound) = bound_in(h, pane) {
            return bound;
        }
        assert!(
            Instant::now() < until,
            "{pane} never bound; hints {:?}",
            recognition(h).terminal_hints
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_hand_started_claude_binds_by_the_prompt_on_its_screen() {
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let prompt = "把左侧会话列表按最近活动排序";
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u1","cwd":"/tmp","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":prompt}});
    let seed = h.root.join("seed");
    fs::write(&seed, format!("{row}\n")).unwrap();
    // Like `claude` typed by hand: no --session-id, and the transcript is written then closed.
    let script = format!(
        "read -r line; cat {} > {}; read -r end",
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&logs.join(format!("{id}.jsonl")).to_string_lossy())
    );
    h.call(h.launch(
        "hand",
        &format!(
            "exec {} -c {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    let (mut stream, _, _) = h.attach("hand", false);
    std::thread::sleep(Duration::from_millis(300));
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: format!("{prompt}\n").into_bytes(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut stream), Reply::Ok));
    assert_eq!(
        wait_bound(&h, "hand", 15),
        (id, Some(mooring_core::model::Binding::Screen))
    );
    assert!(!recognition(&h).terminal_hints.contains_key("hand"));
}

#[test]
fn the_installed_hook_binds_a_new_claude_before_its_transcript_exists() {
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let path = logs.join(format!("{id}.jsonl"));
    let payload = h.root.join("payload.json");
    fs::write(
        &payload,
        serde_json::json!({"session_id":id,"transcript_path":path,"cwd":"/tmp","hook_event_name":"SessionStart","source":"startup"}).to_string(),
    )
    .unwrap();
    // The settings entry exactly as installed, run the way Claude runs a command hook. It
    // is detached first, so the process tree no longer leads back to the terminal and only
    // the terminal's environment can say where it runs.
    let command = mooring_host::hooks::command(Agent::Claude);
    let status = h.root.join("hook-status");
    let detached = format!(
        "( sleep 0.3; MOORING_HOOK={} /bin/sh -c {} < {}; echo $? > {} ) & exit 0",
        shell_quote(env!("CARGO_BIN_EXE_mooring")),
        shell_quote(&command),
        shell_quote(&payload.to_string_lossy()),
        shell_quote(&status.to_string_lossy()),
    );
    let script = format!("/bin/sh -c {}; read -r end", shell_quote(&detached));
    h.call(h.launch(
        "hooked",
        &format!(
            "exec {} -c {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    assert_eq!(wait_file(&status).trim(), "0");
    let (cli_id, binding) = wait_bound(&h, "hooked", 10);
    assert_eq!(cli_id, id);
    assert_eq!(binding, Some(mooring_core::model::Binding::Hook));
    let session = h.workspace().current_agent("hooked").unwrap().clone();
    assert_eq!(session.cwd, "/tmp");
    assert_eq!(
        session.transcript,
        path.canonicalize()
            .unwrap_or_else(|_| { logs.canonicalize().unwrap().join(format!("{id}.jsonl")) })
            .to_string_lossy()
    );
    // The first prompt then writes the transcript; the same session fills in, still bound.
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u1","cwd":"/tmp","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"hook 先到，对话随后写入"}});
    fs::write(&path, format!("{row}\n")).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let w = h.workspace();
        let s = w.current_agent("hooked").unwrap();
        if w.content
            .get(&s.id)
            .is_some_and(|c| c.title == "hook 先到，对话随后写入")
        {
            assert_eq!(s.id, session.id);
            assert_eq!(s.binding, Some(mooring_core::model::Binding::Hook));
            break;
        }
        assert!(
            Instant::now() < until,
            "the transcript never filled the hooked session"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // Survives a host restart through its own table.
    let stored = rusqlite_binding(&h.root, &session.id);
    assert_eq!(stored.as_deref(), Some("Hook"));
}

/// Grok reads Claude's settings and runs Mooring's hook there as `--agent claude`. A hook
/// naming a transcript under Grok's root must not bind it as a Claude conversation.
#[test]
fn a_claude_hook_naming_a_grok_transcript_is_refused() {
    let h = Host::start();
    h.setup();
    let dir = h.root.join("grok-home/sessions/%2Ftmp/01a1");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("updates.jsonl");
    fs::write(&path, "").unwrap();
    h.call(h.launch("grokked", "read -r end")).unwrap();
    let pid = h
        .workspace()
        .terminals
        .iter()
        .find(|t| t.id == "grokked")
        .and_then(|t| t.pid)
        .unwrap();
    let reply = h.call(Request::Hook {
        runtime: Agent::Claude,
        pid,
        cli_session_id: "01a1".into(),
        transcript: path.to_string_lossy().into_owned(),
        ended: false,
        event: Some("Notification".into()),
        facts: Default::default(),
        terminal: Some("grokked".into()),
        cwd: "/tmp".into(),
    });
    let err = reply.expect_err("a Grok transcript was accepted as Claude's");
    assert!(err.to_string().contains("Grok"), "{err}");
    assert!(h
        .workspace()
        .agent_sessions
        .iter()
        .all(|s| s.runtime != Agent::Claude));
}

fn rusqlite_binding(root: &std::path::Path, session: &str) -> Option<String> {
    let out = Command::new("/usr/bin/sqlite3")
        .arg("-readonly")
        .arg(root.join("workbench.sqlite"))
        .arg(format!(
            "SELECT evidence FROM agent_bindings WHERE session_id='{session}'"
        ))
        .output()
        .unwrap();
    Some(String::from_utf8(out.stdout).unwrap().trim().to_owned()).filter(|s| !s.is_empty())
}

/// Real Claude Code, no model call: its SessionStart hook fires at startup, from a settings
/// file passed on the command line so the user's own settings stay untouched. Run with
/// `MOORING_REAL_CLAUDE_CWD=<a directory Claude already trusts> cargo test -- --ignored real_claude`.
#[test]
#[ignore = "starts the real claude binary"]
fn real_claude_hooks_bind_at_startup_before_any_prompt() {
    let cwd = std::env::var("MOORING_REAL_CLAUDE_CWD").expect("MOORING_REAL_CLAUDE_CWD");
    let h = Host::start();
    h.call(Request::Edit {
        request: None,
        edit: Edit::AddSpace(Space {
            id: "s".into(),
            name: "Real".into(),
            cwd: cwd.clone(),
        }),
    })
    .unwrap();
    let settings = h.root.join("hooks.json");
    // The installed entry, with the host executable set first: this fixture's own
    // `MOORING_HOOK` is the test binary.
    let command = format!(
        "MOORING_HOOK={}; {}",
        shell_quote(env!("CARGO_BIN_EXE_mooring")),
        mooring_host::hooks::command(Agent::Claude)
    );
    let entry =
        serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "timeout": 5 }] }]);
    fs::write(
        &settings,
        serde_json::json!({ "hooks": { "SessionStart": entry, "SessionEnd": entry } }).to_string(),
    )
    .unwrap();
    let claude = Command::new("/bin/zsh")
        .args(["-lc", "whence -p claude"])
        .output()
        .unwrap();
    let claude = String::from_utf8(claude.stdout).unwrap().trim().to_owned();
    h.call(Request::Launch {
        id: "real".into(),
        space: "s".into(),
        // Whatever runs this test may be a Claude Code session itself: none of its markers pass.
        argv: ["/usr/bin/env".to_owned()]
            .into_iter()
            .chain(
                std::env::vars()
                    .map(|(k, _)| k)
                    .filter(|k| k.starts_with("CLAUDE"))
                    .flat_map(|k| ["-u".to_owned(), k]),
            )
            .chain([
                claude,
                "--settings".into(),
                settings.to_string_lossy().into_owned(),
            ])
            .collect(),
        cwd,
        cols: 120,
        rows: 40,
        scheme: Default::default(),
        colors: None,
    })
    .unwrap();
    let (cli_id, binding) = wait_bound(&h, "real", 30);
    assert_eq!(binding, Some(Binding::Hook));
    assert!(uuid::Uuid::parse_str(&cli_id).is_ok(), "{cli_id}");
    eprintln!("bound real claude {cli_id} by hook");
    // Ctrl+C twice quits without a word: `/exit` would leave a transcript behind.
    let (mut stream, _, _) = h.attach("real", false);
    for _ in 0..2 {
        send(
            &mut stream,
            CONTROL,
            &Request::Input {
                bytes: b"\x03".to_vec(),
                lossy: false,
            },
        )
        .unwrap();
        assert!(matches!(ack(&mut stream), Reply::Ok));
        std::thread::sleep(Duration::from_millis(300));
    }
    let until = Instant::now() + Duration::from_secs(20);
    while bound_in(&h, "real").is_some() {
        assert!(Instant::now() < until, "SessionEnd or exit must clear it");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// An Agent behind a wrapper whose process name says nothing: `MOORING_AGENT` names it,
/// the outermost hinted process binds, and its inheriting children do not add a second.
/// macOS hides the environment of its own platform binaries, as it would hide nothing of a
/// real wrapper, so the wrapper here is a non-system Python when the machine has one.
#[test]
fn mooring_agent_names_an_agent_hidden_behind_a_wrapper() {
    let Some(python) = ["/opt/homebrew/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
    else {
        eprintln!("skipped: no non-system python3 to stand in for a wrapper");
        return;
    };
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let wrapper = h.root.join("nono");
    std::os::unix::fs::symlink(&python, &wrapper).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let seed = h.root.join("seed");
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"wrapped"}});
    fs::write(&seed, format!("{row}\n")).unwrap();
    // The wrapper starts a child that inherits the variable, as every descendant would.
    let script = format!(
        "import shutil, subprocess, sys, time; shutil.copy({:?}, {:?}); subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)']); time.sleep(60)",
        seed.to_string_lossy(),
        logs.join(format!("{id}.jsonl")).to_string_lossy()
    );
    h.call(h.launch(
        "wrapped",
        &format!(
            "exec env MOORING_AGENT=claude {} -c {} fixture --session-id {}",
            shell_quote(&wrapper.to_string_lossy()),
            shell_quote(&script),
            id
        ),
    ))
    .unwrap();
    wait_for(&h, "the hinted wrapper never bound", |w| {
        w.current_agent("wrapped")
            .is_some_and(|s| s.cli_session_id == id && s.runtime == Agent::Claude)
    });
    let w = h.workspace();
    let bound = w
        .agent_sessions
        .iter()
        .filter(|s| s.current && s.terminal.as_deref() == Some("wrapped"))
        .count();
    assert_eq!(bound, 1, "the inheriting child counted as a second Agent");
    // An unknown name is ignored.
    h.call(h.launch(
        "unknown",
        &format!(
            "exec env MOORING_AGENT=vim {} -c 'import time; time.sleep(60)'",
            shell_quote(&python.to_string_lossy())
        ),
    ))
    .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    assert!(h.workspace().current_agent("unknown").is_none());
}
