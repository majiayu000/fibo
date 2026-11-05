use super::*;

#[test]
fn external_content_updates_without_probes_saves_full_quotes_and_reopens_on_new_turn() {
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/codex");
    fs::create_dir_all(&logs).unwrap();
    let path = logs.join("content.jsonl");
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fs::write(&path,format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"external\",\"cwd\":\"/tmp\"}}}}\n{{\"timestamp\":{at},\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\",\"turn_id\":\"one\"}}}}\n{{\"timestamp\":{at},\"type\":\"event_msg\",\"payload\":{{\"type\":\"user_message\",\"message\":\"修复输入法，不要提交\"}}}}\n")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let w = loop {
        let w = h.workspace();
        if !w.content.is_empty() {
            break w;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(50));
    };
    let s = w.agent_sessions.first().unwrap();
    assert!(s.terminal.is_none());
    assert!(!s.current);
    assert_eq!(w.content[&s.id].asks[0].text, "修复输入法，不要提交");
    h.call(Request::Edit {
        request: None,
        edit: Edit::SaveSessionTask {
            session: s.id.clone(),
            task: Task {
                id: "from-session".into(),
                space: "s".into(),
                title: "输入法".into(),
                description: "UI truncated quote is not authoritative".into(),
                acceptance: vec![],
                agent: Agent::Codex,
            },
        },
    })
    .unwrap();
    let w = h.workspace();
    assert_eq!(w.tasks[0].description, "修复输入法，不要提交");
    assert_eq!(w.agent_sessions[0].task.as_deref(), Some("from-session"));
    h.call(Request::Edit {
        request: None,
        edit: Edit::SetOutcome {
            seen: None,
            id: s.id.clone(),
            outcome: Outcome::Completed,
        },
    })
    .unwrap();
    use std::io::Write;
    writeln!(fs::OpenOptions::new().append(true).open(&path).unwrap(),"{{\"timestamp\":{},\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\",\"turn_id\":\"two\"}}}}",at+1).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if h.workspace().agent_sessions[0].outcome == Outcome::Open {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(50));
    }
    let w = h.workspace();
    assert!(w
        .outcome_history
        .iter()
        .any(|e| e.reason == "新回合" && e.before == Outcome::Completed));
    let due = mooring_core::content::now() + 2;
    h.call(Request::Edit {
        request: None,
        edit: Edit::SetOutcome {
            seen: None,
            id: s.id.clone(),
            outcome: Outcome::Deferred {
                reason: "deadline test".into(),
                until: due,
            },
        },
    })
    .unwrap();
    // Subscribe once: the host deadline must publish without a refresh/poll driving it.
    let mut stream = UnixStream::connect(h.socket()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    let _: Reply = receive(&mut stream).unwrap();
    send(&mut stream, CONTROL, &Request::WatchRecognition).unwrap();
    loop {
        let _: Reply = receive(&mut stream).unwrap();
        if mooring_core::content::now() >= due {
            break;
        }
    }
    let w = h.workspace();
    assert_eq!(w.agent_sessions[0].outcome, Outcome::Open);
    assert!(w
        .outcome_history
        .iter()
        .any(|e| e.reason == "暂存到期" && e.at == due));
    let stats = recognition(&h);
    assert_eq!((stats.metrics.ps_spawns, stats.metrics.lsof_spawns), (0, 0));
}

#[test]
fn claude_open_files_bind_same_cwd_without_hooks_and_append_without_probes() {
    use std::io::Write;
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    // macOS hides a platform binary's environment, so /bin/sh would always look clean;
    // a re-signed copy reads like the real CLI.
    let cli = h.root.join("claude");
    fs::copy("/bin/bash", &cli).unwrap();
    assert!(std::process::Command::new("/usr/bin/codesign")
        .args(["-f", "-s", "-"])
        .arg(&cli)
        .output()
        .unwrap()
        .status
        .success());
    for i in 0..2 {
        let path = logs.join(format!("claude-{i}.jsonl"));
        let seed = h.root.join(format!("seed-{i}"));
        let row = serde_json::json!({"type":"user","sessionId":format!("claude-{i}"),"uuid":format!("u-{i}"),"cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":format!("修复第 {i} 个项目，不调用模型")}});
        fs::write(&seed, format!("{row}\n")).unwrap();
        let script = format!(
            "exec 3>{}; cat {} >&3; read -r line",
            shell_quote(&path.to_string_lossy()),
            shell_quote(&seed.to_string_lossy())
        );
        h.call(h.launch(
            &format!("claude-pty-{i}"),
            &format!(
                "exec {} -c {}",
                shell_quote(&cli.to_string_lossy()),
                shell_quote(&script)
            ),
        ))
        .unwrap();
    }
    // Wall-clock delivery may wait behind concurrent fixture startup and native
    // FSEvents dispatch. Keep a finite 30s bound; do not relax identity/content checks.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let w = h.workspace();
        if (0..2).all(|i| {
            w.current_agent(&format!("claude-pty-{i}"))
                .is_some_and(|s| s.cli_session_id == format!("claude-{i}"))
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Claude automatic open-file binding timed out"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // Drain setup events before comparing append-only counters.
    std::thread::sleep(Duration::from_millis(500));
    let before = recognition(&h).metrics;
    let path = logs.join("claude-0.jsonl");
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    for row in [
        serde_json::json!({"type":"assistant","sessionId":"claude-0","message":{"content":[{"type":"tool_use","id":"edit","name":"Edit","input":{"file_path":"src/main.rs"}}]}}),
        serde_json::json!({"type":"user","sessionId":"claude-0","message":{"content":[{"type":"tool_result","tool_use_id":"edit","content":"Successfully updated"}]}}),
        serde_json::json!({"type":"assistant","sessionId":"claude-0","message":{"content":[{"type":"tool_use","id":"ask","name":"AskUserQuestion","input":{"questions":[{"question":"选择验证方式？","options":[{"label":"本地测试"},{"label":"人工复核"}]}]}}]}}),
    ] {
        writeln!(file, "{row}").unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let w = h.workspace();
        let agent = w.current_agent("claude-pty-0").unwrap();
        if w.content
            .get(&agent.id)
            .is_some_and(|c| c.turn == mooring_core::content::Turn::Waiting)
        {
            let c = &w.content[&agent.id];
            assert!(c.interactive);
            assert_eq!(c.asks[0].text, "修复第 0 个项目，不调用模型");
            assert!(c.here().done.contains("src/main.rs"));
            assert!(c.here().stopped.contains("选择验证方式"));
            assert!(c.here().stopped.contains("本地测试"));
            let other = w.current_agent("claude-pty-1").unwrap();
            assert!(!w.content[&other.id].here().done.contains("src/main.rs"));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Claude append timed out: recognition={:?}, content={:?}, file_bytes={}",
            recognition(&h),
            w.content.get(&agent.id),
            fs::metadata(&path).unwrap().len()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let after = recognition(&h).metrics;
    assert_eq!(
        (before.ps_spawns, before.lsof_spawns),
        (after.ps_spawns, after.lsof_spawns)
    );
}

#[test]
fn claude_explicit_id_binds_closed_log_and_tracks_exit_without_hooks() {
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let ids: Vec<_> = (0..2).map(|_| uuid::Uuid::new_v4().to_string()).collect();
    for (i, id) in ids.iter().enumerate() {
        let path = logs.join(format!("{id}.jsonl"));
        let seed = h.root.join(format!("seed-{i}"));
        let row = serde_json::json!({"type":"user","sessionId":id,"uuid":format!("u{i}"),"cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"Explicit identity test"}});
        fs::write(&seed, format!("{row}\n")).unwrap();
        // Close the log before FSEvents delivery: lsof cannot supply this association.
        let script = format!(
            "cat {} > {}; read -r line",
            shell_quote(&seed.to_string_lossy()),
            shell_quote(&path.to_string_lossy())
        );
        h.call(h.launch(
            &format!("id-{i}"),
            &format!(
                "exec {} -c {} fixture --session-id {}",
                shell_quote(&cli.to_string_lossy()),
                shell_quote(&script),
                id
            ),
        ))
        .unwrap();
    }
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let w = h.workspace();
        if ids.iter().enumerate().all(|(i, id)| {
            w.current_agent(&format!("id-{i}"))
                .is_some_and(|s| &s.cli_session_id == id)
        }) {
            break;
        }
        assert!(
            Instant::now() < until,
            "closed logs must bind via explicit argv IDs"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(recognition(&h).metrics.lsof_spawns, 0);
    h.call(Request::Stop { id: "id-0".into() }).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while h.workspace().current_agent("id-0").is_some() {
        assert!(
            Instant::now() < until,
            "exited Claude cannot remain current"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(h.workspace().current_agent("id-1").is_some());
}

/// A Claude run straight from the shell names no conversation on its command line and
/// keeps its log closed; `claude agents --json` lists it by pid once it has logged.
#[test]
fn a_plain_claude_is_named_by_its_pid_from_the_official_session_list() {
    use std::os::unix::fs::PermissionsExt;
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let path = logs.join(format!("{id}.jsonl"));
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let seed = h.root.join("seed");
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u1","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"Plain claude, no id on its command line"}});
    fs::write(&seed, format!("{row}\n")).unwrap();
    // Claude Code lists the live session with its pid, as `claude agents --json` does.
    let pid_file = h.root.join("claude-pid");
    let agents = h.root.join("claude-agents");
    fs::write(
        &agents,
        format!(
            "#!/bin/sh\nprintf '[{{\"pid\":%s,\"kind\":\"interactive\",\"sessionId\":\"{id}\",\"status\":\"idle\"}}]' \"$(cat {})\"\n",
            shell_quote(&pid_file.to_string_lossy())
        ),
    )
    .unwrap();
    fs::set_permissions(&agents, fs::Permissions::from_mode(0o755)).unwrap();
    // Closed log, no id in argv, prompt not on screen: only the pid can name it.
    let script = format!(
        "echo $$ > {}; cat {} > {}; read -r line",
        shell_quote(&pid_file.to_string_lossy()),
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&path.to_string_lossy())
    );
    h.call(h.launch(
        "plain",
        &format!(
            "exec {} -c {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while h
        .workspace()
        .current_agent("plain")
        .is_none_or(|s| s.cli_session_id != id)
    {
        assert!(Instant::now() < until, "plain claude never named");
        std::thread::sleep(Duration::from_millis(50));
    }
    let named = h.workspace();
    assert_eq!(
        named.current_agent("plain").and_then(|s| s.binding),
        Some(Binding::Process)
    );
    assert!(!recognition(&h).terminal_hints.contains_key("plain"));
}
#[test]
fn manual_claude_closed_log_stays_external_and_append_does_not_repeat_probes() {
    use std::io::Write;
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let path = logs.join("manual.jsonl");
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let seed = h.root.join("seed");
    let id = uuid::Uuid::new_v4().to_string();
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u1","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"Manual identity needs open-file proof"}});
    fs::write(&seed, format!("{row}\n")).unwrap();
    let script = format!(
        "cat {} > {}; read -r line",
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&path.to_string_lossy())
    );
    h.call(h.launch(
        "manual",
        &format!(
            "exec {} -c {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if h.workspace()
            .agent_sessions
            .iter()
            .any(|s| s.cli_session_id == id)
            && recognition(&h).metrics.lsof_spawns > 0
        {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(500));
    let before = recognition(&h).metrics;
    for i in 0..5 {
        let row = serde_json::json!({"type":"assistant","sessionId":id,"uuid":format!("a{i}"),"message":{"content":[{"type":"text","text":format!("manual append {i}")}]}});
        // Match Claude's brief-open writes; an unbound file has no vnode watch.
        {
            let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(file, "{row}").unwrap();
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let after = recognition(&h).metrics;
        if after.log_bytes > before.log_bytes {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(50));
    }
    let after = recognition(&h).metrics;
    assert_eq!(
        (after.ps_spawns, after.lsof_spawns),
        (before.ps_spawns, before.lsof_spawns)
    );
    let w = h.workspace();
    assert!(w.current_agent("manual").is_none());
    assert!(w
        .agent_sessions
        .iter()
        .find(|s| s.cli_session_id == id)
        .unwrap()
        .terminal
        .is_none());
}

#[test]
fn claude_without_a_transcript_says_so_until_it_exits() {
    let h = Host::start();
    h.setup();
    // macOS hides a platform binary's environment, so /bin/sh would always look clean;
    // a re-signed copy reads like the real CLI.
    let cli = h.root.join("claude");
    fs::copy("/bin/bash", &cli).unwrap();
    assert!(std::process::Command::new("/usr/bin/codesign")
        .args(["-f", "-s", "-"])
        .arg(&cli)
        .output()
        .unwrap()
        .status
        .success());
    let cli = shell_quote(&cli.to_string_lossy());
    // Two lines: the first is the Enter that prompts a scan, the second ends the CLI.
    let script = shell_quote("read -r a; read -r b");
    h.call(h.launch(
        "nested",
        &format!("exec /usr/bin/env CLAUDE_CODE_CHILD_SESSION=1 {cli} -c {script}"),
    ))
    .unwrap();
    // Not left to the host: whatever runs the tests may itself carry the marker.
    h.call(h.launch(
        "plain",
        &format!("exec /usr/bin/env -u CLAUDE_CODE_CHILD_SESSION {cli} -c {script}"),
    ))
    .unwrap();
    let mut streams = vec![];
    for pane in ["nested", "plain"] {
        let (mut stream, _, _) = h.attach(pane, false);
        send(
            &mut stream,
            CONTROL,
            &Request::Input {
                bytes: b"\n".to_vec(),
                lossy: false,
            },
        )
        .unwrap();
        assert!(matches!(ack(&mut stream), Reply::Ok));
        streams.push(stream);
    }
    let until = Instant::now() + Duration::from_secs(10);
    while recognition(&h)
        .terminal_hints
        .get("nested")
        .map(String::as_str)
        != Some(mooring_core::protocol::CLAUDE_TRANSCRIPT_OFF)
    {
        assert!(
            Instant::now() < until,
            "{:?}",
            recognition(&h).terminal_hints
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    // A plain Claude saves its transcript; until a prompt ties it to one it is only unbound.
    assert_eq!(
        recognition(&h)
            .terminal_hints
            .get("plain")
            .map(String::as_str),
        Some(format!("{}claude", mooring_core::protocol::AGENT_UNBOUND_PREFIX).as_str())
    );
    send(
        &mut streams[0],
        CONTROL,
        &Request::Input {
            bytes: b"\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut streams[0]), Reply::Ok));
    let until = Instant::now() + Duration::from_secs(10);
    while recognition(&h).terminal_hints.contains_key("nested") {
        assert!(Instant::now() < until, "the hint outlives the CLI");
        std::thread::sleep(Duration::from_millis(25));
    }
}
