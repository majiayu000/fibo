use super::*;

/// The command line other Agents and scripts use, run as a separate process the way
/// they would: list, send and read a shell, then a prompt refused at an approval.
#[test]
fn command_line_lists_reads_sends_and_refuses_to_type_into_an_approval() {
    let h = Host::start();
    h.setup();
    let cli = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_mooring"))
            .args(args)
            .args(["--runtime", &h.root.to_string_lossy()])
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    h.call(h.launch("shell-cli", "exec /bin/sh -i")).unwrap();
    let (ok, _, err) = cli(&["send", "shell-c", "echo", "sent-$((40+2))", "--enter"]);
    assert!(ok, "{err}");
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let (ok, out, err) = cli(&["read", "shell-cli", "--lines", "5"]);
        assert!(ok, "{err}");
        if out.contains("sent-42") {
            break;
        }
        assert!(Instant::now() < until, "echo output never appeared:\n{out}");
        std::thread::sleep(Duration::from_millis(100));
    }

    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let claude = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &claude).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let seed = h.root.join("seed");
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"CLI approval test"}});
    fs::write(&seed, format!("{row}\n")).unwrap();
    let script = format!(
        "cat {} > {}; printf '%s\\n' 'Bash command' '  git push origin main' 'Do you want to proceed?' '> 1. Yes' '  2. No'; read -r answer; echo \"answer=$answer\"; read -r end",
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&logs.join(format!("{id}.jsonl")).to_string_lossy())
    );
    h.call(h.launch(
        "agent-cli",
        &format!(
            "exec {} -c {} fixture --session-id {}",
            shell_quote(&claude.to_string_lossy()),
            shell_quote(&script),
            id
        ),
    ))
    .unwrap();
    // Approval recognition reads the screen; under a loaded machine it can take a while.
    let (ok, out, err) = cli(&["wait", "agent-cli", "--until", "waiting", "--timeout", "90"]);
    assert!(ok && out.trim() == "waiting", "{out}{err}");
    let (ok, out, _) = cli(&["list"]);
    assert!(ok);
    let line = out
        .lines()
        .find(|l| l.trim_end().ends_with("agent-cli"))
        .unwrap_or_default();
    let waiting = mooring_core::status::Status::Waiting.label();
    assert!(line.contains("claude") && line.contains(waiting), "{out}");
    let (ok, _, err) = cli(&["prompt", "agent-cli", "yes", "please"]);
    assert!(!ok && err.contains("等你回答"), "{err}");
    // Nothing reached the approval.
    std::thread::sleep(Duration::from_millis(500));
    let (_, screen, _) = cli(&["read", "agent-cli"]);
    assert!(!screen.contains("answer="), "{screen}");
    let (ok, _, err) = cli(&["prompt", "agent-cli", "1", "--force"]);
    assert!(ok, "{err}");
    let until = Instant::now() + Duration::from_secs(10);
    while !cli(&["read", "agent-cli"]).1.contains("answer=1") {
        assert!(Instant::now() < until, "forced prompt never arrived");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// 允许一次 / 拒绝 from the workbench: the host answers only the approval still on
/// screen, with the cursor on its first option, and never types into what replaced it.
#[test]
fn answering_an_approval_rereads_the_screen_and_never_types_into_what_replaced_it() {
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let claude = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &claude).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let seed = h.root.join("seed");
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"answer test"}});
    fs::write(&seed, format!("{row}\n")).unwrap();
    // A second approval whose cursor sits on 「No」: Enter there would deny, so allow is refused.
    let script = format!(
        "cat {} > {}; printf '%s\\n' 'Bash command' '  git push origin main' 'Do you want to proceed?' '> 1. Yes' '  2. No'; read -r answer; printf '\\033[2J\\033[Hanswer=[%s]\\n' \"$answer\"; printf '%s\\n' 'Bash command' '  rm -rf build' 'Do you want to remove?' '  1. Yes' '> 2. No'; read -r end",
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&logs.join(format!("{id}.jsonl")).to_string_lossy())
    );
    h.call(h.launch(
        "approve",
        &format!(
            "exec {} -c {} fixture --session-id {}",
            shell_quote(&claude.to_string_lossy()),
            shell_quote(&script),
            id
        ),
    ))
    .unwrap();
    let approval = |h: &Host| {
        let w = h.workspace();
        w.current_agent("approve")
            .and_then(|s| w.content.get(&s.id))
            .map(|c| c.approval.clone())
            .unwrap_or_default()
    };
    let until = Instant::now() + Duration::from_secs(90);
    while approval(&h) != "Do you want to proceed?" {
        assert!(
            Instant::now() < until,
            "approval never showed: {:?}",
            approval(&h)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let answer = |question: &str, allow: bool| {
        h.call(Request::AnswerApproval {
            id: "approve".into(),
            question: question.into(),
            allow,
        })
    };
    let screen = |h: &Host| match h
        .call(Request::ReadScreen {
            id: "approve".into(),
            rows: 24,
        })
        .unwrap()
    {
        Reply::Text(t) => t,
        x => panic!("{x:?}"),
    };
    // Another question than the one on screen: nothing is typed.
    let e = answer("Do you want to delete?", true).unwrap_err();
    assert!(e.to_string().contains("已经没有这个审批"), "{e}");
    std::thread::sleep(Duration::from_millis(300));
    assert!(!screen(&h).contains("answer="), "{}", screen(&h));
    assert!(matches!(
        answer("Do you want to proceed?", true),
        Ok(Reply::Ok)
    ));
    let until = Instant::now() + Duration::from_secs(10);
    while !screen(&h).contains("answer=[]") {
        assert!(
            Instant::now() < until,
            "Enter never arrived: {}",
            screen(&h)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // A second press on the answered approval finds the next screen and types nothing.
    let e = answer("Do you want to proceed?", true).unwrap_err();
    assert!(e.to_string().contains("已经没有这个审批"), "{e}");
    // The cursor is on 「No」: allowing is refused, denying sends Esc.
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        match answer("Do you want to remove?", true) {
            Err(e) if e.to_string().contains("光标不在第一个选项") => break,
            r => assert!(Instant::now() < until, "cursor check: {r:?}"),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(matches!(
        answer("Do you want to remove?", false),
        Ok(Reply::Ok)
    ));
}

/// `mooring split` and `close`, run as another Agent would: a pane opens beside a
/// terminal in its tab, runs its command in a shell that stays, and closes again.
#[test]
fn command_line_splits_a_pane_beside_a_terminal_and_closes_it() {
    let h = Host::start();
    h.setup();
    let cli = |args: &[&str]| {
        // The runtime goes first: after `--` it would be part of the pane's command.
        let mut command = Command::new(env!("CARGO_BIN_EXE_mooring"));
        command.args(&args[..1]);
        command.args(["--runtime", &h.root.to_string_lossy()]);
        let out = command
            .args(&args[1..])
            .env("SHELL", "/bin/sh")
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    h.call(h.launch("base", "exec /bin/sh -i")).unwrap();
    h.call(Request::Edit {
        request: None,
        edit: Edit::CreateTab {
            tab: Tab {
                id: "t".into(),
                space: "s".into(),
                default_task: None,
                title: String::new(),
                layout: Some(Layout::Leaf("base".into())),
            },
        },
    })
    .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while !h
        .workspace()
        .terminals
        .iter()
        .any(|t| t.id == "base" && t.state == ProcessState::Running)
    {
        assert!(Instant::now() < until, "the base shell never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    let (ok, out, err) = cli(&[
        "split",
        "base",
        "--down",
        "--size",
        "0.3",
        "--",
        "echo split-$((6*7))",
    ]);
    assert!(ok, "{err}");
    let id = out.trim().to_owned();
    let w = h.workspace();
    assert_eq!(
        w.tabs[0].layout,
        Some(Layout::Split {
            axis: Axis::Down,
            ratio: 7000,
            first: Box::new(Layout::Leaf("base".into())),
            second: Box::new(Layout::Leaf(id.clone())),
        })
    );
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let (_, screen, _) = cli(&["read", &id, "--lines", "10"]);
        if screen.contains("split-42") {
            break;
        }
        assert!(Instant::now() < until, "the command never ran:\n{screen}");
        std::thread::sleep(Duration::from_millis(100));
    }
    // The shell stays after the command, so the pane keeps running.
    std::thread::sleep(Duration::from_millis(300));
    assert!(h
        .workspace()
        .terminals
        .iter()
        .any(|t| t.id == id && t.state == ProcessState::Running));

    let (ok, _, err) = cli(&["split", "nothing-like-it"]);
    assert!(!ok && err.contains("没有正在运行的终端"), "{err}");
    let before = h.workspace().terminals.len();
    let (ok, _, err) = cli(&["split", "base", "--size", "1.5"]);
    assert!(!ok && err.contains("--size"), "{err}");
    assert_eq!(
        h.workspace().terminals.len(),
        before,
        "a refused split starts nothing"
    );

    // 80 columns cannot give a fifth away: `--here` refuses before anything starts…
    let (ok, _, err) = cli(&["split", "base", "--size", "0.2", "--here"]);
    assert!(!ok && err.contains("只剩 16 列"), "{err}");
    assert_eq!(
        h.workspace().terminals.len(),
        before,
        "a split with no room starts nothing"
    );
    // …and without it the pane goes to a side tab, saying so on stderr only.
    let (ok, out, err) = cli(&["split", "base", "--size", "0.2"]);
    assert!(ok && err.contains("开在了「终端的旁屏」"), "{err}");
    let side = out.trim().to_owned();
    assert!(!side.contains('\n'), "stdout is the id alone: {out}");
    let w = h.workspace();
    let side_tab = w.terminal_tab(&side).unwrap();
    assert_eq!(
        (side_tab.id.as_str(), side_tab.title.as_str()),
        ("side~base", "终端的旁屏")
    );
    assert_eq!(
        w.tab_terminals("t"),
        ["base", &id],
        "the shown tab is untouched"
    );
    let (ok, _, err) = cli(&["split", &id, "--before"]);
    // Gone, so refused like any flag split does not take.
    assert!(!ok && err.contains("没有 --before 这个选项"), "{err}");

    // Fill the tab to four panes; the first one stays first, at the top left.
    let (ok, out, err) = cli(&["split", &id]);
    assert!(ok && err.is_empty(), "{err}");
    let third = out.trim().to_owned();
    let (ok, out, err) = cli(&["split", "base"]);
    assert!(ok && err.is_empty(), "{err}");
    let fourth = out.trim().to_owned();
    assert_eq!(
        h.workspace().tab_terminals("t"),
        ["base", &fourth, &id, &third]
    );
    // A fifth goes beside the side tab's last pane, in the same side tab.
    let (ok, _, err) = cli(&["split", "base", "--here"]);
    assert!(!ok && err.contains("已经有 4 块"), "{err}");
    let (ok, out, err) = cli(&["split", "base"]);
    assert!(ok && err.contains("已经有 4 块"), "{err}");
    let fifth = out.trim().to_owned();
    assert_eq!(
        h.workspace().tab_terminals("side~base"),
        [side.as_str(), &fifth]
    );
    // `--tab` skips the shown tab, and a side pane split again stays in the side tabs.
    let (ok, out, err) = cli(&["split", &id, "--tab"]);
    assert!(ok && err.contains("--tab"), "{err}");
    let sixth = out.trim().to_owned();
    let (ok, out, _) = cli(&["split", &sixth, "--tab"]);
    assert!(ok);
    let seventh = out.trim().to_owned();
    assert_eq!(
        h.workspace().tab_terminals("side~base"),
        [side.as_str(), &fifth, &sixth, &seventh]
    );
    let (ok, out, err) = cli(&["split", &side, "--json"]);
    assert!(ok, "{err}");
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["tab"], "side~base~2");
    assert_eq!(json["tab_title"], "终端的旁屏 2");
    assert_eq!(json["overflow"], "这个标签已经有 4 块");
    assert_eq!(h.workspace().tab_terminals("t")[0], "base");

    for pane in [&third, &fourth] {
        assert!(cli(&["close", pane]).0);
    }
    let (ok, _, err) = cli(&["close", &id]);
    assert!(ok, "{err}");
    assert_eq!(
        h.workspace().tabs[0].layout,
        Some(Layout::Leaf("base".into()))
    );
    let until = Instant::now() + Duration::from_secs(10);
    while h
        .workspace()
        .terminals
        .iter()
        .any(|t| t.id == id && t.state == ProcessState::Running)
    {
        assert!(Instant::now() < until, "a closed pane's shell must end");
        std::thread::sleep(Duration::from_millis(50));
    }
}
