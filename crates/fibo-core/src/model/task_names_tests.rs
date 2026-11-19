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

fn said(w: &mut Workspace, id: &str, cli_name: &str, first: &str) {
    w.content.insert(
        id.into(),
        std::sync::Arc::new(crate::content::Content {
            name: cli_name.into(),
            first_ask_known: !first.is_empty(),
            asks: (!first.is_empty())
                .then(|| crate::content::Quote {
                    text: first.into(),
                    ..Default::default()
                })
                .into_iter()
                .collect(),
            ..Default::default()
        }),
    );
}

/// The reported case of 11-18: four panes of the 「agent纵览」 tab were rows called
/// 「agent纵览 · Codex 3」 and the like, while each CLI had named its own; and a
/// conversation whose first ask is only 「帮我继续」 is told by its CLI's name.
#[test]
fn conversations_of_one_task_are_called_by_their_own_names() {
    let mut w = with_tasks(vec![
        on_task("a", Agent::Codex, "u"),
        on_task("b", Agent::Claude, "u"),
        on_task("c", Agent::Codex, "u"),
        on_task("d", Agent::Codex, "u"),
    ]);
    said(&mut w, "a", "审视项目进展与当前目标", "你帮我看看这个库");
    said(&mut w, "b", "SkyFable 计费剩余工作", "帮我继续");
    said(&mut w, "c", "", "你帮我看看 我的 lark 聊天记录");
    assert_eq!(
        titles(&w),
        [
            "审视项目进展与当前目标",
            "SkyFable 计费剩余工作",
            "我的 lark 聊天记录",
            // Nothing said yet: the task's title, alone of its kind.
            "agent纵览"
        ]
    );
    let sources: Vec<_> = (w.agent_sessions.iter())
        .map(|s| w.session_naming(s).map(|(_, source)| source))
        .collect();
    assert_eq!(
        sources,
        [
            Some(NameSource::Agent),
            Some(NameSource::Agent),
            Some(NameSource::FirstAsk),
            Some(NameSource::Task)
        ]
    );
}

/// Two of one task whose own names are the same still say which is which; the same name
/// on another task, or a conversation of no task, is not compared.
#[test]
fn the_same_own_name_twice_on_one_task_says_the_agent() {
    let mut w = with_tasks(vec![
        on_task("a", Agent::Claude, "t"),
        on_task("b", Agent::Claude, "t"),
        on_task("c", Agent::Codex, "t"),
        on_task("d", Agent::Codex, "u"),
    ]);
    for id in ["a", "b", "c", "d"] {
        said(&mut w, id, "", "帮我继续");
    }
    assert_eq!(
        titles(&w),
        [
            "帮我继续 · Claude Code",
            "帮我继续 · Claude Code 2",
            "帮我继续 · Codex",
            "帮我继续"
        ]
    );
}

/// A narrow row shortens the words and keeps what says which is which: the title comes
/// in two parts that join to it, the tail empty when nothing needs telling apart.
#[test]
fn the_title_splits_before_what_tells_two_of_one_task_apart() {
    let mut w = with_tasks(vec![
        on_task("a", Agent::Codex, "t"),
        on_task("b", Agent::Codex, "t"),
        on_task("c", Agent::Claude, "u"),
    ]);
    w.agent_sessions[2].name = "我起的名字".into();
    let parts: Vec<_> = (w.agent_sessions.iter())
        .map(|s| w.session_title_parts(s))
        .collect();
    assert_eq!(
        parts,
        [
            ("fibo纵览".to_owned(), " · Codex".to_owned()),
            ("fibo纵览".to_owned(), " · Codex 2".to_owned()),
            ("我起的名字".to_owned(), String::new()),
        ]
    );
    for (s, (words, tail)) in w.agent_sessions.iter().zip(parts) {
        assert_eq!(w.session_title(s), words + &tail);
    }
}
