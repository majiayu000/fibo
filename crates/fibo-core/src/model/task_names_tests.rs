//! Conversations sharing a to-do are told apart by name (`task_names`).
use super::*;
use crate::agent::Agent;

fn on_task(id: &str, runtime: Agent, task: &str) -> AgentSession {
    AgentSession {
        id: id.into(),
        runtime,
        cli_session_id: id.into(),
        terminal: Some(format!("pty-{id}")),
        current: true,
        task: Some(task.into()),
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp/fibo".into(),
        transcript: format!("/logs/{id}.jsonl"),
        binding: None,
        name: String::new(),
    }
}

fn with_tasks(sessions: Vec<AgentSession>) -> Workspace {
    let mut w = Workspace::default();
    for (id, title) in [("t", "fibo纵览"), ("u", "agent纵览")] {
        w.tasks.push(Task {
            id: id.into(),
            space: "s".into(),
            title: title.into(),
            description: String::new(),
            acceptance: vec![],
            agent: Agent::Codex,
        });
    }
    w.agent_sessions = sessions;
    w
}

fn titles(w: &Workspace) -> Vec<String> {
    w.agent_sessions
        .iter()
        .map(|s| w.session_title(s))
        .collect()
}

/// The reported case: a Codex opened on 11-16 and a Claude split beside it on 11-17, both
/// for 「fibo纵览」, were two rows called 「fibo纵览」.
#[test]
fn two_agents_on_one_task_say_which_agent() {
    let w = with_tasks(vec![
        on_task("codex", Agent::Codex, "t"),
        on_task("claude", Agent::Claude, "t"),
        on_task("other", Agent::Codex, "u"),
    ]);
    assert_eq!(
        titles(&w),
        ["fibo纵览 · Codex", "fibo纵览 · Claude Code", "agent纵览"]
    );
    assert_eq!(
        w.session_naming(&w.agent_sessions[1])
            .map(|(_, source)| source),
        Some(NameSource::Task)
    );
}

/// Two of one Agent: the first seen keeps `· Agent`, the next ones are numbered, and a
/// conversation that has ended still takes its place (the sidebar folds it, the board
/// shows it).
#[test]
fn the_same_agent_twice_on_one_task_is_numbered_in_the_order_first_seen() {
    let mut w = with_tasks(vec![
        on_task("a", Agent::Claude, "t"),
        on_task("codex", Agent::Codex, "t"),
        on_task("b", Agent::Claude, "t"),
        on_task("c", Agent::Claude, "t"),
    ]);
    w.agent_sessions[0].current = false;
    assert_eq!(
        titles(&w),
        [
            "fibo纵览 · Claude Code",
            "fibo纵览 · Codex",
            "fibo纵览 · Claude Code 2",
            "fibo纵览 · Claude Code 3"
        ]
    );
    let names: std::collections::HashSet<_> = titles(&w).into_iter().collect();
    assert_eq!(names.len(), 4, "no two rows share a name");
}

#[test]
fn a_task_with_one_conversation_keeps_its_title() {
    let w = with_tasks(vec![
        on_task("only", Agent::Claude, "t"),
        on_task("other", Agent::Codex, "u"),
    ]);
    assert_eq!(titles(&w), ["fibo纵览", "agent纵览"]);
}

/// A name the user gave is never touched, and a conversation they named no longer shares
/// the task's title, so the one left with it goes back to the plain title. The automatic
/// name a rename field offers is the one it would have.
#[test]
fn a_name_the_user_gave_stays_and_frees_the_title() {
    let mut w = with_tasks(vec![
        on_task("codex", Agent::Codex, "t"),
        on_task("claude", Agent::Claude, "t"),
    ]);
    w.agent_sessions[0].name = "dy pr回复".into();
    assert_eq!(titles(&w), ["dy pr回复", "fibo纵览"]);
    let unnamed = AgentSession {
        name: String::new(),
        ..w.agent_sessions[0].clone()
    };
    assert_eq!(w.session_title(&unnamed), "fibo纵览 · Codex");
}
