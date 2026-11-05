use super::*;
#[test]
fn the_workbench_db_commits_with_synchronous_normal() {
    let dir = std::env::temp_dir().join(format!("kl-sync-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (db, _) = Store::open(&dir).unwrap();
    let sync: i64 =
        db.0.query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
    assert_eq!(
        sync, 1,
        "workbench.sqlite must stay WAL + synchronous=NORMAL"
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn launch_flags_survive_reopen_and_go_when_dropped() {
    let dir = std::env::temp_dir().join(format!("kl-launch-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut db, before) = Store::open(&dir).unwrap();
    let mut after = before.clone();
    after.agent_sessions.push(AgentSession {
        id: "a".into(),
        runtime: Agent::Codex,
        cli_session_id: "c".into(),
        terminal: None,
        current: false,
        task: None,
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp".into(),
        transcript: String::new(),
        binding: None,
        name: String::new(),
    });
    let flags = vec![
        "-m".to_owned(),
        "gpt-5".into(),
        "-c".into(),
        "a=\"b c\"".into(),
    ];
    after.agent_launch.insert("a".into(), flags.clone());
    db.save(&before, &after).unwrap();
    drop(db);
    let (mut db, loaded) = Store::open(&dir).unwrap();
    assert_eq!(loaded.agent_launch.get("a"), Some(&flags));
    let mut dropped = loaded.clone();
    dropped.agent_launch.clear();
    db.save(&loaded, &dropped).unwrap();
    drop(db);
    let (_, again) = Store::open(&dir).unwrap();
    assert!(again.agent_launch.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn settings_and_notification_receipts_survive_restart() {
    let dir = std::env::temp_dir().join(format!("kl-settings-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut db, before) = Store::open(&dir).unwrap();
    assert!(!before.settings.notifications);
    let mut after = before.clone();
    after.settings = Settings {
        notifications: true,
        sound: true,
        stale_days: 7,
        away_minutes: 45,
        default_agent: Agent::Claude,
    };
    db.save(&before, &after).unwrap();
    assert!(db.claim_notification("session", "turn1:waiting").unwrap());
    assert!(!db.claim_notification("session", "turn1:waiting").unwrap());
    drop(db);
    let (db, loaded) = Store::open(&dir).unwrap();
    assert_eq!(loaded.settings, after.settings);
    assert!(!db.claim_notification("session", "turn1:waiting").unwrap());
    assert!(db.claim_notification("session", "turn2:waiting").unwrap());
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn a_closed_tab_is_gone_after_reopen() {
    let dir = std::env::temp_dir().join(format!("kl-tabs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut db, before) = Store::open(&dir).unwrap();
    let mut after = before.clone();
    after.spaces.push(Space {
        id: "s".into(),
        name: "s".into(),
        cwd: "/tmp".into(),
    });
    for id in ["kept", "closed"] {
        after.terminals.push(Terminal {
            id: id.into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into()],
            cwd: "/tmp".into(),
            pid: None,
            state: ProcessState::Exited,
            exit_code: Some(0),
            error: None,
        });
        after.tabs.push(Tab {
            id: id.into(),
            space: "s".into(),
            default_task: None,
            title: String::new(),
            layout: Some(Layout::Leaf(id.into())),
        });
    }
    db.save(&before, &after).unwrap();
    let mut closed = after.clone();
    closed
        .edit(Edit::CloseTab {
            id: "closed".into(),
            expect: None,
        })
        .unwrap();
    db.save(&after, &closed).unwrap();
    drop(db);
    let (db, loaded) = Store::open(&dir).unwrap();
    let ids: Vec<_> = loaded.tabs.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["kept"]);
    let rows: i64 =
        db.0.query_row("SELECT count(*) FROM tab_layout_nodes", [], |r| r.get(0))
            .unwrap();
    assert_eq!(rows, 1);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
/// What a current build does to tabs (moving a tab's last pane away, closing panes, a
/// restart that interrupts their terminals) leaves no tab without panes or a place in the
/// order, so the old-format refusal never fires on it.
#[test]
fn current_tab_edits_and_restarts_reopen() {
    let dir = std::env::temp_dir().join(format!("kl-tabpaths-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut db, before) = Store::open(&dir).unwrap();
    let mut w = before.clone();
    w.spaces.push(Space {
        id: "s".into(),
        name: "s".into(),
        cwd: "/tmp".into(),
    });
    for id in ["a", "b", "c"] {
        w.terminals.push(Terminal {
            id: id.into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into()],
            cwd: "/tmp".into(),
            pid: None,
            state: ProcessState::Running,
            exit_code: None,
            error: None,
        });
        w.edit(Edit::CreateTab {
            tab: Tab {
                id: id.into(),
                space: "s".into(),
                default_task: None,
                title: String::new(),
                layout: Some(Layout::Leaf(id.into())),
            },
        })
        .unwrap();
    }
    db.save(&before, &w).unwrap();
    let mut next = w.clone();
    next.edit(Edit::MovePane {
        terminal: "a".into(),
        target: "b".into(),
        side: Some((mooring_core::layout::Axis::Right, false)),
    })
    .unwrap();
    db.save(&w, &next).unwrap();
    let mut last = next.clone();
    last.edit(Edit::ClosePane {
        terminal: "c".into(),
    })
    .unwrap();
    last.edit(Edit::ClosePane {
        terminal: "b".into(),
    })
    .unwrap();
    db.save(&next, &last).unwrap();
    drop(db);
    for _ in 0..2 {
        let (_, loaded) = Store::open(&dir).unwrap();
        let ids: Vec<_> = loaded.tabs.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["b"]);
        assert_eq!(loaded.tabs[0].layout, Some(Layout::Leaf("a".into())));
        assert_eq!(loaded.terminals[0].state, ProcessState::Interrupted);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn an_old_format_database_is_refused_and_left_as_it_was() {
    let cases = [
        "CREATE TABLE workspace (id INTEGER PRIMARY KEY, json TEXT NOT NULL);",
        "CREATE TABLE terminals (id TEXT PRIMARY KEY, position INTEGER NOT NULL);",
        "INSERT INTO spaces VALUES('s','s','/tmp'); INSERT INTO tabs VALUES('hidden','s',NULL,'',1);
         INSERT INTO tab_order VALUES('hidden',0); INSERT INTO terminals VALUES('p','s','[]','/tmp',NULL,'Exited',0,NULL);
         INSERT INTO tab_layout_nodes VALUES('hidden','','p',NULL,NULL);",
        "INSERT INTO spaces VALUES('s','s','/tmp'); INSERT INTO tabs VALUES('empty','s',NULL,'',0);
         INSERT INTO tab_order VALUES('empty',0);",
        "INSERT INTO spaces VALUES('s','s','/tmp'); INSERT INTO tabs VALUES('unordered','s',NULL,'',0);
         INSERT INTO terminals VALUES('p','s','[]','/tmp',NULL,'Exited',0,NULL);
         INSERT INTO tab_layout_nodes VALUES('unordered','','p',NULL,NULL);",
    ];
    for old in cases {
        let dir = std::env::temp_dir().join(format!("kl-old-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("workbench.sqlite");
        if old.starts_with("CREATE") {
            Connection::open(&file).unwrap().execute_batch(old).unwrap();
        } else {
            drop(Store::open(&dir).unwrap());
            Connection::open(&file).unwrap().execute_batch(old).unwrap();
        }
        let dump = |file: &Path| -> Vec<String> {
            let db = Connection::open(file).unwrap();
            let mut tables = db
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            let names: Vec<String> = tables
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            names
                .iter()
                .map(|t| {
                    let n: i64 = db
                        .query_row(&format!("SELECT count(*) FROM \"{t}\""), [], |r| r.get(0))
                        .unwrap();
                    format!("{t}:{n}")
                })
                .collect()
        };
        let kept = dump(&file);
        let error = Store::open(&dir)
            .err()
            .unwrap_or_else(|| panic!("opened {old}"));
        let message = error.to_string();
        assert!(message.contains("旧版本的格式"), "{message}");
        assert!(message.contains("--runtime"), "{message}");
        assert_eq!(dump(&file), kept, "{old}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn a_restart_records_which_conversation_each_lost_terminal_was_running() {
    let dir = std::env::temp_dir().join(format!("kl-lost-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut db, before) = Store::open(&dir).unwrap();
    let mut after = before.clone();
    after.spaces.push(Space {
        id: "s".into(),
        name: "s".into(),
        cwd: "/tmp".into(),
    });
    for id in ["agent", "shell", "handed"] {
        after.terminals.push(Terminal {
            id: id.into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into()],
            cwd: "/tmp".into(),
            pid: Some(1),
            state: ProcessState::Running,
            exit_code: None,
            error: None,
        });
    }
    let session = |id: &str, terminal: &str, current| AgentSession {
        id: id.into(),
        runtime: Agent::Codex,
        cli_session_id: id.into(),
        terminal: Some(terminal.into()),
        current,
        task: None,
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp".into(),
        transcript: String::new(),
        binding: None,
        name: String::new(),
    };
    after.agent_sessions.extend([
        session("running", "agent", true),
        session("quit", "shell", false),
        session("moved", "handed", true),
    ]);
    db.save(&before, &after).unwrap();
    drop(db);
    let kept = ["handed".to_owned()].into_iter().collect();
    let (db, loaded) = Store::open_keeping(&dir, &kept).unwrap();
    drop(db);
    let expected: std::collections::HashMap<_, _> = [
        ("agent".to_owned(), Some("running".to_owned())),
        ("shell".to_owned(), None),
    ]
    .into_iter()
    .collect();
    assert_eq!(loaded.lost_agents, expected);
    assert!(!loaded
        .agent_sessions
        .iter()
        .any(|s| s.id == "running" && s.current));
    // A second restart keeps the record; the terminals are no longer running.
    let (_, again) = Store::open_keeping(&dir, &kept).unwrap();
    assert_eq!(again.lost_agents, expected);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn tab_order_and_removed_spaces_survive_reopen() {
    let dir = std::env::temp_dir().join(format!("kl-order-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut store, mut w) = Store::open(&dir).unwrap();
    let before = w.clone();
    for id in ["keep", "gone"] {
        w.spaces.push(Space {
            id: id.into(),
            name: id.into(),
            cwd: "/tmp".into(),
        });
    }
    for (i, space) in ["keep", "keep", "keep", "gone"].iter().enumerate() {
        w.terminals.push(Terminal {
            id: format!("p{i}"),
            space: (*space).into(),
            argv: vec!["/bin/sh".into()],
            cwd: "/tmp".into(),
            pid: None,
            state: ProcessState::Running,
            exit_code: None,
            error: None,
        });
        w.tabs.push(Tab {
            id: format!("t{i}"),
            space: (*space).into(),
            default_task: None,
            title: String::new(),
            layout: Some(Layout::Leaf(format!("p{i}"))),
        });
    }
    w.tasks.push(Task {
        id: "task".into(),
        space: "gone".into(),
        title: "unfinished".into(),
        description: String::new(),
        agent: Agent::Codex,
        acceptance: vec![],
    });
    store.save(&before, &w).unwrap();
    let before = w.clone();
    w.edit(Edit::MoveTab {
        id: "t2".into(),
        before: Some("t0".into()),
    })
    .unwrap();
    let err = w
        .clone()
        .edit(Edit::RemoveSpace { id: "gone".into() })
        .unwrap_err();
    assert!(err.contains("1 个没收尾的待办"), "{err}");
    w.edit(Edit::SetTaskOutcome {
        seen: None,
        id: "task".into(),
        outcome: Outcome::Abandoned,
    })
    .unwrap();
    w.edit(Edit::RemoveSpace { id: "gone".into() }).unwrap();
    assert!(
        w.clone()
            .edit(Edit::RemoveSpace { id: "keep".into() })
            .is_err(),
        "last space stays"
    );
    store.save(&before, &w).unwrap();
    drop(store);
    let (_, w) = Store::open(&dir).unwrap();
    assert_eq!(
        w.tabs.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["t2", "t0", "t1"]
    );
    assert_eq!(
        w.spaces.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["keep"]
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn client_departure_survives_reopen_without_changing_sessions() {
    let dir = std::env::temp_dir().join(format!("kl-visit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut db, before) = Store::open(&dir).unwrap();
    let mut after = before.clone();
    after.last_left = 12345;
    db.save(&before, &after).unwrap();
    drop(db);
    let (_, loaded) = Store::open(&dir).unwrap();
    assert_eq!(loaded.last_left, 12345);
    assert!(loaded.agent_sessions.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn task_outcome_and_pending_override_survive_reopen() {
    let root = std::env::temp_dir().join(format!("kl-task-store-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (mut store, mut w) = Store::open(&root).unwrap();
    let before = w.clone();
    w.spaces.push(Space {
        id: "s".into(),
        name: "s".into(),
        cwd: "/tmp".into(),
    });
    w.tasks.push(Task {
        id: "t".into(),
        space: "s".into(),
        title: "task".into(),
        description: "original".into(),
        agent: Agent::Codex,
        acceptance: vec![AcceptanceCriterion {
            id: "c".into(),
            description: "check".into(),
            evidence: vec![CriterionEvidence {
                session: "s1".into(),
                record: mooring_core::content::Evidence {
                    kind: "test".into(),
                    summary: "cargo test：test result: ok. 4 passed; 0 failed".into(),
                    exit_code: Some(0),
                    at: 42,
                    source: mooring_core::content::Source {
                        path: "/log".into(),
                        inode: 7,
                        offset: 42,
                    },
                },
            }],
        }],
    });
    w.terminals.push(Terminal {
        id: "p".into(),
        space: "s".into(),
        argv: vec!["/bin/sh".into()],
        cwd: "/tmp".into(),
        pid: None,
        state: ProcessState::Exited,
        exit_code: Some(0),
        error: None,
    });
    w.edit(Edit::SetPendingTask {
        terminal: "p".into(),
        task: None,
    })
    .unwrap();
    w.edit(Edit::SetTaskOutcome {
        seen: None,
        id: "t".into(),
        outcome: Outcome::Deferred {
            reason: "await review".into(),
            until: 1000,
        },
    })
    .unwrap();
    w.agent_sessions.push(AgentSession {
        id: "s1".into(),
        runtime: Agent::Codex,
        cli_session_id: "s1".into(),
        terminal: None,
        current: false,
        task: None,
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp".into(),
        transcript: "/fixture".into(),
        binding: None,
        name: String::new(),
    });
    w.change_outcome("s1", Outcome::Completed, 2, "用户操作")
        .unwrap();
    w.edit(Edit::RenameSession {
        id: "s1".into(),
        name: "  修复   登录  ".into(),
    })
    .unwrap();
    store.save(&before, &w).unwrap();
    drop(store);
    let (mut store, read) = Store::open(&root).unwrap();
    assert_eq!(read.agent_sessions[0].name, "修复 登录");
    // Clearing the name gives it back to the CLI and the first ask.
    let mut cleared = read.clone();
    cleared
        .edit(Edit::RenameSession {
            id: "s1".into(),
            name: String::new(),
        })
        .unwrap();
    store.save(&read, &cleared).unwrap();
    drop(store);
    let (store, read) = Store::open(&root).unwrap();
    assert_eq!(read.agent_sessions[0].name, "");
    assert_eq!(read.tasks, w.tasks);
    assert_eq!(read.task_outcomes, w.task_outcomes);
    assert_eq!(read.outcome_history, w.outcome_history);
    assert_eq!(read.pending_tasks, w.pending_tasks);
    drop(store);
    // Evidence that no longer parses fails the load instead of reading as none, which
    // would quietly take a criterion back from proven to open.
    Connection::open(root.join("workbench.sqlite"))
        .unwrap()
        .execute(
            "UPDATE task_criteria SET evidence='[{\"session\":' WHERE id='c'",
            [],
        )
        .unwrap();
    let error = Store::open(&root)
        .err()
        .expect("broken evidence must not load");
    assert!(error.to_string().contains("证据"), "{error}");
    std::fs::remove_dir_all(root).unwrap();
}
