//! A pane split into a to-do's tab is not the to-do's unless started for it.
use super::*;
use crate::agent::Agent;

fn terminal(id: &str) -> Terminal {
    Terminal {
        id: id.into(),
        space: "s".into(),
        argv: vec!["/bin/sh".into()],
        cwd: "/tmp/s".into(),
        pid: Some(42),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    }
}

fn agent(id: &str, terminal: &str) -> AgentSession {
    AgentSession {
        id: id.into(),
        runtime: Agent::Codex,
        cli_session_id: id.into(),
        terminal: Some(terminal.into()),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp/s".into(),
        transcript: format!("/logs/{id}.jsonl"),
        binding: None,
        name: String::new(),
    }
}

fn split(w: &mut Workspace, terminal: &str) {
    w.edit(Edit::SplitPane {
        target: "first".into(),
        terminal: terminal.into(),
        axis: crate::layout::Axis::Right,
        before: false,
        share: 5000,
        forced: true,
    })
    .unwrap();
}

/// The reported case of 11-18: conversations about a skyfable issue and a gateway error,
/// split into the 「搜索工具」 tab, were counted as that to-do's and named by it.
#[test]
fn a_split_takes_the_tabs_todo_only_when_started_for_it() {
    let mut w = Workspace::default();
    w.spaces.push(Space {
        id: "s".into(),
        name: "s".into(),
        cwd: "/tmp/s".into(),
    });
    w.tasks.push(Task {
        id: "t".into(),
        space: "s".into(),
        title: "搜索工具".into(),
        description: String::new(),
        acceptance: vec![],
        agent: Agent::Codex,
    });
    for id in ["first", "other", "again", "relaunch"] {
        w.terminals.push(terminal(id));
    }
    w.edit(Edit::CreateTab {
        tab: Tab {
            id: "tab".into(),
            space: "s".into(),
            default_task: Some("t".into()),
            title: String::new(),
            layout: Some(crate::layout::Layout::Leaf("first".into())),
        },
    })
    .unwrap();
    // The pane the tab was opened with is the to-do's.
    w.observe_agent(agent("in-first", "first")).unwrap();
    // A split is other work.
    split(&mut w, "other");
    w.observe_agent(agent("in-other", "other")).unwrap();
    // 「再开一个」 for the to-do says so before the pane is placed.
    w.edit(Edit::SetPendingTask {
        terminal: "again".into(),
        task: Some("t".into()),
    })
    .unwrap();
    split(&mut w, "again");
    w.observe_agent(agent("in-again", "again")).unwrap();
    let task = |id: &str| {
        (w.agent_sessions.iter())
            .find(|s| s.id == id)
            .and_then(|s| s.task.clone())
    };
    assert_eq!(task("in-first").as_deref(), Some("t"));
    assert_eq!(task("in-other"), None);
    assert_eq!(task("in-again").as_deref(), Some("t"));
    // A split into a tab of no to-do leaves nothing behind.
    w.tabs[0].default_task = None;
    split(&mut w, "relaunch");
    assert!(!w.pending_tasks.contains_key("relaunch"));
}
