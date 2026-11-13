use super::*;
use fibo_core::model::{Space, Terminal};
#[test]
fn codex_wrapper_and_native_child_with_one_resume_id_bind_once() {
    let mut w = Workspace::default();
    w.terminals.push(Terminal {
        id: "pty".into(),
        space: "s".into(),
        argv: vec!["sh".into()],
        cwd: "/same".into(),
        pid: Some(100),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    let id = uuid::Uuid::new_v4().to_string();
    let process = |pid, parent| Process {
        pid,
        parent,
        runtime: Some(Agent::Codex),
        cli_session_id: Some(id.clone()),
        no_daemon: false,
        launch: vec![],
    };
    let shell = Process {
        pid: 100,
        parent: 1,
        runtime: None,
        cli_session_id: None,
        no_daemon: false,
        launch: vec![],
    };
    let processes = vec![shell, process(101, 100), process(102, 101)];
    let transcripts = vec![Transcript {
        runtime: Agent::Codex,
        cli_id: id.clone(),
        path: "/logs/x.jsonl".into(),
        cwd: "/same".into(),
        updated: 1,
    }];
    reconcile(&mut w, &transcripts, &processes, &HashMap::new()).unwrap();
    assert_eq!(w.current_agent("pty").unwrap().cli_session_id, id);
}

#[test]
fn a_current_conversation_keeps_the_flags_its_agent_was_started_with() {
    let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    let id = uuid::Uuid::new_v4().to_string();
    let codex = crate::agents::adapter(Agent::Codex);
    // Wrapper or native child: the same flags, without cwd, subcommand, id or prompt,
    // and never `--no-daemon`: a resume uses whatever server the user's codex picks.
    let flags = argv(&["--yolo", "-m", "gpt-5", "-c", "a=1"]);
    for start in [&["node", "/opt/homebrew/bin/codex"][..], &["/v/bin/codex"]] {
        let mut args = argv(start);
        args.extend(argv(&[
            "--yolo",
            "--no-daemon",
            "-m",
            "gpt-5",
            "-C",
            "/x",
            "-c",
        ]));
        args.extend(argv(&["a=1", "resume", &id, "继续"]));
        assert_eq!(codex.launch_flags(&args), flags);
    }
    // The TUI's app-server child, as seen on this machine: its `-c` are not the user's.
    assert!(codex
        .launch_flags(&argv(&[
            "/v/bin/codex",
            "app-server",
            "-c",
            "features.shell_tool=false",
        ]))
        .is_empty());
    // The shape of a real pane's Codex, read from `ps` on this machine.
    assert_eq!(
        codex.launch_flags(&argv(&[
            "/opt/homebrew/lib/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex",
            "--dangerously-bypass-approvals-and-sandbox",
            "resume",
            &id,
        ])),
        argv(&["--dangerously-bypass-approvals-and-sandbox"])
    );
    // Claude lists run to the next flag; `=value` stays one argument.
    let claude = argv(&[
        "claude",
        "--dangerously-skip-permissions",
        "--add-dir",
        "a",
        "b",
        "--model=opus",
        "--session-id",
        "u",
        "hello",
    ]);
    assert_eq!(
        crate::agents::adapter(Agent::Claude).launch_flags(&claude),
        argv(&[
            "--dangerously-skip-permissions",
            "--add-dir",
            "a",
            "b",
            "--model=opus"
        ])
    );

    let mut w = Workspace::default();
    w.terminals.push(Terminal {
        id: "pty".into(),
        space: "s".into(),
        argv: vec!["sh".into()],
        cwd: "/same".into(),
        pid: Some(100),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    let process = |pid, parent, launch: &Vec<String>| Process {
        pid,
        parent,
        runtime: Some(Agent::Codex),
        cli_session_id: Some(id.clone()),
        no_daemon: false,
        launch: launch.clone(),
    };
    let shell = Process {
        pid: 100,
        parent: 1,
        runtime: None,
        cli_session_id: None,
        no_daemon: false,
        launch: vec![],
    };
    let transcripts = vec![Transcript {
        runtime: Agent::Codex,
        cli_id: id.clone(),
        path: "/logs/x.jsonl".into(),
        cwd: "/same".into(),
        updated: 1,
    }];
    let processes = vec![
        shell.clone(),
        process(101, 100, &flags),
        process(102, 101, &flags),
    ];
    reconcile(&mut w, &transcripts, &processes, &HashMap::new()).unwrap();
    record_launch(&mut w, &processes);
    let session = w.current_agent("pty").unwrap().clone();
    assert_eq!(w.agent_launch.get(&session.id), Some(&flags));
    assert_eq!(
        w.resume_command(&session),
        format!(
            "{} --yolo -m gpt-5 -c a=1 resume '{id}'",
            fibo_platform::shell::past_functions("codex")
        )
    );
    // Started again without flags: nothing left to repeat.
    record_launch(&mut w, &[shell, process(103, 100, &vec![])]);
    assert!(!w.agent_launch.contains_key(&session.id));
    assert_eq!(w.resume_command(&session), format!("codex resume '{id}'"));
}

#[test]
fn explicit_ids_disambiguate_same_cwd_without_open_files() {
    let mut w = Workspace::default();
    let mut processes = vec![];
    let mut transcripts = vec![];
    for i in 0..3 {
        let id = uuid::Uuid::new_v4().to_string();
        w.terminals.push(Terminal {
            id: format!("pty-{i}"),
            space: "s".into(),
            argv: vec!["claude".into()],
            cwd: "/same".into(),
            pid: Some(100 + i),
            state: ProcessState::Running,
            exit_code: None,
            error: None,
        });
        processes.push(Process {
            pid: 100 + i,
            parent: 1,
            runtime: Some(Agent::Claude),
            cli_session_id: (i < 2).then(|| id.clone()),
            no_daemon: false,
            launch: vec![],
        });
        transcripts.push(Transcript {
            runtime: Agent::Claude,
            cli_id: id,
            path: format!("/same/{i}.jsonl"),
            cwd: "/same".into(),
            updated: 1,
        });
    }
    reconcile(&mut w, &transcripts, &processes, &HashMap::new()).unwrap();
    for (i, transcript) in transcripts.iter().take(2).enumerate() {
        assert_eq!(
            w.current_agent(&format!("pty-{i}")).unwrap().cli_session_id,
            transcript.cli_id
        );
    }
    assert!(w.current_agent("pty-2").is_none());
    // A managed process with a mismatched ID cannot claim another transcript even if it opens it.
    let mut mismatch = w.clone();
    mismatch.agent_sessions.clear();
    processes[0].cli_session_id = Some(uuid::Uuid::new_v4().to_string());
    reconcile(
        &mut mismatch,
        &transcripts,
        &processes,
        &HashMap::from([(100, vec![transcripts[0].path.clone()])]),
    )
    .unwrap();
    assert!(mismatch.current_agent("pty-0").is_none());
}

#[test]
fn a_hook_switch_after_clear_outlives_the_launch_id_on_argv() {
    let mut w = Workspace::default();
    let [a, b] = [0, 1].map(|_| uuid::Uuid::new_v4().to_string());
    w.terminals.push(Terminal {
        id: "pty-0".into(),
        space: "s".into(),
        argv: vec!["claude".into()],
        cwd: "/same".into(),
        pid: Some(100),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    // Reopened by `claude --resume <a>`: argv names `a` for the life of the process.
    let processes = vec![Process {
        pid: 100,
        parent: 1,
        runtime: Some(Agent::Claude),
        cli_session_id: Some(a.clone()),
        no_daemon: false,
        launch: vec![],
    }];
    let log = |id: &str, updated| Transcript {
        runtime: Agent::Claude,
        cli_id: id.into(),
        path: format!("/same/{id}.jsonl"),
        cwd: "/same".into(),
        updated,
    };
    let (ta, tb) = (log(&a, 1), log(&b, 2));
    let both = [ta.clone(), tb.clone()];
    let current = |w: &Workspace| w.current_agent("pty-0").map(|s| s.cli_session_id.clone());
    reconcile(&mut w, &both, &processes, &HashMap::new()).unwrap();
    assert_eq!(current(&w), Some(a.clone()));
    // `/clear`: SessionEnd for `a`; a scan before SessionStart must not revive it.
    apply_hook(
        &mut w,
        Agent::Claude,
        100,
        Some("pty-0"),
        &a,
        &ta,
        &processes,
        true,
    )
    .unwrap();
    reconcile(&mut w, &both, &processes, &HashMap::new()).unwrap();
    assert_eq!(current(&w), None);
    apply_hook(
        &mut w,
        Agent::Claude,
        100,
        Some("pty-0"),
        &b,
        &tb,
        &processes,
        false,
    )
    .unwrap();
    for _ in 0..2 {
        reconcile(&mut w, &both, &processes, &HashMap::new()).unwrap();
        assert_eq!(current(&w), Some(b.clone()));
    }
    assert_eq!(
        w.current_agent("pty-0").unwrap().binding,
        Some(Binding::Hook)
    );
    // `/resume <a>` from inside: the hook moves the terminal back.
    apply_hook(
        &mut w,
        Agent::Claude,
        100,
        Some("pty-0"),
        &a,
        &ta,
        &processes,
        false,
    )
    .unwrap();
    reconcile(&mut w, &both, &processes, &HashMap::new()).unwrap();
    assert_eq!(current(&w), Some(a));
}

#[test]
fn a_claude_run_from_the_terminals_claude_keeps_the_binding() {
    let mut w = Workspace::default();
    let [outer, inner] = [0, 1].map(|_| uuid::Uuid::new_v4().to_string());
    w.terminals.push(Terminal {
        id: "pty-0".into(),
        space: "s".into(),
        argv: vec!["zsh".into()],
        cwd: "/same".into(),
        pid: Some(100),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    let process = |pid, parent, runtime| Process {
        pid,
        parent,
        runtime,
        cli_session_id: None,
        no_daemon: false,
        launch: vec![],
    };
    // zsh → claude (node wrapper → native) → its shell tool → claude -p
    let mut processes = vec![
        process(100, 1, None),
        process(101, 100, Some(Agent::Claude)),
        process(102, 101, Some(Agent::Claude)),
    ];
    let log = |id: &str| Transcript {
        runtime: Agent::Claude,
        cli_id: id.into(),
        path: format!("/same/{id}.jsonl"),
        cwd: "/same".into(),
        updated: 1,
    };
    let current = |w: &Workspace| w.current_agent("pty-0").map(|s| s.cli_session_id.clone());
    let hook = |w: &mut Workspace, pid, id: &str, processes: &[Process], ended| {
        apply_hook(
            w,
            Agent::Claude,
            pid,
            Some("pty-0"),
            id,
            &log(id),
            processes,
            ended,
        )
    };
    // The wrapper and its native child are one Agent, not a nesting.
    hook(&mut w, 1, &outer, &processes, false).unwrap();
    assert_eq!(current(&w), Some(outer.clone()));
    processes.push(process(103, 102, None));
    processes.push(process(104, 103, Some(Agent::Claude)));
    // Traced to the inner CLI, or only named by the environment after its caller left.
    for pid in [104, 1] {
        assert!(hook(&mut w, pid, &inner, &processes, false).is_err());
        assert!(hook(&mut w, pid, &inner, &processes, true).is_err());
        assert_eq!(current(&w), Some(outer.clone()));
    }
    // The terminal's own Agent still reports, and `/clear` there still moves it.
    hook(&mut w, 1, &outer, &processes, true).unwrap();
    assert_eq!(current(&w), None);
    hook(&mut w, 1, &inner, &processes, false).unwrap();
    assert_eq!(current(&w), Some(inner));
}

#[test]
fn log_identity_alone_does_not_bind_by_directory_and_ambiguity_stays_unbound() {
    let mut workspace = Workspace::default();
    workspace.spaces.push(Space {
        id: "s".into(),
        name: "s".into(),
        cwd: "/tmp".into(),
    });
    workspace.terminals.push(Terminal {
        id: "pty".into(),
        space: "s".into(),
        argv: vec!["sh".into()],
        cwd: "/tmp".into(),
        pid: Some(100),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    let transcripts = vec![
        Transcript {
            runtime: Agent::Codex,
            cli_id: "one".into(),
            path: "/logs/one.jsonl".into(),
            cwd: "/tmp".into(),
            updated: 1,
        },
        Transcript {
            runtime: Agent::Codex,
            cli_id: "two".into(),
            path: "/logs/two.jsonl".into(),
            cwd: "/tmp".into(),
            updated: 2,
        },
    ];
    let processes = vec![
        Process {
            pid: 100,
            parent: 1,
            runtime: None,
            cli_session_id: None,
            no_daemon: false,
            launch: vec![],
        },
        Process {
            pid: 101,
            parent: 100,
            runtime: Some(Agent::Codex),
            cli_session_id: None,
            no_daemon: false,
            launch: vec![],
        },
    ];
    reconcile(&mut workspace, &transcripts, &processes, &HashMap::new()).unwrap();
    assert!(workspace
        .agent_sessions
        .iter()
        .all(|s| s.terminal.is_none()));
    let files = HashMap::from([(
        101,
        vec!["/logs/one.jsonl".into(), "/logs/two.jsonl".into()],
    )]);
    reconcile(&mut workspace, &transcripts, &processes, &files).unwrap();
    assert!(workspace
        .agent_sessions
        .iter()
        .all(|s| s.terminal.is_none()));
    let files = HashMap::from([(101, vec!["/logs/two.jsonl".into()])]);
    reconcile(&mut workspace, &transcripts, &processes, &files).unwrap();
    assert_eq!(
        workspace.current_agent("pty").unwrap().cli_session_id,
        "two"
    );
}

#[test]
fn a_conversation_handed_to_the_claude_daemon_stays_with_its_terminal() {
    let mut w = Workspace::default();
    let [a, b, other] = [0, 1, 2].map(|_| uuid::Uuid::new_v4().to_string());
    w.terminals.push(Terminal {
        id: "pty-0".into(),
        space: "s".into(),
        argv: vec!["zsh".into()],
        cwd: "/same".into(),
        pid: Some(100),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    // The terminal's Claude only shows it now; the daemon runs the forked session `b`.
    let processes = vec![
        Process {
            pid: 101,
            parent: 100,
            runtime: Some(Agent::Claude),
            cli_session_id: None,
            no_daemon: false,
            launch: vec![],
        },
        Process {
            pid: 300,
            parent: 1,
            runtime: Some(Agent::Claude),
            cli_session_id: None,
            no_daemon: false,
            launch: vec![],
        },
        Process {
            pid: 301,
            parent: 300,
            runtime: Some(Agent::Claude),
            cli_session_id: Some(b.clone()),
            no_daemon: false,
            launch: vec![],
        },
    ];
    let log = |id: &str| Transcript {
        runtime: Agent::Claude,
        cli_id: id.into(),
        path: format!("/same/{id}.jsonl"),
        cwd: "/same".into(),
        updated: 1,
    };
    let (ta, tb, tc) = (log(&a), log(&b), log(&other));
    apply_hook(
        &mut w,
        Agent::Claude,
        101,
        Some("pty-0"),
        &a,
        &ta,
        &processes,
        false,
    )
    .unwrap();
    let first = w.current_agent("pty-0").unwrap().id.clone();
    w.agent_sessions[0].task = Some("todo".into());
    let current = |w: &Workspace| w.current_agent("pty-0").map(|s| s.cli_session_id.clone());
    // Before the old log says where it went, a daemon's hook belongs to nobody.
    assert!(apply_hook(&mut w, Agent::Claude, 301, None, &b, &tb, &processes, false).is_err());
    w.content.insert(
        first.clone(),
        fibo_core::content::Content {
            continued_in: b.clone(),
            ..Default::default()
        },
    );
    assert_eq!(
        continued_terminal(&w, Agent::Claude, &b).as_deref(),
        Some("pty-0")
    );
    assert_eq!(continued_terminal(&w, Agent::Claude, &other), None);
    assert!(
        apply_hook(
            &mut w,
            Agent::Claude,
            301,
            None,
            &other,
            &tc,
            &processes,
            false
        )
        .is_err(),
        "only the session the log names"
    );
    apply_hook(&mut w, Agent::Claude, 301, None, &b, &tb, &processes, false).unwrap();
    assert_eq!(current(&w), Some(b.clone()));
    assert_eq!(
        w.current_agent("pty-0").unwrap().binding,
        Some(Binding::Continued)
    );
    // Its next hooks (Stop, approvals) keep reaching the terminal.
    apply_hook(&mut w, Agent::Claude, 301, None, &b, &tb, &processes, false).unwrap();
    assert_eq!(current(&w), Some(b.clone()));
    // The old log held open by the terminal's Claude does not take it back.
    let open = HashMap::from([(101, vec![ta.path.clone()])]);
    reconcile(&mut w, &[ta.clone(), tb.clone()], &processes, &open).unwrap();
    assert_eq!(current(&w), Some(b.clone()));

    // Without hooks the log alone moves the terminal, and the todo goes along.
    let mut w = Workspace {
        terminals: w.terminals.clone(),
        ..Default::default()
    };
    apply_hook(
        &mut w,
        Agent::Claude,
        101,
        Some("pty-0"),
        &a,
        &ta,
        &processes,
        false,
    )
    .unwrap();
    let first = w.current_agent("pty-0").unwrap().id.clone();
    w.agent_sessions[0].task = Some("todo".into());
    w.content.insert(
        first,
        fibo_core::content::Content {
            continued_in: b.clone(),
            ..Default::default()
        },
    );
    reconcile(&mut w, std::slice::from_ref(&ta), &[], &HashMap::new()).unwrap();
    assert_eq!(current(&w), Some(a.clone()), "the new log is not known yet");
    reconcile(&mut w, &[ta, tb], &[], &HashMap::new()).unwrap();
    assert_eq!(current(&w), Some(b));
    let moved = w.current_agent("pty-0").unwrap();
    assert_eq!(moved.binding, Some(Binding::Continued));
    assert_eq!(moved.task.as_deref(), Some("todo"));
}

#[test]
fn a_thread_whose_pane_ended_is_compared_however_old_its_last_prompt() {
    assert!(!screen_candidate(false, false, None, true));
    assert!(screen_candidate(false, true, None, true));
    assert!(screen_candidate(true, false, None, true));
}

#[test]
fn only_a_screen_tied_thread_can_leave_its_pane_and_only_for_an_open_one() {
    let held = |binding| AgentSession {
        id: "s".into(),
        runtime: Agent::Codex,
        cli_session_id: "c".into(),
        terminal: Some("t".into()),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: 0,
        cwd: String::new(),
        transcript: String::new(),
        binding,
        name: String::new(),
    };
    let screen = held(Some(Binding::Screen));
    assert!(screen_candidate(true, false, Some(&screen), true));
    assert!(!screen_candidate(true, false, Some(&screen), false));
    let process = held(Some(Binding::Process));
    assert!(!screen_candidate(true, false, Some(&process), true));
}
