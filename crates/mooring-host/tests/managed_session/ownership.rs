use super::*;

#[test]
fn two_cli_conversations_in_one_real_pty_keep_separate_ownership_after_restart() {
    let mut h = Host::start();
    h.setup();
    let codex = h.root.join("codex");
    let claude = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &codex).unwrap();
    std::os::unix::fs::symlink("/bin/sh", &claude).unwrap();
    let first = h.root.join("first.jsonl");
    let second = h.root.join("second.jsonl");
    fs::write(
        &first,
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-one\",\"cwd\":\"/tmp\"}}\n",
    )
    .unwrap();
    fs::write(&second,"{\"type\":\"user\",\"sessionId\":\"claude-two\",\"cwd\":\"/tmp\",\"message\":{\"content\":\"test\"}}\n").unwrap();
    for id in ["first-task", "second-task"] {
        h.call(Request::Edit {
            request: None,
            edit: Edit::AddTask(Task {
                id: id.into(),
                space: "s".into(),
                title: id.into(),
                description: "test".into(),
                acceptance: vec![AcceptanceCriterion {
                    id: "criterion-1".into(),
                    description: "关联正确".into(),
                    evidence: vec![],
                }],
                agent: Agent::Codex,
            }),
        })
        .unwrap();
    }
    fs::write(
        h.root.join("hook.json"),
        serde_json::to_vec(&serde_json::json!({"session_id":"codex-one","transcript_path":first}))
            .unwrap(),
    )
    .unwrap();
    let first_script = format!(
        "echo $$ > {root}/pid1; while [ ! -f {root}/go ]; do sleep 0.05; done; {helper} hook --runtime {root} --agent codex < {root}/hook.json; echo $? > {root}/hook-status; sleep 30",
        root = h.root.display(),
        helper = env!("CARGO_BIN_EXE_mooring")
    );
    let command = format!(
        "{} -c {}; {} -c {}",
        shell_quote(&codex.to_string_lossy()),
        shell_quote(&first_script),
        shell_quote(&claude.to_string_lossy()),
        shell_quote(&format!("echo $$ > {}/pid2; sleep 30", h.root.display()))
    );
    h.call(h.launch("one", &command)).unwrap();
    h.call(Request::Edit {
        request: None,
        edit: Edit::CreateTab {
            tab: Tab {
                id: "t".into(),
                space: "s".into(),
                default_task: Some("first-task".into()),
                title: "test".into(),
                layout: Some(Layout::Leaf("one".into())),
            },
        },
    })
    .unwrap();
    fs::write(h.root.join("go"), "start").unwrap();
    assert_eq!(
        wait_file(&h.root.join("hook-status")).trim(),
        "0",
        "real hook helper failed"
    );
    let pid1: u32 = wait_file(&h.root.join("pid1")).trim().parse().unwrap();
    let hook = || Request::Hook {
        runtime: Agent::Codex,
        pid: pid1,
        cli_session_id: "codex-one".into(),
        transcript: first.to_string_lossy().into_owned(),
        ended: false,
        event: None,
        facts: Default::default(),
        terminal: None,
        cwd: String::new(),
    };
    h.call(hook()).unwrap();
    h.call(hook()).unwrap();
    let first_id = h.workspace().agent_sessions[0].id.clone();
    h.call(Request::Edit {
        request: None,
        edit: Edit::SetOutcome {
            seen: None,
            id: first_id.clone(),
            outcome: Outcome::Completed,
        },
    })
    .unwrap();
    assert_eq!(unsafe { libc::kill(pid1 as i32, libc::SIGTERM) }, 0);
    let pid2: u32 = wait_file(&h.root.join("pid2")).trim().parse().unwrap();
    h.call(Request::Hook {
        runtime: Agent::Claude,
        pid: pid2,
        cli_session_id: "wrong".into(),
        transcript: second.to_string_lossy().into_owned(),
        ended: false,
        event: None,
        facts: Default::default(),
        terminal: None,
        cwd: String::new(),
    })
    .unwrap_err();
    h.call(Request::Hook {
        runtime: Agent::Claude,
        pid: pid2,
        cli_session_id: "claude-two".into(),
        transcript: second.to_string_lossy().into_owned(),
        ended: false,
        event: None,
        facts: Default::default(),
        terminal: None,
        cwd: String::new(),
    })
    .unwrap();
    let second_id = h
        .workspace()
        .agent_sessions
        .iter()
        .find(|s| s.runtime == Agent::Claude)
        .unwrap()
        .id
        .clone();
    h.call(Request::Edit {
        request: None,
        edit: Edit::AssignTask {
            id: second_id,
            task: Some("second-task".into()),
        },
    })
    .unwrap();
    let w = h.workspace();
    assert_eq!(w.terminals.len(), 1);
    assert_eq!(w.agent_sessions.len(), 2);
    assert_eq!(w.current_agent("one").unwrap().cli_session_id, "claude-two");
    assert_eq!(
        w.agent_sessions
            .iter()
            .find(|s| s.id == first_id)
            .unwrap()
            .outcome,
        Outcome::Completed
    );
    let db = rusqlite::Connection::open(h.root.join("workbench.sqlite")).unwrap();
    let names: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for name in [
        "spaces",
        "tasks",
        "tabs",
        "terminals",
        "agent_sessions",
        "task_criteria",
    ] {
        assert!(names.iter().any(|n| n == name));
    }
    assert!(!names.iter().any(|n| n == "workspace"));
    let task: String = db
        .query_row(
            "SELECT task_id FROM agent_sessions WHERE cli_session_id='claude-two'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(task, "second-task");
    let evidence: Option<String> = db
        .query_row(
            "SELECT evidence FROM task_criteria WHERE task_id='first-task'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(evidence.is_none());
    drop(db);
    h.call(Request::Stop { id: "one".into() }).unwrap();
    h.child.kill().unwrap();
    h.child.wait().unwrap();
    h.child = fixture_command(&h.root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn_tied()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if h.call(Request::Workspace).is_ok() {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(25));
    }
    let restored = h.workspace();
    assert_eq!(restored.agent_sessions.len(), 2);
    assert!(restored.agent_sessions.iter().all(|s| !s.current));
    assert_eq!(
        restored
            .agent_sessions
            .iter()
            .find(|s| s.id == first_id)
            .unwrap()
            .outcome,
        Outcome::Completed
    );
    assert_eq!(restored.tab_terminals("t"), ["one"]);
}

#[test]
fn stop_agent_clears_current_without_stopping_shell_or_changing_outcome() {
    let h = Host::start();
    h.setup();
    let cli = h.root.join("codex");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let log = h.root.join("exit.jsonl");
    fs::write(
        &log,
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"exit-proof\",\"cwd\":\"/tmp\"}}\n",
    )
    .unwrap();
    let script = format!(
        "echo $$ > {}; read -r line",
        h.root.join("agent-pid").display()
    );
    h.call(h.launch(
        "shell",
        &format!(
            "{} -c {}; read -r line",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    let pid: u32 = wait_file(&h.root.join("agent-pid")).trim().parse().unwrap();
    h.call(Request::Hook {
        runtime: Agent::Codex,
        pid,
        cli_session_id: "exit-proof".into(),
        transcript: log.to_string_lossy().into_owned(),
        ended: false,
        event: None,
        facts: Default::default(),
        terminal: None,
        cwd: String::new(),
    })
    .unwrap();
    let before = h.workspace().current_agent("shell").unwrap().clone();
    h.call(Request::Edit {
        request: None,
        edit: Edit::SetOutcome {
            seen: None,
            id: before.id.clone(),
            outcome: Outcome::Completed,
        },
    })
    .unwrap();
    h.call(Request::StopAgent { id: "shell".into() }).unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        let w = h.workspace();
        if w.current_agent("shell").is_none() {
            assert_eq!(w.terminals[0].state, ProcessState::Running);
            let session = &w.agent_sessions[0];
            assert_eq!(session.id, before.id);
            assert_eq!(session.task, before.task);
            assert_eq!(session.outcome, Outcome::Completed);
            let db = rusqlite::Connection::open(h.root.join("workbench.sqlite")).unwrap();
            let current: bool = db
                .query_row(
                    "SELECT is_current FROM agent_sessions WHERE id=?1",
                    [&before.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(!current);
            break;
        }
        assert!(Instant::now() < until, "Agent exit event was not persisted");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn manually_entered_unbound_codex_gets_a_hint_and_exit_clears_it() {
    let h = Host::start();
    h.setup();
    let cli = h.root.join("codex");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    h.call(h.launch(
        "shell",
        "stty -echo; while IFS= read -r line; do eval \"$line\"; done",
    ))
    .unwrap();
    let (mut stream, _, _) = h.attach("shell", false);
    let script = format!(
        "echo $$ > {}; read -r line",
        h.root.join("hint-pid").display()
    );
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: format!(
                "{} -c {}\n",
                shell_quote(&cli.to_string_lossy()),
                shell_quote(&script)
            )
            .into_bytes(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut stream), Reply::Ok));
    let pid: u32 = wait_file(&h.root.join("hint-pid")).trim().parse().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if recognition(&h)
            .terminal_hints
            .get("shell")
            .is_some_and(|s| s == "未关联：发下一句后自动关联对话")
        {
            break;
        }
        assert!(Instant::now() < until, "missing unbound Codex hint");
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(h.workspace().agent_sessions.is_empty());
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    let until = Instant::now() + Duration::from_secs(3);
    while !recognition(&h).terminal_hints.is_empty() {
        assert!(Instant::now() < until, "stale hint after process exit");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn stop_agent_refuses_an_unidentified_shell() {
    let h = Host::start();
    h.setup();
    h.call(h.launch("plain", "cat")).unwrap();
    assert!(h.call(Request::StopAgent { id: "plain".into() }).is_err());
    let terminal = h.workspace().terminals[0].clone();
    assert_eq!(terminal.state, ProcessState::Running);
    assert_eq!(unsafe { libc::kill(terminal.pid.unwrap() as i32, 0) }, 0);
}

/// plan.md stage 1 acceptance: while the workspace keeps moving (here every few ms, as an
/// Agent's output moves it), marking a session done and open again 100 times, each from
/// the copy the window had a moment before, is never refused.
#[test]
fn marking_done_while_the_workspace_moves_is_never_refused() {
    let h = Host::start();
    h.setup();
    let codex = h.root.join("codex");
    std::os::unix::fs::symlink("/bin/sh", &codex).unwrap();
    let log = h.root.join("log.jsonl");
    fs::write(
        &log,
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-busy\",\"cwd\":\"/tmp\"}}\n",
    )
    .unwrap();
    let script = format!("echo $$ > {}/pid; sleep 30", h.root.display());
    h.call(h.launch(
        "one",
        &format!(
            "{} -c {}",
            shell_quote(&codex.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    let pid: u32 = wait_file(&h.root.join("pid")).trim().parse().unwrap();
    h.call(Request::Hook {
        runtime: Agent::Codex,
        pid,
        cli_session_id: "codex-busy".into(),
        transcript: log.to_string_lossy().into_owned(),
        ended: false,
        event: None,
        facts: Default::default(),
        terminal: None,
        cwd: String::new(),
    })
    .unwrap();
    let id = h.workspace().agent_sessions[0].id.clone();
    let start = h.workspace().revision;
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut refused = Vec::new();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut at = 1;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                h.call(Request::ClientLeft { at }).unwrap();
                at += 1;
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        for i in 0..100 {
            let seen = h.workspace();
            std::thread::sleep(Duration::from_millis(5));
            let mut edit = Edit::SetOutcome {
                id: id.clone(),
                outcome: if i % 2 == 0 {
                    Outcome::Completed
                } else {
                    Outcome::Open
                },
                seen: None,
            };
            seen.stamp(&mut edit);
            if let Err(e) = h.call(Request::Edit {
                request: None,
                edit,
            }) {
                refused.push(e.to_string());
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let moved = h.workspace().revision - start;
    assert!(moved > 200, "the workspace barely moved: {moved}");
    assert!(
        refused.is_empty(),
        "{} refused: {:?}",
        refused.len(),
        refused
    );
    assert_eq!(h.workspace().agent_sessions[0].outcome, Outcome::Open);
}
