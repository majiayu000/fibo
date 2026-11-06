use super::*;

#[test]
fn quiet_agent_identity_is_published_before_a_transcript_and_cleared_on_exit() {
    let (mut host, root) = super::tests::fixture();
    host.workspace.terminals.push(Terminal {
        id: "t".into(),
        space: "s".into(),
        argv: vec!["/bin/sh".into()],
        cwd: "/tmp".into(),
        pid: Some(100),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    let process = |pid, parent, runtime| crate::recognition::Process {
        pid,
        parent,
        runtime,
        cli_session_id: None,
        no_daemon: true,
        launch: vec![],
    };
    let shell = process(100, 0, None);
    let codex = process(101, 100, Some(Agent::Codex));
    let (tx, rx) = sync_channel(16);
    let (socket, _peer) = UnixStream::pair().unwrap();
    host.recognition_subscribers.insert(
        1,
        Subscriber {
            tx,
            socket,
            lagged: Default::default(),
        },
    );
    host.observe_processes(&[shell.clone(), codex.clone()], &HashMap::new())
        .unwrap();
    assert_eq!(
        host.recognition.terminal_agents.get("t"),
        Some(&Agent::Codex)
    );
    assert!(
        host.recognition.terminal_notes.is_empty(),
        "identity alone must not invent a warning"
    );
    assert!(
        host.workspace.agent_sessions.is_empty(),
        "identity is not a conversation"
    );
    let (_, bytes) = rx
        .try_recv()
        .expect("identity-only change must reach the window");
    let Reply::Recognition(seen) = serde_json::from_slice(&bytes).unwrap() else {
        panic!()
    };
    assert_eq!(seen.terminal_agents.get("t"), Some(&Agent::Codex));
    host.observe_processes(&[shell.clone(), codex], &HashMap::new())
        .unwrap();
    assert!(
        rx.try_recv().is_err(),
        "unchanged identity must not publish another update"
    );
    host.agent_exited(101).unwrap();
    assert!(!host.recognition.terminal_agents.contains_key("t"));
    assert!(
        rx.try_recv().is_ok(),
        "exit must clear the icon in the window"
    );
    let claude = process(102, 100, Some(Agent::Claude));
    host.observe_processes(&[shell.clone(), claude], &HashMap::new())
        .unwrap();
    assert_eq!(
        host.recognition.terminal_agents.get("t"),
        Some(&Agent::Claude)
    );
    host.observe_processes(&[shell], &HashMap::new()).unwrap();
    assert!(host.recognition.terminal_agents.is_empty());
    drop(host);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn must_scan_repairs_only_event_subtree_and_publishes_status_then_continues() {
    let root = std::env::temp_dir().join(format!("kl-gap-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("logs/a")).unwrap();
    fs::create_dir_all(root.join("logs/b")).unwrap();
    let root = root.canonicalize().unwrap();
    let roots = vec![root.join("logs")];
    let (store, workspace) = Store::open(&root).unwrap();
    let mut index =
        crate::log_index::Index::open(&root, vec![(Agent::Codex, roots[0].clone())]).unwrap();
    index.initialize().unwrap();
    index.bootstrap().unwrap();
    let shared = Arc::new(Mutex::new(Host {
        remote: None,
        store,
        workspace,
        terminals: HashMap::new(),
        closed: Default::default(),
        instance: "test".into(),
        index: index.fork(&root).unwrap(),
        recognition: RecognitionStatus::default(),
        log_events: None,
        new_roots: None,
        recognition_subscribers: HashMap::new(),
        event_subscribers: HashMap::new(),
        events: Default::default(),
        exits: None,
        agent_pids: HashMap::new(),
        unbound_pids: HashMap::new(),
        released_logs: Default::default(),
        unrecorded_pids: HashMap::new(),
        submits: HashMap::new(),
        input_probes: Default::default(),
        deadlines: None,
        myself: std::sync::Weak::new(),
        http_asks: HashMap::new(),
        screen_waits: HashMap::new(),
        title_busy: HashMap::new(),
        screen_busy: HashMap::new(),
        hooks: Default::default(),
        report_back: Default::default(),
        shell_watch: Default::default(),
        reconciled: HashMap::new(),
    }));
    let (tx, rx) = sync_channel(16);
    let (socket, _peer) = UnixStream::pair().unwrap();
    lock(&shared).unwrap().recognition_subscribers.insert(
        1,
        Subscriber {
            tx,
            socket,
            lagged: Default::default(),
        },
    );
    for dir in ["a", "b"] {
        fs::write(
            root.join(format!("logs/{dir}/new.jsonl")),
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{dir}\",\"cwd\":\"/tmp\"}}}}\n"
            ),
        )
        .unwrap();
    }
    let batch = crate::file_events::Batch {
        events: vec![crate::file_events::Event {
            path: root.join("logs/a"),
            flags: 1,
            id: 1,
        }],
    };
    let mut scopes = vec![];
    assert!(!event_scopes(&batch, &roots, &mut scopes));
    let before = index.counts.clone();
    recover_logs(&shared, &mut index, &mut Association::default(), &scopes).unwrap();
    assert_eq!(index.counts.directories - before.directories, 1);
    assert_eq!(
        index
            .cached()
            .unwrap()
            .iter()
            .map(|t| t.cli_id.as_str())
            .collect::<Vec<_>>(),
        ["a"]
    );
    let mut phases: Vec<_> = rx
        .try_iter()
        .map(
            |(_, bytes)| match serde_json::from_slice::<Reply>(&bytes).unwrap() {
                Reply::Recognition(s) => s.phase,
                _ => panic!(),
            },
        )
        .collect();
    // Content imports publish a revision while recovery remains in progress.
    phases.dedup();
    assert_eq!(
        phases,
        [
            RecognitionPhase::Interrupted,
            RecognitionPhase::Recovering,
            RecognitionPhase::Healthy
        ]
    );
    assert!(index.recovery_scopes().unwrap().is_empty());
    index
        .update(Agent::Codex, &root.join("logs/b/new.jsonl"), true)
        .unwrap();
    assert_eq!(index.cached().unwrap().len(), 2);
    drop(shared);
    drop(index);
    fs::remove_dir_all(root).unwrap();
}

/// After a restart a screen-bound conversation has only its pane to go by. Undoing
/// `only_cli_of_screen_bound` leaves its exit unnoticed: the pane keeps the dead session.
#[test]
fn a_new_host_takes_the_one_cli_in_a_pane_for_its_screen_bound_conversation() {
    use mooring_core::model::{AgentSession, Binding, Outcome};
    let session = |binding| AgentSession {
        terminal: Some("t".into()),
        binding,
        runtime: Agent::Codex,
        outcome: Outcome::Open,
        ..crate::host::tests::session("s", 1, Some("t"), true)
    };
    let process =
        |pid, parent, runtime: Option<Agent>, named: Option<&str>| crate::recognition::Process {
            pid,
            parent,
            runtime,
            cli_session_id: named.map(str::to_owned),
            no_daemon: true,
            launch: vec![],
        };
    let mut w = Workspace::default();
    w.terminals.push(Terminal {
        id: "t".into(),
        space: "s".into(),
        argv: vec!["sh".into()],
        cwd: "/".into(),
        pid: Some(100),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    let shell = process(100, 1, None, None);
    let wrapper = process(101, 100, Some(Agent::Codex), None);
    let native = process(102, 101, Some(Agent::Codex), None);
    let one = [shell.clone(), wrapper.clone(), native.clone()];
    let only = crate::recognition::only_cli_of_screen_bound;
    let screen = session(Some(Binding::Screen));
    assert!(only(&screen, &native, &one, &w));
    assert!(!only(&screen, &wrapper, &one, &w), "its wrapper is not it");
    assert!(
        !only(&session(Some(Binding::Process)), &native, &one, &w),
        "other evidence decides that binding"
    );
    let second = process(103, 100, Some(Agent::Codex), None);
    let two = [shell.clone(), wrapper, native.clone(), second];
    assert!(!only(&screen, &native, &two, &w), "two CLIs: no telling");
    let named = process(104, 100, Some(Agent::Codex), Some("other"));
    assert!(!only(&screen, &named, &[shell, named.clone()], &w));
}
