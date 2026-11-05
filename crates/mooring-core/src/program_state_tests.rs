use super::*;
use crate::content::Background;
use crate::model::{Agent, AgentSession, Outcome, ProcessState, Terminal, Workspace};
use crate::status;

fn report(state: &str) -> ProgramReport {
    ProgramReport {
        state: state.into(),
        kind: None,
        progress: None,
        app: "script".into(),
        title: String::new(),
        msg: String::new(),
        since: 5,
    }
}
fn content(turn: Turn) -> Content {
    Content {
        updated: 1,
        turn_started: 1,
        turn,
        asks: vec![crate::content::Quote::default()],
        ..Default::default()
    }
}
/// The state of a conversation in a running terminal after its program said `state`.
fn bound(mut c: Content, state: &str) -> Status {
    apply(&mut c, &report(state));
    let mut w = Workspace::default();
    w.terminals.push(Terminal {
        id: "t".into(),
        space: String::new(),
        argv: vec![],
        cwd: "/tmp".into(),
        pid: None,
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    w.content.insert("s".into(), c);
    let s = AgentSession {
        id: "s".into(),
        runtime: Agent::Codex,
        cli_session_id: "c".into(),
        terminal: Some("t".into()),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp".into(),
        transcript: "f".into(),
        binding: None,
        name: String::new(),
    };
    status::of(&w, &s, 1)
}

#[test]
fn unbound_idle_is_paused() {
    assert_eq!(of_program("idle"), Status::Paused);
    assert_eq!(of_program("done"), Status::Review);
}

#[test]
fn bound_idle_reads_as_unbound_idle() {
    assert_eq!(bound(content(Turn::Running), "idle"), of_program("idle"));
    assert_eq!(bound(content(Turn::Waiting), "idle"), of_program("idle"));
    assert_eq!(bound(content(Turn::Running), "done"), of_program("done"));
}

#[test]
fn idle_keeps_a_background_in_flight() {
    let mut c = content(Turn::Completed);
    c.background = Some(Background {
        shells: 1,
        ..Default::default()
    });
    assert_eq!(bound(c, "idle"), Status::Background);
}

#[test]
fn idle_before_any_turn_stays_fresh() {
    assert_eq!(bound(Content::default(), "idle"), Status::Fresh);
}
