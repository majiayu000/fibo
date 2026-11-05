use super::*;
fn workspace() -> Workspace {
    let mut w = Workspace::default();
    for id in ["a", "b"] {
        w.spaces.push(Space {
            id: id.into(),
            name: id.into(),
            cwd: format!("/tmp/{id}"),
        });
        w.tasks.push(Task {
            id: id.into(),
            space: id.into(),
            title: id.into(),
            description: String::new(),
            acceptance: vec![],
            agent: Agent::Codex,
        });
    }
    w.terminals.push(Terminal {
        id: "pty".into(),
        space: "a".into(),
        argv: vec!["/bin/sh".into()],
        cwd: "/tmp/a".into(),
        pid: Some(42),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    w
}
fn observation(id: &str) -> AgentSession {
    AgentSession {
        id: id.into(),
        runtime: Agent::Codex,
        cli_session_id: id.into(),
        terminal: Some("pty".into()),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp/a".into(),
        transcript: format!("/logs/{id}.jsonl"),
        binding: None,
        name: String::new(),
    }
}
/// Marking done goes through however often Agent output moved the revision, and is
/// refused only when the session itself moved on since the sender looked.
#[test]
fn outcome_edits_depend_on_what_their_sender_saw_not_the_revision() {
    let mut w = workspace();
    w.observe_agent(observation("s")).unwrap();
    w.content.insert(
        "s".into(),
        crate::content::Content {
            turn_key: "t1".into(),
            ..Default::default()
        },
    );
    let seen = w.clone();
    let done = |seen: &Workspace| {
        let mut e = Edit::SetOutcome {
            id: "s".into(),
            outcome: Outcome::Completed,
            seen: None,
        };
        seen.stamp(&mut e);
        e
    };
    // Output moves the revision; the edit made on the older copy still applies.
    w.revision += 100;
    w.edit(done(&seen)).unwrap();
    assert_eq!(w.agent_sessions[0].outcome, Outcome::Completed);
    // Sent twice, it is what was asked already.
    w.edit(done(&seen)).unwrap();
    // Reopened, then the Agent starts another turn: the old look is refused.
    w.change_outcome("s", Outcome::Open, 0, "test").unwrap();
    let seen = w.clone();
    w.content.get_mut("s").unwrap().turn_key = "t2".into();
    let err = w.edit(done(&seen)).unwrap_err();
    assert!(err.contains("新的一轮"), "{err}");
    // Changed by someone else since: refused too.
    let seen = w.clone();
    w.change_outcome("s", Outcome::Abandoned, 0, "test")
        .unwrap();
    let err = w.edit(done(&seen)).unwrap_err();
    assert!(err.contains("刚被改过"), "{err}");
    // Scripts that say nothing of what they saw are applied as before.
    w.edit(Edit::SetOutcome {
        id: "s".into(),
        outcome: Outcome::Completed,
        seen: None,
    })
    .unwrap();
    // A todo's outcome the same way.
    let seen = w.clone();
    w.edit(Edit::SetTaskOutcome {
        id: "a".into(),
        outcome: Outcome::Abandoned,
        seen: None,
    })
    .unwrap();
    let mut e = Edit::SetTaskOutcome {
        id: "a".into(),
        outcome: Outcome::Completed,
        seen: None,
    };
    seen.stamp(&mut e);
    assert!(w.edit(e).unwrap_err().contains("刚被改过"));
}
#[test]
fn a_temporary_directory_hides_only_sessions_mooring_did_not_start() {
    let mut c = crate::content::Content {
        hidden_reason: crate::content::TEMPORARY_DIRECTORY.into(),
        ..Default::default()
    };
    let mut s = observation("tmp");
    assert_eq!(c.hidden_for(&s), None);
    s.terminal = None;
    assert!(c.hidden_for(&s).is_some());
    // Real automation stays hidden even in a Mooring terminal.
    s.terminal = Some("pty".into());
    c.hidden_reason = "Claude 子代理 / sidechain".into();
    assert!(c.hidden_for(&s).is_some());
}
fn tab(w: &mut Workspace, id: &str, space: &str, task: &str) {
    w.edit(Edit::CreateTab {
        tab: Tab {
            id: id.into(),
            space: space.into(),
            default_task: Some(task.into()),
            title: String::new(),
            layout: Some(Layout::Leaf("pty".into())),
        },
    })
    .unwrap();
}
#[test]
fn settings_reject_invalid_intervals_and_drive_stale_boundary() {
    let mut w = Workspace::default();
    let settings = Settings {
        stale_days: 0,
        ..Default::default()
    };
    assert!(w.edit(Edit::Settings(settings)).is_err());
    assert_eq!(w.settings, Settings::default());
    let settings = Settings {
        stale_days: 7,
        away_minutes: 45,
        ..Default::default()
    };
    w.edit(Edit::Settings(settings.clone())).unwrap();
    assert_eq!(w.settings, settings);
}
#[test]
fn outcomes_expire_once_preserve_history_and_leave_processes_alone() {
    use crate::content::{Content, Turn};
    use crate::status::{Status, of};
    let mut w = workspace();
    w.observe_agent(observation("one")).unwrap();
    let now = 4 * 86400;
    let c = Content {
        interactive: true,
        updated: 1,
        turn_started: 1,
        turn: Turn::Completed,
        ..Default::default()
    };
    w.content.insert("one".into(), c.clone());
    // Its terminal still runs: an agent open in front of you is never 很久没动.
    assert_eq!(of(&w, &w.agent_sessions[0], now), Status::Review);
    w.change_outcome(
        "one",
        Outcome::Deferred {
            reason: "check later".into(),
            until: now + 2,
        },
        now,
        "用户操作",
    )
    .unwrap();
    assert_eq!(w.next_deadline(now), Some(now + 2));
    assert!(!w.expire_outcomes(now + 1));
    assert!(w.expire_outcomes(now + 2));
    assert!(!w.expire_outcomes(now + 3));
    assert_eq!(w.agent_sessions[0].outcome, Outcome::Open);
    assert_eq!(of(&w, &w.agent_sessions[0], now + 2), Status::Review);
    assert_eq!(w.outcome_history.len(), 2);
    w.change_outcome("one", Outcome::Abandoned, now + 3, "用户操作")
        .unwrap();
    w.change_outcome("one", Outcome::Open, now + 4, "用户操作")
        .unwrap();
    assert_eq!(w.outcome_history.len(), 4);
    assert_eq!(w.terminals[0].state, ProcessState::Running);
    assert_eq!(
        w.agent_sessions[0].updated, 1,
        "outcome dates are not log activity"
    );
    assert_eq!(w.next_deadline(now + 4), Some(now + 2 + 3 * 86400 + 1));
}
#[test]
fn task_start_reuses_running_shell_and_unlink_is_consumed_once() {
    let mut w = workspace();
    tab(&mut w, "tab", "a", "a");
    assert_eq!(w.task_terminal("a").unwrap().id, "pty");
    w.edit(Edit::SetPendingTask {
        terminal: "pty".into(),
        task: None,
    })
    .unwrap();
    w.observe_agent(observation("cancelled")).unwrap();
    assert!(w.agent_sessions[0].task.is_none());
    assert!(!w.pending_tasks.contains_key("pty"));
    w.observe_agent(observation("next")).unwrap();
    assert_eq!(w.agent_sessions[1].task.as_deref(), Some("a"));
    w.edit(Edit::SetTaskOutcome {
        seen: None,
        id: "a".into(),
        outcome: Outcome::Completed,
    })
    .unwrap();
    assert_eq!(w.terminals[0].state, ProcessState::Running);
    assert_eq!(w.agent_sessions[1].outcome, Outcome::Open);
    w.terminals[0].state = ProcessState::Exited;
    assert!(w.task_terminal("a").is_none());
}
#[test]
fn task_edit_preserves_individual_criteria_and_tab_title_tracks_ownership() {
    let mut w = workspace();
    tab(&mut w, "tab", "a", "a");
    w.observe_agent(observation("one")).unwrap();
    let mut task = w.tasks[0].clone();
    task.title = "新的待办名".into();
    task.acceptance.push(AcceptanceCriterion {
        id: "criterion".into(),
        description: "退出码为零".into(),
        evidence: vec![evidence("test", Some(0))],
    });
    w.edit(Edit::UpdateTask(task.clone())).unwrap();
    assert_eq!(
        w.tab_title(&w.tabs[0], &Default::default(), None),
        "新的待办名"
    );
    assert_eq!(w.tasks[0].acceptance, task.acceptance);
    w.edit(Edit::AssignTask {
        id: "one".into(),
        task: None,
    })
    .unwrap();
    assert_eq!(w.tab_title(&w.tabs[0], &Default::default(), None), "Codex");
    assert!(
        w.edit(Edit::SetTaskOutcome {
            seen: None,
            id: "a".into(),
            outcome: Outcome::Deferred {
                reason: "".into(),
                until: 1
            }
        })
        .is_err()
    );
}
#[test]
fn update_space_renames_and_moves_only_an_existing_space() {
    let mut w = workspace();
    let id = w.spaces[0].id.clone();
    w.edit(Edit::UpdateSpace(Space {
        id: id.clone(),
        name: "新名字".into(),
        cwd: "/tmp".into(),
    }))
    .unwrap();
    assert_eq!(
        (w.spaces[0].name.as_str(), w.spaces[0].cwd.as_str()),
        ("新名字", "/tmp")
    );
    assert!(
        w.edit(Edit::UpdateSpace(Space {
            id: "missing".into(),
            name: "x".into(),
            cwd: "/tmp".into(),
        }))
        .is_err()
    );
    assert!(
        w.edit(Edit::UpdateSpace(Space {
            id,
            name: " ".into(),
            cwd: "/tmp".into(),
        }))
        .is_err()
    );
}
#[test]
fn tab_title_prefers_given_name_then_contents_then_program() {
    let mut w = workspace();
    for (tab, terminal) in [("first", "other"), ("second", "spare")] {
        w.terminals.push(Terminal {
            id: terminal.into(),
            ..w.terminals[0].clone()
        });
        w.edit(Edit::CreateTab {
            tab: Tab {
                id: tab.into(),
                space: "a".into(),
                default_task: None,
                title: String::new(),
                layout: Some(Layout::Leaf(terminal.into())),
            },
        })
        .unwrap();
    }
    tab(&mut w, "tab", "a", "a");
    w.observe_agent(observation("one")).unwrap();
    w.edit(Edit::AssignTask {
        id: "one".into(),
        task: None,
    })
    .unwrap();
    let mut titles = std::collections::HashMap::new();
    let title = |w: &Workspace, titles: &std::collections::HashMap<String, String>, i: usize| {
        w.tab_title(&w.tabs[i], titles, None)
    };
    // Never a bare position: the program, numbered only among same-named tabs.
    assert_eq!(title(&w, &titles, 0), "sh");
    assert_eq!(title(&w, &titles, 1), "sh 2");
    assert_eq!(title(&w, &titles, 2), "Codex");
    titles.insert("pty".to_owned(), "~/code — zsh".to_owned());
    assert_eq!(title(&w, &titles, 2), "~/code — zsh");
    let session = w.agent_sessions[0].id.clone();
    w.content.insert(
        session,
        crate::content::Content {
            title: "评估接入 Kling v4 ESR".into(),
            ..Default::default()
        },
    );
    assert_eq!(title(&w, &titles, 2), "评估接入 Kling v4 ESR");
    // A name pinned on a tab (saved before names had one owner) moves onto the
    // conversation the tab holds alone; both read that one name from then on.
    w.tabs[2].title = "kling".into();
    w.adopt_tab_names();
    assert!(w.tabs[2].title.is_empty());
    assert_eq!(title(&w, &titles, 2), "kling");
    let s = w.agent_sessions[0].clone();
    assert_eq!(
        w.session_naming(&s),
        Some(("kling".into(), NameSource::User))
    );
    // Renaming the conversation renames its tab: one owner, no order to disagree on.
    w.edit(Edit::RenameSession {
        id: s.id.clone(),
        name: "接入 Kling".into(),
    })
    .unwrap();
    assert_eq!(title(&w, &titles, 2), "接入 Kling");
    // The program title only names a tab whose conversation has no name at all.
    w.agent_sessions[0].name.clear();
    w.content.clear();
    titles.insert("spare".to_owned(), "vim".to_owned());
    assert_eq!(title(&w, &titles, 1), "vim");
    titles.insert("other".to_owned(), "htop".to_owned());
    assert_eq!(title(&w, &titles, 0), "htop");
}
#[test]
fn agent_start_commands_need_no_daemon_flag_and_quote_resume_ids() {
    assert_eq!(Agent::Claude.command(), "claude");
    assert_eq!(Agent::Codex.command(), "codex");
    assert_eq!(
        Agent::Codex.resume_command("a'b", &[]),
        format!("codex resume {}", shell_quote("a'b"))
    );
    assert_eq!(
        Agent::Claude.resume_command("safe", &[]),
        "claude --resume 'safe'"
    );
    assert_eq!(Agent::Grok.command(), "grok");
    assert_eq!(Agent::Grok.resume_command("id", &[]), "grok --resume 'id'");
    for agent in Agent::all() {
        assert_eq!(Agent::from_executable(agent.executable()), Some(agent));
    }
    assert_eq!(Agent::from_executable("node"), None);
}
#[test]
fn sequential_conversations_keep_independent_tasks_and_outcomes() {
    let mut w = workspace();
    tab(&mut w, "tab", "a", "a");
    w.observe_agent(observation("first")).unwrap();
    w.edit(Edit::SetOutcome {
        seen: None,
        id: "first".into(),
        outcome: Outcome::Completed,
    })
    .unwrap();
    w.observe_agent(observation("second")).unwrap();
    w.edit(Edit::AssignTask {
        id: "second".into(),
        task: Some("b".into()),
    })
    .unwrap();
    assert_eq!(w.terminals.len(), 1);
    assert_eq!(w.agent_sessions.len(), 2);
    assert_eq!(w.agent_sessions[0].outcome, Outcome::Completed);
    assert_eq!(w.agent_sessions[0].task.as_deref(), Some("a"));
    assert_eq!(w.current_agent("pty").unwrap().cli_session_id, "second");
    assert_eq!(w.current_agent("pty").unwrap().task.as_deref(), Some("b"));
}
#[test]
fn cross_space_placement_does_not_reassign_work_and_close_removes_the_tab() {
    let mut w = workspace();
    tab(&mut w, "old", "a", "a");
    w.observe_agent(observation("first")).unwrap();
    tab(&mut w, "new", "b", "b");
    assert!(
        w.tabs.iter().all(|t| t.id != "old"),
        "moving its only pane away closes the old tab"
    );
    assert_eq!(w.tab_terminals("new"), ["pty"]);
    assert_eq!(w.agent_sessions[0].task.as_deref(), Some("a"));
    assert_eq!(w.terminals[0].space, "a");
    w.edit(Edit::CloseTab {
        id: "new".into(),
        expect: None,
    })
    .unwrap();
    assert!(w.tabs.is_empty());
    assert!(w.terminal_tab("pty").is_none());
    assert!(
        w.edit(Edit::CloseTab {
            id: "new".into(),
            expect: None
        })
        .is_err()
    );
}
#[test]
fn closing_the_last_pane_closes_its_tab() {
    let mut w = workspace();
    w.terminals.push(Terminal {
        id: "second".into(),
        ..w.terminals[0].clone()
    });
    w.edit(Edit::CreateTab {
        tab: Tab {
            id: "t".into(),
            space: "a".into(),
            default_task: None,
            title: String::new(),
            layout: Some(Layout::Split {
                axis: Axis::Right,
                ratio: 5000,
                first: Box::new(Layout::Leaf("pty".into())),
                second: Box::new(Layout::Leaf("second".into())),
            }),
        },
    })
    .unwrap();
    w.edit(Edit::ClosePane {
        terminal: "second".into(),
    })
    .unwrap();
    assert_eq!(w.tab_terminals("t"), ["pty"]);
    w.edit(Edit::ClosePane {
        terminal: "pty".into(),
    })
    .unwrap();
    assert!(w.tabs.is_empty());
}
#[test]
fn a_split_lands_beside_its_target_on_the_side_and_share_asked() {
    let mut w = workspace();
    for id in ["second", "third"] {
        w.terminals.push(Terminal {
            id: id.into(),
            ..w.terminals[0].clone()
        });
    }
    w.edit(Edit::CreateTab {
        tab: Tab {
            id: "t".into(),
            space: "a".into(),
            default_task: None,
            title: String::new(),
            layout: Some(Layout::Leaf("pty".into())),
        },
    })
    .unwrap();
    w.edit(Edit::SplitPane {
        target: "pty".into(),
        terminal: "second".into(),
        axis: Axis::Right,
        before: false,
        share: 3000,
        forced: false,
    })
    .unwrap();
    w.edit(Edit::SplitPane {
        target: "second".into(),
        terminal: "third".into(),
        axis: Axis::Down,
        before: true,
        share: 2500,
        forced: false,
    })
    .unwrap();
    let leaf = |id: &str| Box::new(Layout::Leaf(id.into()));
    assert_eq!(
        w.tabs[0].layout,
        Some(Layout::Split {
            axis: Axis::Right,
            ratio: 7000,
            first: leaf("pty"),
            second: Box::new(Layout::Split {
                axis: Axis::Down,
                ratio: 2500,
                first: leaf("third"),
                second: leaf("second"),
            }),
        })
    );
    let again = Edit::SplitPane {
        target: "pty".into(),
        terminal: "third".into(),
        axis: Axis::Right,
        before: false,
        share: 5000,
        forced: false,
    };
    assert!(
        w.edit(again).is_err(),
        "a placed terminal is not placed twice"
    );
    let missing = Edit::SplitPane {
        target: "gone".into(),
        terminal: "pty".into(),
        axis: Axis::Right,
        before: false,
        share: 5000,
        forced: false,
    };
    assert!(w.edit(missing).is_err());
}
#[test]
fn log_refresh_never_reopens_or_reassigns_an_existing_conversation() {
    let mut w = workspace();
    tab(&mut w, "t", "a", "a");
    w.observe_agent(observation("first")).unwrap();
    w.edit(Edit::AssignTask {
        id: "first".into(),
        task: None,
    })
    .unwrap();
    w.edit(Edit::SetOutcome {
        seen: None,
        id: "first".into(),
        outcome: Outcome::Abandoned,
    })
    .unwrap();
    let mut unbound = observation("first");
    unbound.terminal = None;
    unbound.current = false;
    w.observe_agent(unbound).unwrap();
    assert_eq!(w.agent_sessions.len(), 1);
    assert_eq!(w.agent_sessions[0].task, None);
    assert_eq!(w.agent_sessions[0].outcome, Outcome::Abandoned);
}
fn evidence(kind: &str, exit_code: Option<i32>) -> CriterionEvidence {
    CriterionEvidence {
        session: "s".into(),
        record: crate::content::Evidence {
            kind: kind.into(),
            summary: format!("{kind} {exit_code:?}"),
            exit_code,
            at: 42,
            source: Default::default(),
        },
    }
}
#[test]
fn criteria_from_older_clients_read_as_no_evidence() {
    for old in [r#","evidence":null"#, r#","evidence":"test:42""#, ""] {
        let text = format!(r#"{{"id":"c","description":"d"{old}}}"#);
        let c: AcceptanceCriterion = serde_json::from_str(&text).unwrap();
        assert!(c.evidence.is_empty());
    }
}
#[test]
fn criteria_take_only_mechanical_evidence_and_merge_duplicates() {
    let mut w = workspace();
    let mut task = w.tasks[0].clone();
    let criterion = |id: &str, text: &str, evidence| AcceptanceCriterion {
        id: id.into(),
        description: text.into(),
        evidence,
    };
    // A plain command or an Agent's error line is not something a criterion can hold.
    task.acceptance = vec![criterion("a", "跑通", vec![evidence("command", Some(0))])];
    assert!(w.edit(Edit::UpdateTask(task.clone())).is_err());
    let merged = merge_criteria(vec![
        criterion("a", " 标签名保留 ", vec![evidence("file", Some(0))]),
        criterion("b", "", vec![]),
        criterion("c", "测试通过", vec![]),
        criterion(
            "d",
            "标签名保留",
            vec![evidence("commit", Some(0)), evidence("file", Some(0))],
        ),
    ]);
    assert_eq!(
        merged
            .iter()
            .map(|c| (c.id.as_str(), c.description.as_str(), c.evidence.len()))
            .collect::<Vec<_>>(),
        [("a", "标签名保留", 2), ("c", "测试通过", 0)]
    );
    task.acceptance = merged;
    w.edit(Edit::UpdateTask(task)).unwrap();
    assert_eq!(w.tasks[0].proven(), 1);
}
#[test]
fn task_prompt_and_per_criterion_evidence_are_independent() {
    let mut w = workspace();
    w.tasks[0].acceptance = vec![
        AcceptanceCriterion {
            id: "one".into(),
            description: "测试通过".into(),
            evidence: vec![evidence("test", Some(0))],
        },
        AcceptanceCriterion {
            id: "two".into(),
            description: "原生输入正确".into(),
            evidence: vec![evidence("test", Some(101))],
        },
    ];
    assert!(
        w.tasks[0]
            .prompt()
            .ends_with("验收标准：\n- 测试通过\n- 原生输入正确")
    );
    let mut bare = w.tasks[0].clone();
    bare.description = " ".into();
    bare.acceptance.clear();
    assert_eq!(
        bare.prompt(),
        bare.title.trim(),
        "blank parts add no headings"
    );
    assert_eq!(
        w.tasks[0].proven(),
        1,
        "a failed test is an attempt, not evidence"
    );
    assert!(w.tasks[0].acceptance[0].proven());
    assert_eq!(
        shell_quote("a'$(touch /tmp/no)\nb"),
        "'a'\\''$(touch /tmp/no)\nb'"
    );
}
#[test]
fn session_names_prefer_the_user_then_the_task_then_the_cli_then_the_first_ask() {
    let mut w = Workspace::default();
    w.observe_agent(AgentSession {
        terminal: None,
        current: false,
        ..observation("one")
    })
    .unwrap();
    let id = w.agent_sessions[0].id.clone();
    let session = |w: &Workspace| w.agent_sessions[0].clone();
    assert_eq!(w.session_name(&session(&w)), None);
    assert_eq!(w.session_title(&session(&w)), "codex · a");
    w.content.insert(
        id.clone(),
        crate::content::Content {
            first_ask_known: true,
            title: "跳至主要内容 你帮我看看 这个页面讲了什么".into(),
            asks: vec![crate::content::Quote {
                text: "跳至主要内容\n你帮我看看 这个页面讲了什么".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    assert_eq!(
        w.session_name(&session(&w)).as_deref(),
        Some("这个页面讲了什么")
    );
    w.content.get_mut(&id).unwrap().name = "总结页面".into();
    assert_eq!(
        w.session_naming(&session(&w)),
        Some(("总结页面".into(), NameSource::Agent))
    );
    // A task it was started for or given outranks what the CLI generated.
    w.tasks.push(Task {
        id: "t".into(),
        space: "s".into(),
        title: "修复登录跳转".into(),
        description: String::new(),
        acceptance: vec![],
        agent: Agent::Codex,
    });
    w.agent_sessions[0].task = Some("t".into());
    assert_eq!(
        w.session_naming(&session(&w)),
        Some(("修复登录跳转".into(), NameSource::Task))
    );
    w.edit(Edit::RenameSession {
        id: id.clone(),
        name: "我的名字".into(),
    })
    .unwrap();
    // Seeing the conversation again keeps what the user called it.
    w.observe_agent(AgentSession {
        terminal: None,
        current: false,
        ..observation("one")
    })
    .unwrap();
    assert_eq!(
        w.session_naming(&session(&w)),
        Some(("我的名字".into(), NameSource::User))
    );
    assert_eq!(w.session_title(&session(&w)), "我的名字");
    assert!(
        w.edit(Edit::RenameSession {
            id: "missing".into(),
            name: "x".into(),
        })
        .is_err()
    );
}
