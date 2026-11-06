use super::*;

fn cli(h: &Host, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(args)
        .args(["--runtime", &h.root.to_string_lossy()])
        // Whatever runs this test may itself be a Mooring terminal.
        .env_remove("MOORING_TERMINAL")
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// What an Agent runs, as a separate process: it writes down and writes back, and has no
/// way to mark anything done.
#[test]
fn todo_commands_write_back_and_cannot_complete() {
    let h = Host::start();
    h.setup();
    let (ok, id, err) = cli(
        &h,
        &["todo", "add", "沙箱起不来", "--said", "同时开 3 个就失败"],
    );
    assert!(ok, "{err}");
    let id = id.trim().to_owned();
    assert_eq!(id.len(), 8, "{id}");

    let (ok, _, err) = cli(&h, &["todo", "note", "查清了是容量不够"]);
    assert!(!ok, "no terminal and no --task names no todo");
    assert!(err.contains("--task"), "{err}");

    let (ok, _, err) = cli(&h, &["todo", "note", "查清了是容量不够", "--task", &id]);
    assert!(ok, "{err}");
    let (ok, _, err) = cli(&h, &["todo", "note", "   ", "--task", &id]);
    assert!(!ok, "an empty note is refused");
    assert!(err.contains("进展不能为空"), "{err}");

    let w = h.workspace();
    let task = w.tasks.iter().find(|t| t.id.starts_with(&id)).unwrap();
    assert_eq!(task.description, "同时开 3 个就失败");
    let log = &w.task_logs[&task.id];
    assert!(log.created.is_some());
    assert_eq!(log.notes.len(), 1);
    assert_eq!(log.notes[0].text, "查清了是容量不够");
    assert!(w
        .task_handoff(task, "mooring todo note")
        .contains("- 查清了是容量不够"));

    let (ok, shown, err) = cli(&h, &["todo", "show", "--task", &id, "--json"]);
    assert!(ok, "{err}");
    let shown: serde_json::Value = serde_json::from_str(&shown).unwrap();
    assert_eq!(shown["said"], "同时开 3 个就失败");
    assert_eq!(shown["notes"][0]["text"], "查清了是容量不够");
    assert_eq!(shown["closed"], false);

    for word in ["done", "complete", "close"] {
        let (ok, _, err) = cli(&h, &["todo", word, "--task", &id]);
        assert!(!ok, "{word}");
        assert!(err.contains("用户"), "{err}");
    }
    let w = h.workspace();
    assert!(!w.task_outcomes.contains_key(&w.tasks.last().unwrap().id));

    // The app's ids share a long prefix: what `list` prints has to name one todo.
    edit_task(&h, "35883-1793-1");
    edit_task(&h, "35883-1793-2");
    let (_, listed, _) = cli(&h, &["todo", "list"]);
    assert!(
        listed.contains("35883-1793-1") && listed.contains("35883-1793-2"),
        "{listed}"
    );
    let (ok, _, err) = cli(&h, &["todo", "note", "写到它", "--task", "35883-1793-2"]);
    assert!(ok, "{err}");
    let (ok, _, err) = cli(&h, &["todo", "note", "哪一个", "--task", "35883-17"]);
    assert!(!ok && err.contains("多写几位"), "{err}");

    let (ok, listed, _) = cli(&h, &["todo", "list"]);
    assert!(
        ok && listed.contains("沙箱起不来") && listed.contains("容量不够"),
        "{listed}"
    );
    h.call(Request::Edit {
        request: None,
        edit: Edit::SetTaskOutcome {
            id: task_id(&h, &id),
            outcome: Outcome::Completed,
            seen: None,
        },
    })
    .unwrap();
    let (_, listed, _) = cli(&h, &["todo", "list"]);
    assert!(!listed.contains("沙箱起不来"), "{listed}");
    let (_, listed, _) = cli(&h, &["todo", "list", "--all"]);
    assert!(listed.contains("沙箱起不来"), "{listed}");
}
fn edit_task(h: &Host, id: &str) {
    let space = h.workspace().spaces[0].id.clone();
    h.call(Request::Edit {
        request: None,
        edit: Edit::AddTask(Task {
            id: id.into(),
            space,
            title: id.into(),
            description: String::new(),
            acceptance: vec![],
            agent: Agent::Claude,
        }),
    })
    .unwrap();
}
fn task_id(h: &Host, short: &str) -> String {
    let w = h.workspace();
    w.tasks
        .iter()
        .find(|t| t.id.starts_with(short))
        .unwrap()
        .id
        .clone()
}

/// The real thing, which spends model quota: Claude Code started for a todo writes
/// progress back with nothing but the handoff to go on, and a second conversation picks
/// up from it without redoing the work. Run with
/// `MOORING_REAL_CLAUDE_CWD=<a directory Claude already trusts> cargo test -- --ignored real_claude_writes`.
#[test]
#[ignore = "starts the real claude binary and calls a model"]
fn real_claude_writes_back_and_the_next_conversation_continues() {
    let cwd = std::env::var("MOORING_REAL_CLAUDE_CWD").expect("MOORING_REAL_CLAUDE_CWD");
    let h = Host::start();
    let edit = |edit: Edit| {
        h.call(Request::Edit {
            request: None,
            edit,
        })
        .unwrap()
    };
    edit(Edit::AddSpace(Space {
        id: "s".into(),
        name: "Real".into(),
        cwd: cwd.clone(),
    }));
    edit(Edit::AddTask(Task {
        id: "real-todo".into(),
        space: "s".into(),
        title: "数一下 crates 目录下有几项".into(),
        description: "运行 ls crates | wc -l，别的都不用做".into(),
        acceptance: vec![],
        agent: Agent::Claude,
    }));
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_mooring"));
    let settings = h.root.join("hooks.json");
    let command = format!(
        "MOORING_HOOK={}; {}",
        shell_quote(&binary.to_string_lossy()),
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
    let path = format!(
        "PATH={}:{}",
        binary.parent().unwrap().display(),
        std::env::var("PATH").unwrap()
    );
    let start = |pane: &str, tab: &str| {
        h.call(Request::Launch {
            id: pane.into(),
            space: "s".into(),
            argv: ["/usr/bin/env".to_owned()]
                .into_iter()
                .chain(
                    std::env::vars()
                        .map(|(k, _)| k)
                        .filter(|k| k.starts_with("CLAUDE"))
                        .flat_map(|k| ["-u".to_owned(), k]),
                )
                .chain([
                    path.clone(),
                    format!("MOORING_HOOK={}", binary.display()),
                    claude.clone(),
                    "--dangerously-skip-permissions".into(),
                    "--settings".into(),
                    settings.to_string_lossy().into_owned(),
                ])
                .collect(),
            cwd: cwd.clone(),
            cols: 120,
            rows: 40,
            scheme: Default::default(),
            colors: None,
        })
        .unwrap();
        edit(Edit::CreateTab {
            tab: Tab {
                id: tab.into(),
                space: "s".into(),
                default_task: Some("real-todo".into()),
                title: "数 crates".into(),
                layout: Some(Layout::Leaf(pane.into())),
            },
        });
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            let w = h.workspace();
            if let Some(s) = w.current_agent(pane) {
                assert_eq!(s.task.as_deref(), Some("real-todo"), "bound to the todo");
                return s.id.clone();
            }
            assert!(Instant::now() < until, "{pane} never bound");
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let ask = |pane: &str, text: &str| {
        std::thread::sleep(Duration::from_secs(3));
        let (ok, _, err) = cli(&h, &["prompt", pane, text]);
        assert!(ok, "{err}");
    };
    let wait_notes = |count: usize| {
        let until = Instant::now() + Duration::from_secs(300);
        loop {
            let w = h.workspace();
            let notes = w.task_logs.get("real-todo").map_or(0, |l| l.notes.len());
            if notes >= count {
                return w.task_logs["real-todo"].notes.clone();
            }
            assert!(Instant::now() < until, "no note {count} written back");
            std::thread::sleep(Duration::from_millis(500));
        }
    };

    let first_session = start("real-1", "tab-1");
    let w = h.workspace();
    ask("real-1", &w.task_handoff(&w.tasks[0], "mooring todo note"));
    let notes = wait_notes(1);
    eprintln!("first note: {}", notes[0].text);
    assert_eq!(notes[0].session.as_deref(), Some(first_session.as_str()));
    let digits: String = notes[0].text.chars().filter(char::is_ascii_digit).collect();
    assert!(
        !digits.is_empty(),
        "the count is in the note: {}",
        notes[0].text
    );
    let (mut stream, _, _) = h.attach("real-1", true);
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

    let second_session = start("real-2", "tab-2");
    assert_ne!(first_session, second_session);
    let w = h.workspace();
    let handoff = w.task_handoff(&w.tasks[0], "mooring todo note");
    assert!(handoff.contains(&notes[0].text), "{handoff}");
    ask(
        "real-2",
        &format!("{handoff}\n\n这次不要再运行任何统计命令。只把上面进展里记下的数字原样写成一条新进展，以「接上：」开头。"),
    );
    let notes = wait_notes(2);
    eprintln!("second note: {}", notes[1].text);
    assert_eq!(notes[1].session.as_deref(), Some(second_session.as_str()));
    assert!(
        notes[1].text.contains("接上") && notes[1].text.contains(&digits),
        "{}",
        notes[1].text
    );
    let w = h.workspace();
    assert!(
        !w.task_outcomes.contains_key("real-todo"),
        "the Agent cannot complete it"
    );
    assert_eq!(
        w.tasks[0].description, "运行 ls crates | wc -l，别的都不用做",
        "the user's words stay"
    );
}

/// The real thing again, with only the installed hook to go on: a conversation nobody
/// started from a todo writes one down when told 「记一下」, and a plain question leaves
/// nothing behind. Run like `real_claude_writes_back_and_the_next_conversation_continues`.
#[test]
#[ignore = "starts the real claude binary and calls a model"]
fn real_claude_notes_a_todo_when_told_and_only_then() {
    let cwd = std::env::var("MOORING_REAL_CLAUDE_CWD").expect("MOORING_REAL_CLAUDE_CWD");
    let h = Host::start();
    let edit = |edit: Edit| {
        h.call(Request::Edit {
            request: None,
            edit,
        })
        .unwrap()
    };
    edit(Edit::AddSpace(Space {
        id: "s".into(),
        name: "Real".into(),
        cwd: cwd.clone(),
    }));
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_mooring"));
    let settings = h.root.join("hooks.json");
    let entry = |event: &str| {
        let command = format!(
            "MOORING_HOOK={}; {}",
            shell_quote(&binary.to_string_lossy()),
            mooring_host::hooks::command_at(Agent::Claude, event)
        );
        serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "timeout": 5 }] }])
    };
    fs::write(
        &settings,
        serde_json::json!({ "hooks": {
            "SessionStart": entry("SessionStart"),
            "Stop": entry("Stop"),
            "SessionEnd": entry("SessionEnd"),
        } })
        .to_string(),
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
        argv: ["/usr/bin/env".to_owned()]
            .into_iter()
            .chain(
                std::env::vars()
                    .map(|(k, _)| k)
                    .filter(|k| k.starts_with("CLAUDE"))
                    .flat_map(|k| ["-u".to_owned(), k]),
            )
            .chain([
                format!(
                    "PATH={}:{}",
                    binary.parent().unwrap().display(),
                    std::env::var("PATH").unwrap()
                ),
                claude,
                "--dangerously-skip-permissions".into(),
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
    edit(Edit::CreateTab {
        tab: Tab {
            id: "tab".into(),
            space: "s".into(),
            default_task: None,
            title: "普通会话".into(),
            layout: Some(Layout::Leaf("real".into())),
        },
    });
    let until = Instant::now() + Duration::from_secs(60);
    let session = loop {
        if let Some(s) = h.workspace().current_agent("real") {
            break s.id.clone();
        }
        assert!(Instant::now() < until, "never bound");
        std::thread::sleep(Duration::from_millis(200));
    };
    std::thread::sleep(Duration::from_secs(3));
    let (ok, _, err) = cli(
        &h,
        &[
            "prompt",
            "real",
            "1 加 1 等于几？直接回答。",
            "--wait",
            "--timeout",
            "120",
        ],
    );
    assert!(ok, "{err}");
    assert!(
        h.workspace().tasks.is_empty(),
        "a plain question notes nothing"
    );

    let (ok, _, err) = cli(
        &h,
        &["prompt", "real", "compose 超时这个问题后面要查一下，记一下"],
    );
    assert!(ok, "{err}");
    let until = Instant::now() + Duration::from_secs(180);
    let (w, task) = loop {
        let w = h.workspace();
        if let Some(task) = w.tasks.first().cloned() {
            break (w, task);
        }
        assert!(Instant::now() < until, "nothing was noted");
        std::thread::sleep(Duration::from_millis(500));
    };
    eprintln!("noted: {} / said: {}", task.title, task.description);
    assert!(task.title.contains("compose") || task.description.contains("compose"));
    assert_eq!(
        w.task_logs[&task.id].source.as_deref(),
        Some(session.as_str())
    );
    assert_eq!(
        w.current_agent("real").unwrap().task,
        None,
        "noted in this conversation, not tied to it"
    );
    assert!(!w.task_outcomes.contains_key(&task.id));
}
