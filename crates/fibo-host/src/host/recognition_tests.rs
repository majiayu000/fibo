use super::*;
use fibo_platform::local;

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
    let (socket, _peer) = local::Stream::pair().unwrap();
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
    let mut index =
        crate::log_index::Index::open(&root, vec![(Agent::Codex, roots[0].clone())]).unwrap();
    index.initialize().unwrap();
    index.bootstrap().unwrap();
    let shared = shared_host(&root, &index);
    let (tx, rx) = sync_channel(16);
    let (socket, _peer) = local::Stream::pair().unwrap();
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
    let batch = fibo_platform::file_watch::Batch {
        events: vec![fibo_platform::file_watch::Event {
            path: root.join("logs/a"),
            flags: fibo_platform::file_watch::flag::MUST_SCAN,
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
    let mut phases: Vec<_> = published(&rx).into_iter().map(|s| s.phase).collect();
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
    use fibo_core::model::{AgentSession, Binding, Outcome};
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

#[test]
fn a_window_that_stops_reading_is_caught_up_not_cut_off() {
    let (host, _root) = super::tests::fixture();
    let shared = Arc::new(Mutex::new(host));
    let (server, mut window) = local::Stream::pair().unwrap();
    {
        let shared = Arc::clone(&shared);
        thread::spawn(move || status::watch_recognition(server, shared, 7));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !lock(&shared)
        .unwrap()
        .recognition_subscribers
        .contains_key(&7)
    {
        assert!(Instant::now() < deadline, "window never subscribed");
        thread::sleep(Duration::from_millis(5));
    }
    // Far more than the queue and the socket buffer hold while the window reads nothing.
    let stale = |n: usize| {
        let status = RecognitionStatus {
            detail: format!("{n}:{}", "x".repeat(64 * 1024)),
            ..RecognitionStatus::default()
        };
        Arc::new(serde_json::to_vec(&Reply::Recognition(status)).unwrap())
    };
    for n in 0..64 {
        status::offer_recognition(&mut lock(&shared).unwrap(), stale(n));
    }
    {
        let mut host = lock(&shared).unwrap();
        assert!(
            host.recognition_subscribers.contains_key(&7),
            "a full queue must not cut the window"
        );
        host.recognition.detail = "current".into();
    }
    window
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut seen = 0;
    loop {
        let (_, bytes) = read_frame(&mut window).expect("window was cut off");
        seen += 1;
        let Reply::Recognition(status) = serde_json::from_slice(&bytes).unwrap() else {
            panic!()
        };
        if status.detail == "current" {
            break;
        }
    }
    assert!(
        seen < 65,
        "stale statuses are skipped once the window catches up"
    );
}

/// A folder that held logs is deleted (a removed worktree's conversations, say) while the
/// Mac sleeps; waking, FSEvents asks for a rescan of it. Once its logs are forgotten the
/// repair is over: recognition is Healthy again, saying what went. It used to stay
/// Interrupted, which nothing cleared, and the window read that as the host being cut off.
#[test]
fn a_deleted_log_folder_ends_its_repair_healthy() {
    let root = std::env::temp_dir().join(format!("kl-lost-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("logs/gone")).unwrap();
    let root = root.canonicalize().unwrap();
    let logs = root.join("logs");
    fs::write(
        logs.join("gone/old.jsonl"),
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"old\",\"cwd\":\"/tmp\"}}\n",
    )
    .unwrap();
    let mut index =
        crate::log_index::Index::open(&root, vec![(Agent::Codex, logs.clone())]).unwrap();
    index.initialize().unwrap();
    index.bootstrap().unwrap();
    assert_eq!(index.cached().unwrap().len(), 1);
    let shared = shared_host(&root, &index);
    let (tx, rx) = sync_channel(16);
    let (socket, _peer) = local::Stream::pair().unwrap();
    lock(&shared).unwrap().recognition_subscribers.insert(
        1,
        Subscriber {
            tx,
            socket,
            lagged: Default::default(),
        },
    );

    fs::remove_dir_all(logs.join("gone")).unwrap();
    let scopes = vec![logs.join("gone")];
    recover_logs(&shared, &mut index, &mut Association::default(), &scopes).unwrap();

    let last = published(&rx).pop().unwrap();
    assert_eq!(last.phase, RecognitionPhase::Healthy, "{}", last.detail);
    assert!(last.detail.contains("被删除"), "{}", last.detail);
    assert_eq!(
        lock(&shared).unwrap().recognition.phase,
        RecognitionPhase::Healthy
    );
    assert!(index.cached().unwrap().is_empty());
    assert!(index.recovery_scopes().unwrap().is_empty());
    drop(shared);
    drop(index);
    fs::remove_dir_all(root).unwrap();
}

/// A log batch commits a new session and a card it changed, and nothing when every card
/// it read is as the workspace has it: only the cards read are compared.
#[test]
fn a_batch_commits_only_when_a_card_it_read_changed() {
    let root = std::env::temp_dir().join(format!("kl-cards-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("logs")).unwrap();
    let root = root.canonicalize().unwrap();
    let logs = root.join("logs");
    let ask = |text: &str| {
        format!("{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"user_message\",\"message\":\"{text}\"}}}}\n")
    };
    let log = logs.join("a.jsonl");
    let meta = "{\"type\":\"session_meta\",\"payload\":{\"id\":\"a\",\"cwd\":\"/tmp\"}}\n";
    fs::write(&log, format!("{meta}{}", ask("第一句"))).unwrap();
    let other = fibo_core::content::Content {
        title: "别的会话".into(),
        ..Default::default()
    };
    let mut index =
        crate::log_index::Index::open(&root, vec![(Agent::Codex, logs.clone())]).unwrap();
    index.initialize().unwrap();
    index.bootstrap().unwrap();
    let shared = shared_host(&root, &index);
    lock(&shared)
        .unwrap()
        .workspace
        .content
        .insert("x".into(), other.into());
    let revision = || lock(&shared).unwrap().workspace.revision;
    let mut association = Association::default();
    let batch = index.cached().unwrap();
    association.apply(&shared, &index, &batch).unwrap();
    let first = revision();
    assert_eq!(lock(&shared).unwrap().workspace.agent_sessions.len(), 1);
    association.apply(&shared, &index, &batch).unwrap();
    assert_eq!(revision(), first, "the same card again commits nothing");

    fs::write(&log, format!("{meta}{}{}", ask("第一句"), ask("第二句"))).unwrap();
    let batch = vec![index.update(Agent::Codex, &log, false).unwrap().unwrap()];
    association.apply(&shared, &index, &batch).unwrap();
    assert!(revision() > first, "a changed card commits");
    let host = lock(&shared).unwrap();
    let id = &host.workspace.agent_sessions[0].id;
    assert_eq!(host.workspace.content[id].asks.len(), 2);
    assert_eq!(host.workspace.content["x"].title, "别的会话");
    drop(host);
    drop(shared);
    drop(index);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn large_log_batch_keeps_existing_identities_and_applies_duplicate_paths_in_order() {
    use crate::recognition::{observation, Transcript};

    let root = std::env::temp_dir().join(format!("fibo-association-{}", uuid::Uuid::new_v4()));
    let logs = root.join("logs");
    fs::create_dir_all(&logs).unwrap();
    let mut index =
        crate::log_index::Index::open(&root, vec![(Agent::Codex, logs.clone())]).unwrap();
    index.initialize().unwrap();
    let shared = shared_host(&root, &index);
    let now = fibo_core::content::now();
    let transcript = |runtime, id: &str, file: &str, updated| Transcript {
        runtime,
        cli_id: id.into(),
        path: logs.join(file).to_str().unwrap().into(),
        cwd: "/repo".into(),
        updated,
    };
    let first = transcript(Agent::Codex, "same", "first.jsonl", now);
    let last = transcript(Agent::Codex, "same", "last.jsonl", now);
    let fresh = transcript(Agent::Codex, "fresh", "fresh.jsonl", now);
    for (t, prompt) in [
        (&first, "first prompt"),
        (&last, "last prompt"),
        (&fresh, "new prompt"),
    ] {
        let meta = serde_json::json!({"type":"session_meta","payload":{"id":t.cli_id,"cwd":t.cwd}});
        let ask = serde_json::json!({"timestamp":now,"type":"event_msg","payload":{"type":"user_message","message":prompt}});
        fs::write(&t.path, format!("{meta}\n{ask}\n")).unwrap();
        index
            .update(Agent::Codex, Path::new(&t.path), true)
            .unwrap();
    }
    let claude = transcript(Agent::Claude, "same", "same-claude.jsonl", 0);
    let first_id = {
        let mut host = lock(&shared).unwrap();
        let mut workspace = host.workspace.clone();
        let mut original = observation(&first, None);
        original.name = "handwritten name".into();
        let first_id = original.id.clone();
        workspace
            .agent_sessions
            .extend([original, observation(&claude, None)]);
        for n in 0..128 {
            let id = format!("stored-{n}");
            let old = transcript(Agent::Codex, &id, "old.jsonl", 0);
            workspace.agent_sessions.push(observation(&old, None));
        }
        host.commit(workspace).unwrap();
        first_id
    };
    drop(shared);
    let shared = shared_host(&root, &index);
    let mut batch = vec![first, last.clone(), fresh.clone(), fresh, claude];
    for n in 0..128 {
        let id = format!("unmanaged-{n}");
        batch.push(transcript(Agent::Codex, &id, "unmanaged.jsonl", 0));
    }
    Association::default()
        .apply(&shared, &index, &batch)
        .unwrap();
    {
        let host = lock(&shared).unwrap();
        let workspace = &host.workspace;
        assert_eq!(workspace.agent_sessions.len(), 131);
        assert_eq!(workspace.agent_sessions[0].transcript, last.path);
        assert_eq!(workspace.agent_sessions[0].name, "handwritten name");
        assert_eq!(workspace.content[&first_id].title, "last prompt");
        assert_eq!(workspace.agent_sessions[1].runtime, Agent::Claude);
        assert!(!workspace
            .content
            .contains_key(&workspace.agent_sessions[1].id));
        let added = workspace.agent_sessions.last().unwrap();
        assert_eq!(added.cli_session_id, "fresh");
        assert_eq!(workspace.content[&added.id].title, "new prompt");
    }
    drop(shared);
    drop(index);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_removed_log_clears_the_published_card_but_keeps_the_session_and_outcome() {
    let root = std::env::temp_dir().join(format!("fibo-removed-card-{}", uuid::Uuid::new_v4()));
    let logs = root.join("logs");
    fs::create_dir_all(&logs).unwrap();
    let root = root.canonicalize().unwrap();
    let logs = root.join("logs");
    let log = logs.join("old.jsonl");
    let write_log = |path: &std::path::Path, id: &str, text: &str| {
        let meta = serde_json::json!({
            "type":"session_meta", "payload":{"id":id,"cwd":"/tmp"}
        });
        let message = serde_json::json!({
            "type":"event_msg", "payload":{"type":"user_message","message":text}
        });
        let call = serde_json::json!({
            "type":"response_item",
            "payload":{"type":"function_call","name":"list","call_id":id,"arguments":"{}"}
        });
        fs::write(path, format!("{meta}\n{message}\n{call}\n")).unwrap();
    };
    write_log(&log, "old", "test prompt");
    let mut index = crate::log_index::Index::open(&root, vec![(Agent::Codex, logs)]).unwrap();
    let checkpoint = index.initialize().unwrap();
    let shared = shared_host(&root, &index);
    let mut association = Association::default();
    let batch = index.update_many([log.clone()], checkpoint + 1).unwrap();
    association.apply(&shared, &index, &batch).unwrap();
    let (id, revision) = {
        let mut host = lock(&shared).unwrap();
        let mut next = host.workspace.clone();
        let session = &mut next.agent_sessions[0];
        session.outcome = fibo_core::model::Outcome::Completed;
        let id = session.id.clone();
        host.commit(next).unwrap();
        assert_eq!(host.snapshot().content[&id].title, "test prompt");
        (id, host.workspace.revision)
    };

    fs::remove_file(&log).unwrap();
    assert!(index
        .update_many([log.clone()], checkpoint + 2)
        .unwrap()
        .is_empty());
    // The path is reused before the host receives the removal.
    write_log(&log, "new", "new prompt");
    let batch = index.update_many([log.clone()], checkpoint + 3).unwrap();
    // Model a crash after the index committed, before the host saved its new
    // workspace. A new index has no Held entries or pending removals.
    let (cold_store, cold_workspace) = Store::open(&root).unwrap();
    let cold_index =
        crate::log_index::Index::open(&root, vec![(Agent::Codex, root.join("logs"))]).unwrap();
    let stored = cold_workspace
        .agent_sessions
        .iter()
        .find(|s| s.id == id)
        .unwrap();
    assert_eq!(stored.transcript, log.to_str().unwrap());
    assert!(cold_index
        .card_for(stored.runtime, &stored.cli_session_id, &stored.transcript)
        .unwrap()
        .is_none());
    assert_eq!(
        cold_index
            .card_for(Agent::Codex, "new", &stored.transcript)
            .unwrap()
            .unwrap()
            .title,
        "new prompt",
    );
    drop((cold_store, cold_index));

    let chat = super::client::dispatch(
        &shared,
        Request::Chat {
            session: id.clone(),
            after: None,
            before: None,
            limit: 10,
            tools: true,
        },
    )
    .unwrap();
    assert!(matches!(chat, Reply::Chat { ref messages, .. } if messages.is_empty()));
    assert!(index
        .tool_call(log.to_str().unwrap(), "new")
        .unwrap()
        .is_some());
    assert!(super::client::dispatch(
        &shared,
        Request::ToolCall {
            session: id.clone(),
            call: "new".into(),
        }
    )
    .is_err());
    Association::apply_removals(&shared, &index).unwrap();
    association.apply(&shared, &index, &batch).unwrap();
    let host = lock(&shared).unwrap();
    let snapshot = host.snapshot();
    assert!(!host.workspace.content.contains_key(&id));
    assert!(!snapshot.content.contains_key(&id));
    assert!(snapshot.revision > revision);
    let session = snapshot.agent_sessions.iter().find(|s| s.id == id).unwrap();
    assert_eq!(session.outcome, fibo_core::model::Outcome::Completed);
    assert!(session.transcript.is_empty());
    let new = snapshot
        .agent_sessions
        .iter()
        .find(|s| s.cli_session_id == "new")
        .unwrap();
    assert_eq!(new.transcript, log.to_str().unwrap());
    assert_eq!(snapshot.content[&new.id].title, "new prompt");
    drop(host);

    // A file can also be replaced between two visits without a separate removal.
    let replacement = root.join("replacement.jsonl");
    write_log(&replacement, "replacement", "replacement prompt");
    fs::rename(replacement, &log).unwrap();
    let batch = index.update_many([log], checkpoint + 4).unwrap();
    let db = rusqlite::Connection::open(root.join("workbench.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_workspace_commit BEFORE UPDATE ON meta
         BEGIN SELECT RAISE(ABORT, 'failed to publish removal'); END;",
    )
    .unwrap();
    assert!(Association::apply_removals(&shared, &index).is_err());
    assert_eq!(
        lock(&shared).unwrap().snapshot().content[&new.id].title,
        "new prompt"
    );

    db.execute_batch("DROP TRIGGER reject_workspace_commit")
        .unwrap();
    drop(db);
    // The index checkpoint already includes the deletion. The same index bootstraps
    // after the subscription error, so it must publish the queued removal unaided.
    assert_eq!(index.initialize().unwrap(), checkpoint + 4);
    index.bootstrap().unwrap();
    Association::apply_removals(&shared, &index).unwrap();
    association.apply(&shared, &index, &batch).unwrap();
    let snapshot = lock(&shared).unwrap().snapshot();
    let previous = snapshot
        .agent_sessions
        .iter()
        .find(|s| s.cli_session_id == "new")
        .unwrap();
    assert!(previous.transcript.is_empty());
    assert!(!snapshot.content.contains_key(&previous.id));
    let latest = snapshot
        .agent_sessions
        .iter()
        .find(|s| s.cli_session_id == "replacement")
        .unwrap();
    assert_eq!(snapshot.content[&latest.id].title, "replacement prompt");
    drop(shared);
    drop(index);
    fs::remove_dir_all(root).unwrap();
}

/// A host over `root`'s store with nothing running, for driving recognition by hand.
fn shared_host(root: &std::path::Path, index: &crate::log_index::Index) -> Shared {
    let (store, workspace) = Store::open(root).unwrap();
    Arc::new(Mutex::new(Host {
        remote: None,
        store,
        workspace,
        terminals: HashMap::new(),
        closed: Default::default(),
        instance: "test".into(),
        index: index.fork(root).unwrap(),
        transcripts: Arc::new(Mutex::new(index.fork(root).unwrap())),
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
        started_clis: HashMap::new(),
        deadlines: None,
        myself: std::sync::Weak::new(),
        http_asks: HashMap::new(),
        screen_waits: HashMap::new(),
        screen_answered: HashMap::new(),
        title_busy: HashMap::new(),
        screen_busy: HashMap::new(),
        hooks: Default::default(),
        report_back: Default::default(),
        shell_watch: Default::default(),
        reconciled: HashMap::new(),
    }))
}

/// Recognition status frames sent to subscriber `rx`, in order.
fn published(rx: &std::sync::mpsc::Receiver<Frame>) -> Vec<RecognitionStatus> {
    rx.try_iter()
        .map(
            |(_, bytes)| match serde_json::from_slice::<Reply>(&bytes).unwrap() {
                Reply::Recognition(s) => s,
                _ => panic!(),
            },
        )
        .collect()
}
