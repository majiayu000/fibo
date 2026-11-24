use super::*;
use fibo_platform::local;

// One wait until an actual outcome/group deadline. No tick, no directory or process I/O.
fn start_deadlines(shared: &Shared) -> io::Result<()> {
    let (tx, rx) = sync_channel(1);
    lock(shared)?.deadlines = Some(tx);
    let weak = Arc::downgrade(shared);
    thread::spawn(move || {
        let mut child_dialogs_at = 0;
        let mut run = || -> io::Result<()> {
            loop {
                let Some(shared) = weak.upgrade() else {
                    return Ok(());
                };
                let deadline = {
                    let mut host = lock(&shared)?;
                    let now = fibo_core::content::now();
                    if host.workspace.outcomes_due(now) {
                        let mut next = host.workspace.clone();
                        next.expire_outcomes(now);
                        host.commit(next)?;
                    }
                    let outcome = host.workspace.next_deadline(now);
                    let dialogs = host.child_dialogs();
                    let check = if dialogs.is_empty() {
                        None
                    } else {
                        if child_dialogs_at <= now {
                            host.reconcile(&shared, dialogs);
                            child_dialogs_at = now + CHILD_DIALOG_EVERY;
                        }
                        Some(child_dialogs_at)
                    };
                    (outcome, check.into_iter().chain(outcome).min())
                };
                drop(shared);
                let (outcome, deadline) = deadline;
                let result = match deadline {
                    Some(at) => rx.recv_timeout(std::time::Duration::from_secs(
                        at.saturating_sub(fibo_core::content::now()),
                    )),
                    None => rx
                        .recv()
                        .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected),
                };
                match result {
                    Ok(()) => (),
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    // Woken only to look at subagents' dialogs again: the loop does that.
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                        if outcome.is_none_or(|at| at > fibo_core::content::now()) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        let Some(shared) = weak.upgrade() else {
                            return Ok(());
                        };
                        let mut host = lock(&shared)?;
                        let mut next = host.workspace.clone();
                        next.expire_outcomes(fibo_core::content::now());
                        host.commit(next)?;
                    }
                }
            }
        };
        // A failed commit (a busy or full disk) must not stop outcomes expiring until the
        // host restarts: start the loop again with the same backoff as the log subscription.
        let mut retry = SUBSCRIPTION_RETRY_MIN;
        loop {
            let started = Instant::now();
            let Err(e) = run() else { return };
            if started.elapsed() > SUBSCRIPTION_RETRY_MAX {
                retry = SUBSCRIPTION_RETRY_MIN;
            }
            eprintln!("outcome deadline failed, retrying in {retry:?}: {e}");
            thread::sleep(retry);
            retry = (retry * 2).min(SUBSCRIPTION_RETRY_MAX);
        }
    });
    Ok(())
}
/// A terminal fully prepared while the previous host still owns its process.
struct Taken {
    id: String,
    live: Live,
    reader: term::Reader,
    replies: Arc<Mutex<Vec<u8>>>,
}
fn take_over(root: &Path) -> io::Result<(local::Stream, Vec<Taken>)> {
    let mut stream = local::Stream::connect(root.join("workbench.sock"))?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
    match receive::<Reply>(&mut stream)? {
        Reply::Hello { major: MAJOR, .. } => (),
        other => return Err(io::Error::other(format!("old host refused: {other:?}"))),
    }
    send(&mut stream, CONTROL, &Request::Handoff)?;
    // A host from before handoffs drops the connection at the unknown request.
    let terminals = match receive::<Reply>(&mut stream) {
        Ok(Reply::Handoff { terminals }) => terminals,
        other => {
            return Err(io::Error::other(format!(
                "the running host cannot hand its terminals over: {other:?}"
            )));
        }
    };
    let mut snapshots = vec![];
    for _ in &terminals {
        let mut snapshot = vec![];
        loop {
            match read_frame(&mut stream)? {
                (SNAPSHOT, bytes) => snapshot.extend(bytes),
                (READY, _) => break,
                (kind, _) => return Err(io::Error::other(format!("unexpected frame {kind}"))),
            }
        }
        snapshots.push(snapshot);
    }
    let masters = term::receive_masters(&stream, terminals.len())?;
    let taken = terminals
        .into_iter()
        .zip(snapshots)
        .zip(masters)
        .map(|((handed, snapshot), master)| {
            let mut vt = Vt::restore(&snapshot)?;
            let replies = Arc::new(Mutex::new(Vec::new()));
            let out = Arc::clone(&replies);
            vt.on_reply(move |bytes| {
                if let Ok(mut out) = out.lock() {
                    out.extend_from_slice(bytes);
                }
            })?;
            let cell = CellPixels::default();
            cell.set(handed.cell.0, handed.cell.1);
            vt.answer_queries(cell.clone())?;
            let reader = master.reader()?;
            let input = Input::start(&master, &handed.id)?;
            Ok(Taken {
                id: handed.id,
                live: Live {
                    vt,
                    master,
                    input,
                    cols: handed.cols,
                    rows: handed.rows,
                    pid: handed.pid,
                    owner: None,
                    subscribers: HashMap::new(),
                    watchers: Default::default(),
                    exited: false,
                    drained: false,
                    status: None,
                    exit_code: None,
                    title: handed.title,
                    cell,
                    input_at: None,
                    marks: crate::command_marks::Marks::after_handoff(),
                    reports: Default::default(),
                    pwd: String::new(),
                },
                reader,
                replies,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    // No ACK yet: failure while preparing the store, watcher or listening socket must
    // leave the old host alive. Dropping this connection makes it resume serving.
    Ok((stream, taken))
}
/// What starting would trip over that does not wait on the old host leaving: an entry
/// where either listening socket goes that is not one.
fn ready_to_take_over(root: &Path) -> io::Result<()> {
    for name in ["workbench.sock", ".sock-start"] {
        if local::at_path(&root.join(name))? == local::AtPath::Other {
            return Err(io::Error::other(format!(
                "not taking over: {name} in the runtime is not this user's socket"
            )));
        }
    }
    Ok(())
}
pub(super) fn serve_runtime(
    root: &Path,
    roots: Vec<(Agent, std::path::PathBuf)>,
    recent_days: u64,
    takeover: bool,
) -> io::Result<()> {
    if !std::sync::LazyLock::force(&BUILD).launch.is_empty() {
        // Every terminal inherits it: say so where whoever started it will look.
        eprintln!(
            "fibo: not started by the window ({:?}); its terminals inherit this. Open the app to start the host itself",
            BUILD.launch
        );
    }
    fibo_platform::fs::secure_dir(root)?;
    let instance_lock = fibo_platform::fs::lock_file(&root.join("host.lock"))?;
    if !takeover {
        instance_lock
            .try_lock()
            .map_err(|_| io::Error::other("a daemon already owns this runtime"))?;
    }
    ready_to_take_over(root)?;
    let (handoff, taken) = if takeover {
        let (stream, taken) = take_over(root)?;
        (Some(stream), taken)
    } else {
        (None, vec![])
    };
    let socket = root.join("workbench.sock");
    // Without it hooks fall back to the process tree alone.
    if let Err(e) = pty::announce_host(root) {
        eprintln!("announce host to terminals: {e}");
    }
    // Without it shells still start; only their directory reports are missing.
    if let Err(e) = pty::install_shell_integration(root) {
        eprintln!("install shell integration: {e}");
    }
    let kept = taken.iter().map(|t| t.id.clone()).collect();
    let (store, mut workspace) = Store::open_keeping(root, &kept)?;
    let mut index = crate::log_index::Index::open(root, roots)?;
    index.recent_days = recent_days;
    for s in &workspace.agent_sessions {
        if let Some(card) = index.card(&s.transcript)? {
            workspace
                .content
                .insert(s.id.clone(), std::sync::Arc::new(card));
        }
    }
    // Only the working set stays in memory; the store keeps the rest (fibo_core::retention).
    fibo_core::retention::archive(
        &mut workspace,
        fibo_core::content::now(),
        &Default::default(),
    );
    let host = Arc::new(Mutex::new(Host {
        remote: None,
        store,
        index: index.fork(root)?,
        transcripts: Arc::new(Mutex::new(index.fork(root)?)),
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
        workspace,
        terminals: HashMap::new(),
        closed: Default::default(),
        instance: uuid::Uuid::new_v4().to_string(),
    }));
    lock(&host)?.myself = Arc::downgrade(&host);
    let weak = Arc::downgrade(&host);
    lock(&host)?.exits = Some(fibo_platform::exit_watch::ExitWatch::new(move |pid| {
        if let Some(host) = weak.upgrade() {
            if let Err(e) = lock(&host).and_then(|mut h| h.agent_exited(pid)) {
                eprintln!("persist Agent exit: {e}");
            }
        }
    })?);
    // Prepare the replacement listener without unlinking the old host's public socket.
    // A failing VT, store read, watcher or bind above/below therefore has no cutover.
    let pending_socket = root.join(".sock-start");
    if local::at_path(&pending_socket)? == local::AtPath::MySocket {
        fs::remove_file(&pending_socket)?;
    }
    let listener = local::Listener::bind(&pending_socket)?;
    local::restrict(&pending_socket)?;
    if let Some(mut stream) = handoff {
        write_frame(&mut stream, ACK, HANDOFF_TAKEN)?;
        let until = Instant::now() + Duration::from_secs(10);
        while instance_lock.try_lock().is_err() {
            if Instant::now() >= until {
                return Err(io::Error::other(
                    "previous daemon did not release the runtime",
                ));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    wait_log::start(root);
    super::exit_log::start(root);
    for t in taken {
        let pid = t.live.pid;
        let mut guard = lock(&host)?;
        run_live(
            &mut guard,
            &host,
            t.id,
            t.live,
            t.reader,
            t.replies,
            Reaper::Pid(pid),
        );
    }
    // A terminal the older host started with an Agent may still wait on it, and nothing here
    // announces that Agent either: no Enter, and a CLI resuming a conversation writes nothing
    // to its log until it is spoken to. One still loading when the terminals were handed over
    // is looked for as at its launch, or it ties nothing until something else changes.
    let waiting: Vec<String> = (lock(&host)?.workspace.agent_starts.keys())
        .filter(|terminal| kept.contains(*terminal))
        .cloned()
        .collect();
    for terminal in waiting {
        super::launch_probe::look_for_started(&host, &terminal);
    }
    start_deadlines(&host)?;
    archive::start_archiving(&host);
    shell_watch::start(&host)?;
    start_subscription(Arc::clone(&host), index);
    {
        let weak = Arc::downgrade(&host);
        let supervisor = remote::Supervisor::start(root, move || {
            if let Some(host) = weak.upgrade() {
                if let Err(e) = lock(&host).and_then(|mut h| h.remote_changed()) {
                    eprintln!("publish remote status: {e}");
                }
            }
        });
        let mut h = lock(&host)?;
        h.remote = Some(supervisor);
        if let Err(e) = h.remote_changed() {
            eprintln!("publish remote status: {e}");
        }
    }
    // bind(2) creates a pathname before listen(2): publish only after startup is ready.
    fs::rename(&pending_socket, &socket)?;
    eprintln!("fibo listening on {}", socket.display());
    for index in 1u64.. {
        let stream = listener.accept()?;
        let host = Arc::clone(&host);
        thread::spawn(move || {
            if let Err(e) = serve_client(stream, host, index) {
                if !matches!(
                    e.kind(),
                    io::ErrorKind::UnexpectedEof
                        | io::ErrorKind::BrokenPipe
                        | io::ErrorKind::ConnectionReset
                ) {
                    eprintln!("client disconnected: {e}");
                }
            }
        });
    }
    Ok(())
}
