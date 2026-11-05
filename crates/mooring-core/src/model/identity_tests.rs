use super::*;
#[test]
fn a_broad_space_hands_a_project_folder_to_its_own_space() {
    let space = |id: &str, cwd: &str| Space {
        id: id.into(),
        name: id.into(),
        cwd: cwd.into(),
    };
    let spaces = [
        space("code", "/u/code"),
        space("work", "/u/code/work"),
        space("her", "/u/code/work/infra/her"),
        space("other", "/u/elsewhere"),
        space("folderless", ""),
    ];
    let at = |within, cwd| narrower_space(&spaces, within, cwd).map(|s| s.id.as_str());
    // The deepest space holding the folder wins, from any broader space.
    assert_eq!(at("code", "/u/code/work/infra/her/src"), Some("her"));
    assert_eq!(at("work", "/u/code/work/infra/her"), Some("her"));
    assert_eq!(at("code", "/u/code/work/x"), Some("work"));
    // At the space's own folder, beside it, or outside it nothing moves.
    assert_eq!(at("code", "/u/code"), None);
    assert_eq!(at("her", "/u/code/work/infra/her/src"), None);
    assert_eq!(at("her", "/u/elsewhere/a"), None);
    assert_eq!(at("code", "/u/code-old/her"), None);
    // A space without a folder was picked on purpose.
    assert_eq!(at("folderless", "/u/code/work/infra/her"), None);
    assert_eq!(at("missing", "/u/code/work"), None);
}
#[test]
fn one_cli_identity_cannot_be_current_in_two_terminals() {
    let mut w = Workspace::default();
    for id in ["one", "two"] {
        w.terminals.push(Terminal {
            id: id.into(),
            space: "s".into(),
            argv: vec![],
            cwd: "/tmp".into(),
            pid: Some(1),
            state: ProcessState::Running,
            exit_code: None,
            error: None,
        });
    }
    let mut session = AgentSession {
        id: "a".into(),
        runtime: Agent::Codex,
        cli_session_id: "cli".into(),
        terminal: Some("one".into()),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp".into(),
        transcript: "/log".into(),
        binding: None,
        name: String::new(),
    };
    w.observe_agent(session.clone()).unwrap();
    session.terminal = Some("two".into());
    assert!(w.observe_agent(session.clone()).is_err());
    assert_eq!(w.agent_sessions[0].terminal.as_deref(), Some("one"));
    w.agent_sessions[0].current = false;
    w.observe_agent(session).unwrap();
    assert_eq!(w.agent_sessions.len(), 1);
    assert_eq!(w.agent_sessions[0].terminal.as_deref(), Some("two"));
}
