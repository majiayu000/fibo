use super::*;
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
