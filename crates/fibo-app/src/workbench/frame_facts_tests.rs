use super::*;
use crate::workbench::tests::fixture;
use fibo_core::content::{Content, Turn};

#[test]
fn pane_agent_uses_process_identity_before_a_transcript_and_before_history() {
    let mut w = fixture();
    let mut recognized = HashMap::from([("t0".into(), Agent::Codex)]);
    let facts = |w: &Workspace| FrameFacts::work_out(w, inputs(w, &HashMap::new(), 100));
    assert_eq!(
        facts(&w).pane_agent(&w, &recognized, "t0"),
        Some(Agent::Codex)
    );
    assert!(
        w.current_agent("t0").is_none(),
        "a mark is not a conversation"
    );
    assert_eq!(facts(&w).pane_agent(&w, &recognized, "t3"), None);
    recognized.clear();
    assert_eq!(
        facts(&w).pane_agent(&w, &recognized, "t0"),
        None,
        "exit clears an unbound mark"
    );
    // A new CLI must not inherit the mark of an older conversation on this pane.
    recognized.insert("t1".into(), Agent::Claude);
    assert_eq!(
        facts(&w).pane_agent(&w, &recognized, "t1"),
        Some(Agent::Claude)
    );
    // Once bound, the conversation supplies its identity. Historical display is retained.
    w.agent_sessions[0].current = true;
    assert_eq!(
        facts(&w).pane_agent(&w, &recognized, "t1"),
        Some(Agent::Codex)
    );
    w.agent_sessions[0].current = false;
    recognized.clear();
    assert_eq!(
        facts(&w).pane_agent(&w, &recognized, "t1"),
        Some(Agent::Codex)
    );
}

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
            .filter(|s| pick(fibo_core::status::of(w, s, now)))
            .count()
    };
    assert_eq!(facts.working, glyphs(&|g| g.working()), "working");
    assert_eq!(facts.review, glyphs(&|g| g == Status::Review), "review");

    for space in &w.spaces {
        let listed: Vec<_> = w
            .agent_sessions
            .iter()
            .filter(|s| sessions::session_open_in(w, resumed, s))
            .filter(|s| w.session_space(s) == Some(&space.id) && shown(s))
            .collect();
        let states = || {
            listed
                .iter()
                .filter(|s| s.outcome.needs_review(now))
                .map(|s| (s.managed(), fibo_core::status::of(w, s, now)))
        };
        let waiting = states().filter(|s| *s == (true, Status::Waiting)).count();
        let errors = states().filter(|s| *s == (true, Status::Error)).count();
        let group = if errors > 0 {
            Status::Error
        } else if waiting > 0 {
            Status::Waiting
        } else {
            states()
                .map(|(_, state)| state)
                .find(|state| !matches!(state, Status::Waiting | Status::Error))
                .unwrap_or(Status::Stale)
        };
        let counts = facts.space_counts(&space.id);
        assert_eq!(
            (counts.count, counts.waiting, counts.errors, counts.group()),
            (listed.len(), waiting, errors, group),
            "space {}",
            space.id
        );
        if w.space_quiet(&space.id) {
            let mut rows: Vec<_> = w
                .agent_sessions
                .iter()
                .filter(|s| {
                    (sessions::session_open_in(w, resumed, s)
                        || w.lost_agents.values().flatten().any(|id| *id == s.id))
                        && w.session_home(s) == Some(&space.id)
                        && sessions::automation_hidden_in(w, s).is_none()
                })
                .collect();
            rows.sort_by_key(|s| {
                (
                    !status::asks_for_you(w, s, now),
                    std::cmp::Reverse(w.session_updated(s)),
                )
            });
            assert_eq!(facts.quiet_space_sessions(w, &space.id), rows);
        }
    }
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
    w.content.insert(id.into(), std::sync::Arc::new(content));
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
    w.content
        .get_mut("hidden")
        .map(std::sync::Arc::make_mut)
        .unwrap()
        .interactive = false;
    shown_as(&mut w, "second", Turn::Completed);
    shown_as(&mut w, "asking", Turn::Waiting);
    shown_as(&mut w, "resuming", Turn::Completed);
    shown_as(&mut w, "scanned", Turn::Completed);
    let newer = Content {
        updated: 100,
        ..Default::default()
    };
    w.content
        .insert("content-newer".into(), std::sync::Arc::new(newer));
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
    assert_eq!(facts.space_counts("space").waiting, 1);
    assert_eq!(facts.space_counts("space").group(), Status::Waiting);

    w.spaces.push(Space {
        id: "nested".into(),
        name: "Nested".into(),
        cwd: "/tmp/nested".into(),
    });
    w.agent_sessions
        .push(session("failed", Some("t3"), true, 11));
    w.agent_sessions.last_mut().unwrap().cwd = "/tmp/nested".into();
    shown_as(&mut w, "failed", Turn::Error);
    assert_same_as_walking(&w, &resumed, 1000);
    let counts = FrameFacts::work_out(&w, inputs(&w, &resumed, 1000));
    // Counted where its tab is, not in the project space its folder sits in.
    assert_eq!(counts.space_counts("space").errors, 1);
    assert_eq!(counts.space_counts("nested").errors, 0);

    w.lost_agents
        .insert("t1".into(), Some("record-newer".into()));
    shown_as(&mut w, "record-newer", Turn::Interrupted);
    w.quiet_spaces.insert("space".into());
    assert_same_as_walking(&w, &resumed, 1000);
    let quiet = FrameFacts::work_out(&w, inputs(&w, &resumed, 1000));
    let rows = quiet.quiet_space_sessions(&w, "space");
    assert!(rows.iter().any(|s| s.id == "record-newer"));
    assert!(
        rows.iter().any(|s| s.id == "failed"),
        "quiet follows the tab"
    );
    assert_eq!(quiet.space_counts("nested").count, 0);
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
