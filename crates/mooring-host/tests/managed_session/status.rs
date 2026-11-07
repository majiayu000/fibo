use super::*;

#[test]
fn unattached_terminal_publishes_its_program_title_without_spinner() {
    let h = Host::start();
    h.setup();
    h.call(h.launch("shell", "printf '\\033]0;✳ 你好啊\\007'; read -r line"))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while recognition(&h)
        .terminal_titles
        .get("shell")
        .map(String::as_str)
        != Some("你好啊")
    {
        assert!(
            Instant::now() < until,
            "missing program title for a tab in the back"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// A program reports its own status with one printf (OSC 7501); it lasts until the
/// program clears it, its shell command ends, or the terminal exits.
#[test]
fn a_program_reports_its_status_until_it_ends() {
    let h = Host::start();
    h.setup();
    let until = |what: &str, ok: &dyn Fn(&RecognitionStatus) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ok(&recognition(&h)) {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(25));
        }
    };
    // "部署中" in base64.
    h.call(h.launch(
        "script",
        "printf '\\033]7501;state=working:app=deploy:progress=30:msg=6YOo572y5Lit\\033\\\\'; sleep 0.3; printf '\\033]7501;state=blocked:kind=question:app=deploy\\007'; read -r line",
    ))
    .unwrap();
    until("working reported", &|r| {
        r.terminal_reports
            .get("script")
            .is_some_and(|p| p.state != "working" || (p.msg == "部署中" && p.progress == Some(30)))
    });
    until("blocked reported", &|r| {
        r.terminal_reports.get("script").is_some_and(|p| {
            p.state == "blocked" && p.kind.as_deref() == Some("question") && p.app == "deploy"
        })
    });
    h.call(Request::Stop {
        id: "script".into(),
    })
    .unwrap();
    until("gone with its terminal", &|r| {
        !r.terminal_reports.contains_key("script")
    });
    // A script that reports working, then done, and exits: working ends at the next
    // prompt, done stays past it and goes when the next command starts.
    h.call(h.launch(
        "command",
        "printf '\\033]133;C\\007\\033]7501;state=done\\007'; sleep 0.2; printf '\\033]133;D;0\\007\\033]133;A\\007\\033]0;back\\007'; sleep 0.5; printf '\\033]133;C\\007\\033]0;next\\007'; read -r line",
    ))
    .unwrap();
    until("done kept past the prompt", &|r| {
        r.terminal_titles.get("command").map(String::as_str) == Some("back")
            && r.terminal_reports
                .get("command")
                .is_some_and(|p| p.state == "done")
    });
    until("cleared when the next command started", &|r| {
        r.terminal_titles.get("command").map(String::as_str) == Some("next")
            && !r.terminal_reports.contains_key("command")
    });
    // A program asking whether the protocol is spoken gets the answer as its input; the
    // line discipline echoes it, so it shows on screen.
    h.call(h.launch("probe", "printf '\\033]7501;?\\033\\\\'; read -r line"))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let screen = match h
            .call(Request::ReadScreen {
                id: "probe".into(),
                rows: 40,
            })
            .unwrap()
        {
            Reply::Text(t) => t,
            other => panic!("{other:?}"),
        };
        if screen.contains("7501;?:states=idle,working,done,blocked,error") {
            break;
        }
        assert!(Instant::now() < deadline, "no probe answer: {screen:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `mooring run`'s report back: a child Agent's finished turn goes into its starter's
/// input, wrapped, once the starter is idle.
#[test]
fn a_child_agents_finished_turn_is_typed_into_its_starter() {
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let claude = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &claude).unwrap();
    let line = |v: serde_json::Value| format!("{v}\n");
    let user = |id: &str, text: &str| {
        line(
            serde_json::json!({"type":"user","sessionId":id,"uuid":format!("u-{id}"),"cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":text}}),
        )
    };
    let reply = |id: &str, text: &str| {
        line(
            serde_json::json!({"type":"assistant","sessionId":id,"uuid":format!("a-{id}"),"timestamp":mooring_core::content::now(),"message":{"model":"claude","stop_reason":"end_turn","content":[{"type":"text","text":text}]}}),
        )
    };
    let [parent_id, child_id] = ids::<2>();
    let file = |name: &str, text: String| {
        let p = h.root.join(name);
        fs::write(&p, text).unwrap();
        shell_quote(&p.to_string_lossy())
    };
    let log = |id: &str| shell_quote(&logs.join(format!("{id}.jsonl")).to_string_lossy());
    // The starter: one finished turn, then it records whatever is typed into it.
    let got = h.root.join("got");
    let parent_seed = file(
        "parent-seed",
        user(&parent_id, "派 codex 审代码") + &reply(&parent_id, "已经派出去了"),
    );
    let parent_script = format!(
        "cat {parent_seed} > {}; printf '\\033[?2004h> '; while read -r l; do printf '%s\\n' \"$l\" >> {}; done",
        log(&parent_id),
        shell_quote(&got.to_string_lossy()),
    );
    // The child: a turn running until it is handed its task, then its reply.
    let child_seed = file("child-seed", user(&child_id, "开始"));
    let child_done = file("child-done", reply(&child_id, "审完了，没发现问题"));
    let child_script = format!(
        "cat {child_seed} > {}; read -r task; cat {child_done} >> {}; read -r end",
        log(&child_id),
        log(&child_id),
    );
    for (terminal, script, id) in [
        ("parent", &parent_script, &parent_id),
        ("child", &child_script, &child_id),
    ] {
        h.call(h.launch(
            terminal,
            &format!(
                "exec {} -c {} fixture --session-id {}",
                shell_quote(&claude.to_string_lossy()),
                shell_quote(script),
                id
            ),
        ))
        .unwrap();
    }
    wait_for(&h, "both bound", |w| {
        current_is(w, "parent", &parent_id) && current_is(w, "child", &child_id)
    });
    h.call(Request::ReportTo {
        child: "child".into(),
        parent: "parent".into(),
    })
    .unwrap();
    assert!(h
        .call(Request::ReportTo {
            child: "child".into(),
            parent: "child".into(),
        })
        .is_err());
    h.call(Request::SendText {
        session: None,
        id: "child".into(),
        text: "审一下改动".into(),
        submit: true,
    })
    .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let typed = fs::read_to_string(&got).unwrap_or_default();
        if typed.contains("不是用户说的话") {
            assert!(typed.contains("<mooring from=\"child\" agent=\"Claude Code\" event=\"done\">\n审完了，没发现问题\n</mooring>"), "{typed}");
            break;
        }
        assert!(
            Instant::now() < until,
            "no report reached the starter: {typed:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn quiet_agent_screen_with_an_approval_waits_on_the_user_until_they_answer() {
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let path = logs.join(format!("{id}.jsonl"));
    let seed = h.root.join("seed");
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"Screen approval test"}});
    fs::write(&seed, format!("{row}\n")).unwrap();
    // The log never records the pending permission; only the screen shows it.
    let script = format!(
        "cat {} > {}; read -r go; printf '%s\\n' 'Bash command' '  git push origin main' 'Do you want to proceed?' '> 1. Yes' '  2. No'; read -r answer; printf '\\033[2J\\033[Hpushed\\n'; read -r end",
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&path.to_string_lossy())
    );
    h.call(h.launch(
        "approval",
        &format!(
            "exec {} -c {} fixture --session-id {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script),
            id
        ),
    ))
    .unwrap();
    let waiting = |w: &Workspace| {
        w.current_agent("approval")
            .and_then(|s| w.content.get(&s.id))
            .filter(|c| c.turn == mooring_core::content::Turn::Waiting)
            .map(|c| c.waiting.clone())
    };
    let until = Instant::now() + Duration::from_secs(30);
    while h.workspace().current_agent("approval").is_none() {
        assert!(Instant::now() < until, "explicit ID must bind");
        std::thread::sleep(Duration::from_millis(50));
    }
    let (mut stream, _vt, writable) = h.attach("approval", true);
    assert!(writable);
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
    // The prompt follows the user's own keystroke, so it is only attributed to the agent
    // after the input settles, then once.
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(waiting(&h.workspace()), None);
    let until = Instant::now() + Duration::from_secs(10);
    let question = loop {
        if let Some(q) = waiting(&h.workspace()) {
            break q;
        }
        assert!(
            Instant::now() < until,
            "quiet approval screen must wait on the user"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(question, "Do you want to proceed?（画面上的提示）");
    // The card says which rule read it and the lines it saw.
    let card = |w: &Workspace| {
        w.current_agent("approval")
            .and_then(|s| w.content.get(&s.id))
            .cloned()
            .unwrap()
    };
    let detail = card(&h.workspace()).signal_detail;
    assert!(
        detail.starts_with("规则：approval 审批（claude 内置 "),
        "{detail}"
    );
    assert!(
        detail.contains("看到：Do you want to proceed? / > 1. Yes / 2. No"),
        "{detail}"
    );
    // What read it stays on disk: the screen itself is gone by the time a wrong card is noticed.
    let wait_log = || {
        fs::read_to_string(h.root.join("screen-waits.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .collect::<Vec<_>>()
    };
    let logged = wait_log();
    assert_eq!(logged.len(), 1, "one question, one entry: {logged:?}");
    assert_eq!(logged[0]["event"], "waiting");
    assert_eq!(logged[0]["terminal"], "approval");
    assert_eq!(logged[0]["rule"], "approval");
    assert_eq!(logged[0]["session"], id.as_str());
    assert!(logged[0]["screen"]
        .to_string()
        .contains("Do you want to proceed?"));
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: b"1\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut stream), Reply::Ok));
    assert_eq!(waiting(&h.workspace()), None, "answering clears it at once");
    // The answered prompt is gone from the screen, so the next quiet check stays clear
    // and says it found no prompt.
    std::thread::sleep(Duration::from_millis(4500));
    assert_eq!(waiting(&h.workspace()), None);
    let check = card(&h.workspace()).screen_check;
    assert!(
        check.starts_with("没认出提问框（底部没有审批或选择题的提示"),
        "{check}"
    );
    let logged = wait_log();
    assert_eq!(logged.len(), 2, "answered, it ends once: {logged:?}");
    assert_eq!(logged[1]["event"], "ended");
    assert_eq!(logged[1]["terminal"], "approval");
    assert_eq!(recognition(&h).metrics.lsof_spawns, 0);
}

#[test]
fn a_dialog_gone_from_the_screen_is_settled_by_claude_official_status() {
    use std::os::unix::fs::PermissionsExt;
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let path = logs.join(format!("{id}.jsonl"));
    let seed = h.root.join("seed");
    let at = mooring_core::content::now();
    let rows = [
        serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":at,"message":{"content":"Official status test"}}),
        serde_json::json!({"type":"assistant","sessionId":id,"timestamp":at,"message":{"content":[{"type":"tool_use","id":"ask","name":"AskUserQuestion","input":{"questions":[{"question":"选哪个环境？","options":[{"label":"预发"},{"label":"生产"}]}]}}]}}),
    ];
    fs::write(
        &seed,
        rows.iter().map(|r| format!("{r}\n")).collect::<String>(),
    )
    .unwrap();
    // Claude Code says the session sits at its prompt: the dialog was closed with Esc.
    let calls = h.root.join("agents-calls");
    let agents = h.root.join("claude-agents");
    fs::write(
        &agents,
        format!(
            "#!/bin/sh\necho x >> {}\nprintf '%s' '[{{\"pid\":1,\"kind\":\"interactive\",\"sessionId\":\"{id}\",\"status\":\"idle\"}}]'\n",
            shell_quote(&calls.to_string_lossy())
        ),
    )
    .unwrap();
    fs::set_permissions(&agents, fs::Permissions::from_mode(0o755)).unwrap();
    // The screen never shows the question; one quiet screen is enough, the hold re-checks.
    // That screen check runs once per output burst, so the burst waits for the logged
    // question: a check that ran before the log was indexed would find nothing to settle.
    let gate = h.root.join("dialog-closed");
    let script = format!(
        "cat {} > {}; while [ ! -e {} ]; do sleep 0.05; done; printf 'working\\n'; read -r end",
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&path.to_string_lossy()),
        shell_quote(&gate.to_string_lossy())
    );
    h.call(h.launch(
        "official",
        &format!(
            "exec {} -c {} fixture --session-id {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script),
            id
        ),
    ))
    .unwrap();
    let turn = |w: &Workspace| {
        w.current_agent("official")
            .and_then(|s| w.content.get(&s.id))
            .map(|c| (c.turn, c.signal.clone()))
    };
    let until = Instant::now() + Duration::from_secs(30);
    while turn(&h.workspace()).map(|t| t.0) != Some(mooring_core::content::Turn::Waiting) {
        assert!(
            Instant::now() < until,
            "the logged question must wait on the user"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    fs::write(&gate, "").unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    let settled = loop {
        let t = turn(&h.workspace()).unwrap();
        if t.0 != mooring_core::content::Turn::Waiting {
            break t;
        }
        assert!(
            Instant::now() < until,
            "official idle must settle the closed dialog"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(
        settled,
        (mooring_core::content::Turn::Interrupted, "官方状态".into())
    );
    std::thread::sleep(Duration::from_secs(3));
    let runs = fs::read_to_string(&calls).unwrap().lines().count();
    assert!(
        (1..=2).contains(&runs),
        "one run per trigger within the rate limit, got {runs}"
    );
}

/// The Agent draws its approval before Mooring knows which conversation it is (here the
/// log appears 3 s later, as when identification is slow under load). The screen stays
/// still after that, so only binding the conversation can trigger the check.
#[test]
fn an_approval_drawn_before_the_conversation_is_bound_still_waits_on_the_user() {
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let claude = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &claude).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let seed = h.root.join("seed");
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"Late binding test"}});
    fs::write(&seed, format!("{row}\n")).unwrap();
    let script = format!(
        "printf '%s\\n' 'Bash command' '  rm -rf build' 'Do you want to proceed?' '> 1. Yes' '  2. No'; sleep 3; cat {} > {}; read -r answer",
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&logs.join(format!("{id}.jsonl")).to_string_lossy())
    );
    h.call(h.launch(
        "late",
        &format!(
            "exec {} -c {} fixture --session-id {}",
            shell_quote(&claude.to_string_lossy()),
            shell_quote(&script),
            id
        ),
    ))
    .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while h.workspace().current_agent("late").is_none() {
        assert!(Instant::now() < until, "explicit ID must bind");
        std::thread::sleep(Duration::from_millis(50));
    }
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let w = h.workspace();
        let turn = w
            .current_agent("late")
            .and_then(|s| w.content.get(&s.id))
            .map(|c| c.turn);
        if turn == Some(mooring_core::content::Turn::Waiting) {
            break;
        }
        assert!(
            Instant::now() < until,
            "a dialog already on screen at binding is never seen: {turn:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
