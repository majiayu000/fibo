use super::super::tests::{fixture, session};
use super::*;

/// An ended conversation you named and marked done, in a terminal that ended too.
fn stored(host: &mut Host) {
    let mut next = host.workspace.clone();
    next.spaces.push(Space {
        id: "sp".into(),
        name: "x".into(),
        cwd: "/tmp".into(),
    });
    next.terminals.push(Terminal {
        id: "t".into(),
        space: "sp".into(),
        argv: vec!["/bin/sh".into()],
        cwd: "/tmp".into(),
        pid: None,
        state: ProcessState::Exited,
        exit_code: Some(0),
        error: None,
    });
    next.agent_sessions.push(AgentSession {
        name: "我起的名".into(),
        outcome: Outcome::Completed,
        ..session("old", 1, Some("t"), false)
    });
    host.commit(next).unwrap();
}

#[test]
fn an_archived_conversation_comes_back_as_itself() {
    let (mut host, root) = fixture();
    stored(&mut host);
    host.archive_idle().unwrap();
    assert!(host.workspace.agent_sessions.is_empty());
    assert!(host.workspace.terminals.is_empty());
    // Its log moved: recognition sees the same CLI conversation under a new row.
    let mut next = host.workspace.clone();
    next.observe_agent(AgentSession {
        id: "new".into(),
        cli_session_id: "old".into(),
        ..session("new", fibo_core::content::now(), None, false)
    })
    .unwrap();
    host.commit(next).unwrap();
    let back = &host.workspace.agent_sessions;
    assert_eq!(back.len(), 1);
    assert_eq!(
        (back[0].id.as_str(), back[0].name.as_str(), &back[0].outcome),
        ("old", "我起的名", &Outcome::Completed)
    );
    assert_eq!(back[0].terminal.as_deref(), Some("t"));
    assert!(host.workspace.terminals.iter().any(|t| t.id == "t"));
    // The store kept one row, and a host started on it sees the same.
    let (_, loaded) = Store::open(&root).unwrap();
    assert_eq!(loaded.agent_sessions.len(), 1);
    assert_eq!(loaded.agent_sessions[0].name, "我起的名");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn archiving_writes_no_row_away() {
    let (mut host, root) = fixture();
    stored(&mut host);
    host.archive_idle().unwrap();
    let (_, loaded) = Store::open(&root).unwrap();
    assert_eq!(loaded.agent_sessions.len(), 1);
    assert_eq!(loaded.terminals.len(), 1);
    fs::remove_dir_all(root).unwrap();
}
