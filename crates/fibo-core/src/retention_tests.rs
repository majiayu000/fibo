use super::*;
use crate::content::now;
use crate::model::{AgentSession, Outcome, ProcessState};

const DAY: u64 = 86_400;

/// Every sample conversation once its tab is closed and its process ended.
fn ended() -> Workspace {
    let mut w = crate::sample::every_state();
    w.tabs.clear();
    for t in &mut w.terminals {
        t.state = ProcessState::Exited;
    }
    w
}

#[test]
fn nothing_open_or_recent_goes() {
    let w = crate::sample::every_state();
    let (sessions, terminals) = archivable(&w, now(), &HashSet::new());
    assert!(
        sessions.is_empty() && terminals.is_empty(),
        "{sessions:?} {terminals:?}"
    );
    // Closed and ended, but moved less than 30 days ago: still here.
    let (sessions, _) = archivable(&ended(), now() + 29 * DAY, &HashSet::new());
    assert!(sessions.is_empty(), "{sessions:?}");
}

#[test]
fn after_thirty_days_only_what_still_holds_you_stays() {
    let w = ended();
    let (sessions, terminals) = archivable(&w, now() + 31 * DAY, &HashSet::new());
    for s in &w.agent_sessions {
        let stays = matches!(s.outcome, Outcome::Deferred { .. }) || s.task.is_some();
        assert_eq!(!sessions.contains(&s.id), stays, "{}", s.id);
        // A terminal never goes before its conversation.
        let terminal = s.terminal.as_ref().unwrap();
        assert_eq!(!terminals.contains(terminal), stays, "{terminal}");
    }
}

#[test]
fn a_terminal_still_open_or_awaited_keeps_its_conversation() {
    let mut w = ended();
    w.terminals
        .iter_mut()
        .find(|t| t.id == "pty-waiting")
        .unwrap()
        .state = ProcessState::Running;
    let held = HashSet::from(["pty-error".to_owned()]);
    w.pinned_terminals = Some(vec!["pty-renamed".into()]);
    let (sessions, terminals) = archivable(&w, now() + 60 * DAY, &held);
    for id in ["waiting", "error", "renamed"] {
        assert!(!sessions.contains(id), "{id}");
        assert!(!terminals.contains(&format!("pty-{id}")), "{id}");
    }
}

#[test]
fn a_closed_todo_no_longer_holds_its_conversation() {
    let mut w = ended();
    w.task_outcomes.insert("task".into(), Outcome::Completed);
    let (sessions, _) = archivable(&w, now() + 31 * DAY, &HashSet::new());
    assert!(sessions.contains("tasked"));
}

#[test]
fn archive_leaves_out_rows_and_content() {
    let mut w = ended();
    assert!(archive(&mut w, now() + 31 * DAY, &HashSet::new()));
    assert!(
        w.agent_sessions
            .iter()
            .all(|s| w.content.contains_key(&s.id) || s.id == "fresh")
    );
    assert!(!w.content.contains_key("closed"));
    assert!(!archive(&mut w, now() + 31 * DAY, &HashSet::new()));
}

#[test]
fn a_conversation_back_from_the_archive_keeps_what_you_gave_it() {
    let mut w = ended();
    let archived = w
        .agent_sessions
        .iter()
        .find(|s| s.id == "renamed")
        .cloned()
        .unwrap();
    let terminal = w.terminals.iter().find(|t| t.id == "pty-renamed").cloned();
    archive(&mut w, now() + 31 * DAY, &HashSet::new());
    assert!(!w.agent_sessions.iter().any(|s| s.id == "renamed"));
    // Its log moved: recognition made a new row for the same CLI conversation.
    let made = AgentSession {
        id: "new-id".into(),
        terminal: None,
        current: false,
        name: String::new(),
        outcome: Outcome::Open,
        updated: now() + 31 * DAY,
        ..archived.clone()
    };
    w.observe_agent(made).unwrap();
    w.content
        .insert("new-id".into(), w.content.values().next().unwrap().clone());
    readopt(&mut w, "new-id", archived.clone(), terminal).unwrap();
    let back = w
        .agent_sessions
        .iter()
        .filter(|s| s.cli_session_id == archived.cli_session_id)
        .collect::<Vec<_>>();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].id, "renamed");
    assert_eq!(back[0].name, archived.name);
    assert_eq!(back[0].terminal.as_deref(), Some("pty-renamed"));
    assert_eq!(back[0].updated, now() + 31 * DAY);
    assert!(w.content.contains_key("renamed") && !w.content.contains_key("new-id"));
    assert!(w.terminals.iter().any(|t| t.id == "pty-renamed"));
}

#[test]
fn a_terminal_a_restart_interrupted_stays_to_be_reopened() {
    let mut w = ended();
    w.agent_sessions.clear();
    w.lost_agents.insert("pty-waiting".into(), None);
    let (_, terminals) = archivable(&w, now(), &HashSet::new());
    assert!(!terminals.contains("pty-waiting"));
    assert!(terminals.contains("pty-closed"));
}
