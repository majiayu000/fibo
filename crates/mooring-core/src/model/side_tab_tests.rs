use super::*;

fn holding_one(tab_id: &str, title: &str) -> Workspace {
    let mut w = Workspace::default();
    w.spaces.push(Space {
        id: "a".into(),
        name: "a".into(),
        cwd: "/tmp/a".into(),
    });
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
    w.edit(Edit::CreateTab {
        tab: Tab {
            id: tab_id.into(),
            space: "a".into(),
            default_task: None,
            title: title.into(),
            layout: Some(Layout::Leaf("pty".into())),
        },
    })
    .unwrap();
    w.observe_agent(AgentSession {
        id: "s".into(),
        runtime: Agent::Codex,
        cli_session_id: "s".into(),
        terminal: Some("pty".into()),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp/a".into(),
        transcript: "/logs/s.jsonl".into(),
        binding: None,
        name: String::new(),
    })
    .unwrap();
    w
}

/// Undoing `is_side` in `adopt_tab_name` names every `fibo run` session after its tab.
#[test]
fn a_side_tab_keeps_its_default_and_the_conversation_keeps_no_name() {
    let w = holding_one("side~owner", "Codex 的旁屏");
    assert_eq!(w.agent_sessions[0].name, "");
    assert_eq!(w.tabs[0].title, "Codex 的旁屏");
    // An ordinary tab still hands a name it was given to the one conversation it holds.
    let w = holding_one("tab", "部署");
    assert_eq!(w.agent_sessions[0].name, "部署");
    assert!(w.tabs[0].title.is_empty());
}

/// A workspace saved by 0.1.2 has side-tab defaults stored as conversation names.
#[test]
fn loading_clears_side_tab_defaults_older_builds_stored_as_names() {
    for (stored, kept) in [
        ("Claude Code 的旁屏", ""),
        ("Claude Code 的旁屏 2", ""),
        ("终端的旁屏", ""),
        ("Codex side", ""),
        ("Terminal side 3", ""),
        ("我的旁屏", "我的旁屏"),
        ("部署", "部署"),
    ] {
        let mut w = holding_one("side~owner", "");
        w.agent_sessions[0].name = stored.into();
        w.adopt_tab_names();
        assert_eq!(w.agent_sessions[0].name, kept, "{stored}");
    }
}
