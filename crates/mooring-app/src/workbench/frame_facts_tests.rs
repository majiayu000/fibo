use super::*;
use crate::workbench::tests::fixture;
use mooring_core::content::{Content, Turn};

/// What the window worked out by walking every session before these were remembered:
/// the facts must give the same answer for any workspace.
fn assert_same_as_walking(w: &Workspace, resumed: &HashMap<String, String>, now: u64) {
    let facts = FrameFacts::work_out(w, inputs(w, resumed, now));
    let id = |s: Option<&AgentSession>| s.map(|s| s.id.clone());
    let mut terminals: Vec<String> = w.terminals.iter().map(|t| t.id.clone()).collect();
    terminals.extend(w.agent_sessions.iter().filter_map(|s| s.terminal.clone()));
    terminals.push("no-such-terminal".into());
    for t in &terminals {
        assert_eq!(
            id(facts.current_agent(w, t)),
            id(w.current_agent(t)),
            "current {t}"
        );
        let walked = w
            .agent_sessions
            .iter()
            .filter(|s| s.terminal.as_deref() == Some(t))
            .max_by_key(|s| w.session_updated(s));
        assert_eq!(id(facts.latest(w, t)), id(walked), "latest {t}");
    }
    for tab in &w.tabs {
        let ids = w.tab_terminals(&tab.id);
        let walked: Vec<_> = w
            .agent_sessions
            .iter()
            .filter(|s| s.current && s.terminal.as_ref().is_some_and(|id| ids.contains(id)))
            .map(|s| s.id.clone())
            .collect();
        let got: Vec<_> = facts
            .current_on(w, &ids)
            .iter()
            .map(|s| s.id.clone())
            .collect();
        assert_eq!(got, walked, "tab {}", tab.id);
    }
    let walked: Vec<_> = w
        .agent_sessions
        .iter()
        .filter(|s| w.needs_you(s, now))
        .map(|s| s.id.clone())
        .collect();
    let got: Vec<_> = facts.needs_you(w).iter().map(|s| s.id.clone()).collect();
    assert_eq!(got, walked, "needs you");
    let shown = |s: &&AgentSession| sessions::session_hidden_in(w, s).is_none();
    let listed = w
        .agent_sessions
        .iter()
        .filter(shown)
        .filter(|s| sessions::session_open_in(w, resumed, s))
        .count();
    assert_eq!(facts.listed, listed, "listed");
    let glyphs = |pick: &dyn Fn(Status) -> bool| {
        w.agent_sessions
            .iter()
            .filter(shown)
            .filter(|s| s.managed() && s.outcome.needs_review(now))
            .filter(|s| pick(mooring_core::status::of(w, s, now)))
            .count()
    };
    assert_eq!(facts.working, glyphs(&|g| g.working()), "working");
    assert_eq!(facts.review, glyphs(&|g| g == Status::Review), "review");
}

fn session(id: &str, terminal: Option<&str>, current: bool, updated: u64) -> AgentSession {
    AgentSession {
        id: id.into(),
        cli_session_id: id.into(),
        terminal: terminal.map(str::to_owned),
        current,
        updated,
        outcome: Outcome::Open,
        ..fixture().agent_sessions[0].clone()
    }
}

fn shown_as(w: &mut Workspace, id: &str, turn: Turn) {
    let content = Content {
        interactive: true,
        turn,
        ..Default::default()
    };
    w.content.insert(id.into(), content);
}

#[test]
fn facts_answer_as_walking_every_session_does() {
    let mut w = fixture();
    w.agent_sessions.extend([
        // Current on the tab's last pane, yet first in workspace order; and asking.
        session("asking", Some("t3"), true, 1),
        // Two current on one terminal: the first in workspace order is the current one.
        session("first", Some("t0"), true, 5),
        session("second", Some("t0"), true, 9),
        // Running, but hidden: no count may include it.
        session("hidden", Some("t0"), true, 6),
        // A later record loses to content that was updated later.
        session("record-newer", Some("t1"), false, 7),
        session("content-newer", Some("t1"), false, 3),
        // Equal times: the last of them is the latest, as `max_by_key` picks.
        session("tie-a", Some("t2"), false, 7),
        session("tie-b", Some("t2"), false, 7),
        // On a terminal outside any tab, on none, and being resumed in a free one.
        session("outside", Some("gone"), true, 2),
        session("scanned", None, false, 4),
        session("resuming", None, false, 4),
    ]);
    shown_as(&mut w, "first", Turn::Running);
    shown_as(&mut w, "hidden", Turn::Running);
    w.content.get_mut("hidden").unwrap().interactive = false;
    shown_as(&mut w, "second", Turn::Completed);
    shown_as(&mut w, "asking", Turn::Waiting);
    shown_as(&mut w, "resuming", Turn::Completed);
    shown_as(&mut w, "scanned", Turn::Completed);
    let newer = Content {
        updated: 100,
        ..Default::default()
    };
    w.content.insert("content-newer".into(), newer);
    // Nothing is current on t2 in this workspace: a resume can take it.
    let resumed = HashMap::from([("resuming".to_owned(), "t2".to_owned())]);
    assert_same_as_walking(&w, &resumed, 1000);
    assert_same_as_walking(&w, &HashMap::new(), 1000);
    assert_same_as_walking(&Workspace::default(), &HashMap::new(), 1000);

    // The walk above is only worth something if these cases reach every count.
    let facts = FrameFacts::work_out(&w, inputs(&w, &resumed, 1000));
    let idle = FrameFacts::work_out(&w, inputs(&w, &HashMap::new(), 1000));
    assert_eq!(facts.listed, idle.listed + 1, "a resume is listed");
    assert!(
        facts.working > 0 && facts.review > 0 && !facts.needs_you.is_empty(),
        "{facts:?}"
    );
    let on_tab = facts.current_on(&w, &w.tab_terminals("tab"));
    assert_eq!(on_tab[0].id, "asking", "workspace order, not pane order");
    assert_eq!(facts.latest(&w, "t2").unwrap().id, "tie-b");
    assert_eq!(facts.latest(&w, "t1").unwrap().id, "content-newer");
    assert_eq!(facts.current_agent(&w, "t0").unwrap().id, "first");
}

#[test]
fn facts_notice_each_input() {
    let w = fixture();
    let none = HashMap::new();
    let base = inputs(&w, &none, 10);
    assert_eq!(base, inputs(&w, &none, 10));
    assert_ne!(base, inputs(&w, &none, 11), "the second");
    let mut changed = w.clone();
    changed.revision += 1;
    assert_ne!(base, inputs(&changed, &none, 10), "the revision");
    let mut changed = w.clone();
    changed.agent_sessions.clear();
    assert_ne!(base, inputs(&changed, &none, 10), "the sessions");
    let resumed = HashMap::from([("s".to_owned(), "t".to_owned())]);
    assert_ne!(base, inputs(&w, &resumed, 10), "a resume");
}
