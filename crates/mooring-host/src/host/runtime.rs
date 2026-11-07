use super::*;
use mooring_platform::local;

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
                    let now = mooring_core::content::now();
                    let mut next = host.workspace.clone();
                    if next.expire_outcomes(now) {
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
                        at.saturating_sub(mooring_core::content::now()),
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
                        if outcome.is_none_or(|at| at > mooring_core::content::now()) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        let Some(shared) = weak.upgrade() else {
                            return Ok(());
                        };
                        let mut host = lock(&shared)?;
                        let mut next = host.workspace.clone();
                        next.expire_outcomes(mooring_core::content::now());
                        host.commit(next)?;
                    }
                }
            }
        };
        if let Err(e) = run() {
            eprintln!("outcome deadline: {e}");
        }
    });
    Ok(())
}
/// A terminal received from the previous host.
struct Taken {
    handed: Handed,
    snapshot: Vec<u8>,
    master: mooring_platform::terminal::Master,
}
fn take_over(root: &Path) -> io::Result<Vec<Taken>> {
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
    write_frame(&mut stream, ACK, HANDOFF_TAKEN)?;
    Ok(terminals
        .into_iter()
        .zip(snapshots)
        .zip(masters)
        .map(|((handed, snapshot), master)| Taken {
            handed,
            snapshot,
            master,
        })
        .collect())
}
/// What starting would trip over that does not wait on the old host leaving: an entry
/// where the listening socket goes that is not one, a store this user cannot read.
fn ready_to_take_over(root: &Path) -> io::Result<()> {
    for name in ["workbench.sock", ".sock-start"] {
        if local::at_path(&root.join(name))? == local::AtPath::Other {
            return Err(io::Error::other(format!(
                "not taking over: {name} in the runtime is not this user's socket"
            )));
        }
    }
    let db = rusqlite::Connection::open_with_flags(
        root.join("workbench.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| io::Error::other(format!("not taking over: the store does not open: {e}")))?;
    db.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map_err(|e| io::Error::other(format!("not taking over: the store does not read: {e}")))?;
    Ok(())
}
pub(super) fn serve_runtime(
    root: &Path,
    roots: Vec<(Agent, std::path::PathBuf)>,
    recent_days: u64,
    takeover: bool,
) -> io::Result<()> {
    std::sync::LazyLock::force(&BUILD);
    mooring_platform::fs::secure_dir(root)?;
    let taken = if takeover {
        // The old host lets go of every terminal once told they are taken, so what can
        // fail without it gone is tried first: failing later would hang them all up.
        ready_to_take_over(root)?;
        take_over(root)?
    } else {
        vec![]
    };
    let instance_lock = mooring_platform::fs::lock_file(&root.join("host.lock"))?;
    // After a handoff the old host exits on its own; wait for it to let go of the runtime.
    let until = Instant::now() + Duration::from_secs(if takeover { 10 } else { 0 });
    while instance_lock.try_lock().is_err() {
        if Instant::now() >= until {
            return Err(io::Error::other("a daemon already owns this runtime"));
        }
        thread::sleep(Duration::from_millis(20));
    }
    wait_log::start(root);
    let socket = root.join("workbench.sock");
    match local::at_path(&socket)? {
        local::AtPath::MySocket => fs::remove_file(&socket)?,
        local::AtPath::Nothing => (),
        local::AtPath::Other => {
            return Err(io::Error::other(
                "refusing to replace a non-socket runtime entry",
            ));
        }
    }
    // Without it hooks fall back to the process tree alone.
    if let Err(e) = pty::announce_host(root) {
        eprintln!("announce host to terminals: {e}");
    }
    // Without it shells still start; only their directory reports are missing.
    if let Err(e) = pty::install_shell_integration(root) {
        eprintln!("install shell integration: {e}");
    }
    let kept = taken.iter().map(|t| t.handed.id.clone()).collect();
    let (store, mut workspace) = Store::open_keeping(root, &kept)?;
    let mut index = crate::log_index::Index::open(root, roots)?;
    index.recent_days = recent_days;
    for s in &workspace.agent_sessions {
        if let Some(card) = index.card(&s.transcript)? {
            workspace.content.insert(s.id.clone(), card);
        }
    }
    let host = Arc::new(Mutex::new(Host {
        remote: None,
        store,
        index: index.fork(root)?,
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
        workspace,
        terminals: HashMap::new(),
        closed: Default::default(),
        instance: uuid::Uuid::new_v4().to_string(),
    }));
    lock(&host)?.myself = Arc::downgrade(&host);
    let weak = Arc::downgrade(&host);
    lock(&host)?.exits = Some(mooring_platform::exit_watch::ExitWatch::new(move |pid| {
        if let Some(host) = weak.upgrade() {
            if let Err(e) = lock(&host).and_then(|mut h| h.agent_exited(pid)) {
                eprintln!("persist Agent exit: {e}");
            }
        }
    })?);
    for t in taken {
        let mut vt = Vt::restore(&t.snapshot)?;
        let replies = Arc::new(Mutex::new(Vec::new()));
        let out = Arc::clone(&replies);
        vt.on_reply(move |bytes| {
            if let Ok(mut out) = out.lock() {
                out.extend_from_slice(bytes);
            }
        })?;
        let cell = CellPixels::default();
        cell.set(t.handed.cell.0, t.handed.cell.1);
        vt.answer_queries(cell.clone())?;
        let reader = t.master.reader()?;
        let mut guard = lock(&host)?;
        run_live(
            &mut guard,
            &host,
            t.handed.id,
            Live {
                vt,
                master: t.master,
                cols: t.handed.cols,
                rows: t.handed.rows,
                pid: t.handed.pid,
                owner: None,
                subscribers: HashMap::new(),
                watchers: Default::default(),
                exited: false,
                drained: false,
                status: None,
                exit_code: None,
                title: t.handed.title,
                cell,
                input_at: None,
                marks: Default::default(),
                reports: Default::default(),
            },
            reader,
            replies,
            Reaper::Pid(t.handed.pid),
        );
    }
    start_deadlines(&host)?;
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
        h.remote_changed()?;
    }
    // bind(2) creates the pathname before listen(2) makes it connectable. Under
    // scheduler pressure a client seeing that pathname can get ECONNREFUSED.
    // Publish only an initialized, listening socket, while holding host.lock.
    let pending_socket = root.join(".sock-start");
    match local::at_path(&pending_socket)? {
        local::AtPath::MySocket => fs::remove_file(&pending_socket)?,
        local::AtPath::Nothing => (),
        local::AtPath::Other => {
            return Err(io::Error::other(
                "refusing to replace pending non-socket entry",
            ));
        }
    }
    let listener = local::Listener::bind(&pending_socket)?;
    local::restrict(&pending_socket)?;
    fs::rename(&pending_socket, &socket)?;
    eprintln!("mooring listening on {}", socket.display());
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
