use super::*;
pub(super) fn fixture() -> (Host, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("kl-snapshot-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let (store, _) = Store::open(&root).unwrap();
    let index = crate::log_index::Index::open(&root, vec![]).unwrap();
    let host = Host {
        store,
        workspace: Workspace::default(),
        terminals: HashMap::new(),
        closed: Default::default(),
        instance: "test".into(),
        index,
        recognition: RecognitionStatus::default(),
        log_events: None,
        new_roots: None,
        recognition_subscribers: HashMap::new(),
        event_subscribers: HashMap::new(),
        events: Default::default(),
        exits: None,
        agent_pids: HashMap::new(),
        unbound_pids: HashMap::new(),
        unrecorded_pids: HashMap::new(),
        submits: HashMap::new(),
        input_probes: Default::default(),
        deadlines: None,
        myself: std::sync::Weak::new(),
        screen_waits: HashMap::new(),
        title_busy: HashMap::new(),
        screen_busy: HashMap::new(),
        hooks: Default::default(),
        report_back: Default::default(),
        shell_watch: Default::default(),
        reconciled: HashMap::new(),
        remote: None,
    };
    (host, root)
}
pub(super) fn session(
    id: &str,
    updated: u64,
    terminal: Option<&str>,
    current: bool,
) -> AgentSession {
    AgentSession {
        id: id.into(),
        runtime: Agent::Codex,
        cli_session_id: id.into(),
        terminal: terminal.map(str::to_owned),
        current,
        task: None,
        outcome: Outcome::Open,
        updated,
        cwd: "/tmp".into(),
        transcript: String::new(),
        name: String::new(),
        binding: None,
    }
}
#[test]
fn snapshot_keeps_only_retained_content_and_applies_screen_wait_without_mutating_source() {
    use mooring_core::content::{Content, Turn};
    let (mut host, root) = fixture();
    host.workspace.agent_sessions = vec![
        session(
            "recent-no-content",
            mooring_core::content::now(),
            None,
            false,
        ),
        session("expired", 0, None, false),
        session("live", 0, Some("t"), true),
        session("historical", 0, Some("history"), false),
    ];
    for id in ["expired", "live", "historical", "orphan"] {
        host.workspace.content.insert(id.into(), Content::default());
    }
    let seen = crate::screen_rules::explain(
        crate::screen_rules::bundled(Agent::Claude),
        "Do you want to continue?\n❯ 1. Yes\n  2. No",
    );
    host.title_busy.insert("t".into(), ("live".into(), 1));
    assert!(seen.waiting());
    host.screen_waits.insert("t".into(), seen);
    let snapshot = host.snapshot();
    assert_eq!(
        snapshot
            .agent_sessions
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["recent-no-content", "live", "historical"]
    );
    let mut ids = snapshot
        .content
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(ids, ["historical", "live"]);
    let live = &snapshot.content["live"];
    assert_eq!(live.turn, Turn::Waiting);
    assert_eq!(live.waiting_episode, 1);
    assert_eq!(live.approval, "Do you want to continue?");
    assert_eq!(live.signal, "画面");
    assert_eq!(snapshot.content["historical"], Content::default());
    assert_eq!(host.workspace.agent_sessions.len(), 4);
    assert_eq!(host.workspace.content["live"], Content::default());
    host.workspace.agent_sessions.clear();
    assert!(host.snapshot().content.is_empty());
    drop(host);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn an_approved_dialog_runs_again_once_the_screen_ticks() {
    use mooring_core::content::{Content, Turn};
    let (mut host, root) = fixture();
    host.workspace.agent_sessions = vec![session("live", 0, Some("t"), true)];
    let asked = Content {
        turn: Turn::Waiting,
        waiting: "CLI 请求工具审批（Hook）".into(),
        signal: "Hook".into(),
        signal_at: 100,
        ..Content::default()
    };
    host.workspace.content.insert("live".into(), asked);
    // Still on screen: waiting, even with a timer line under the dialog.
    let seen = crate::screen_rules::explain(
        crate::screen_rules::bundled(Agent::Claude),
        "Do you want to proceed?\n❯ 1. Yes\n  2. No",
    );
    host.screen_waits.insert("t".into(), seen);
    host.screen_busy.insert("t".into(), ("live".into(), 101));
    assert_eq!(host.snapshot().content["live"].turn, Turn::Waiting);
    // Answered: the dialog is gone and the timer came back after it.
    host.screen_waits.remove("t");
    let live = &host.snapshot().content["live"];
    assert_eq!(live.turn, Turn::Running);
    assert!(live.waiting.is_empty());
    // A timer from before the dialog proves nothing about it.
    host.screen_busy.insert("t".into(), ("live".into(), 99));
    assert_eq!(host.snapshot().content["live"].turn, Turn::Waiting);
    drop(host);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_program_report_outranks_screen_guesses_and_ends_with_its_program() {
    use crate::program_status::Said;
    use mooring_core::content::{Content, Turn};
    let (mut host, root) = fixture();
    host.workspace.agent_sessions = vec![session("live", 0, Some("t"), true)];
    host.workspace
        .content
        .insert("live".into(), Content::default());
    // The title spins, but the program says it is waiting on the user.
    host.title_busy.insert("t".into(), ("live".into(), 1));
    let said = |state: &str, msg: &str| Said {
        state: state.into(),
        kind: None,
        progress: None,
        app: "my-script".into(),
        title: String::new(),
        msg: msg.into(),
    };
    host.program_said("t", Some(said("blocked", "要部署到生产吗？")))
        .unwrap();
    let live = &host.snapshot().content["live"];
    assert_eq!(live.turn, Turn::Waiting);
    assert_eq!(live.waiting, "要部署到生产吗？（程序上报）");
    assert_eq!(live.signal, "程序上报");
    // The same word again keeps its time; a new word restarts it.
    let since = host.recognition.terminal_reports["t"].since;
    host.recognition
        .terminal_reports
        .get_mut("t")
        .unwrap()
        .since = 7;
    host.program_said("t", Some(said("blocked", "还在等")))
        .unwrap();
    assert_eq!(host.recognition.terminal_reports["t"].since, 7);
    host.program_said("t", Some(said("done", ""))).unwrap();
    assert!(host.recognition.terminal_reports["t"].since >= since);
    assert_eq!(host.snapshot().content["live"].turn, Turn::Completed);
    // Its command ended: back to what the title and the log say.
    host.program_said("t", None).unwrap();
    assert!(host.recognition.terminal_reports.is_empty());
    assert_eq!(host.snapshot().content["live"].signal, "标题");
    drop(host);
    fs::remove_dir_all(root).unwrap();
}

/// Stage 4: `/new` in a terminal puts another conversation there; subscribers hear
/// `session_switched` naming the new one and, as `detail`, the one before, once.
#[test]
fn a_terminal_that_moves_to_another_conversation_says_so_once() {
    let mut w = Workspace::default();
    w.terminals.push(Terminal {
        id: "t".into(),
        space: "s".into(),
        argv: vec![],
        cwd: "/tmp".into(),
        pid: Some(1),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    w.agent_sessions = vec![session("old", 0, Some("t"), true)];
    let mut tracker = events::Tracker::default();
    tracker.seed(&w);
    let switched = |n: &Vec<mooring_core::hook::Notice>| {
        n.iter()
            .filter(|n| n.event == "session_switched")
            .map(|n| (n.session.clone(), n.detail.clone(), n.terminal.clone()))
            .collect::<Vec<_>>()
    };
    assert!(switched(&tracker.changes(&w)).is_empty());
    w.agent_sessions[0].current = false;
    w.agent_sessions.push(session("new", 1, Some("t"), true));
    assert_eq!(
        switched(&tracker.changes(&w)),
        [("new".into(), "old".into(), Some("t".into()))]
    );
    assert!(switched(&tracker.changes(&w)).is_empty(), "said once");
}
