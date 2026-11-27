#![cfg(unix)]
use super::*;

#[test]
fn group_identity_requires_an_original_birth_time_and_current_membership() {
    let (mut child, _master) = pty::spawn(
        &["/bin/sh".into(), "-c".into(), "exec sleep 600".into()],
        Path::new("/tmp"),
        80,
        24,
        "group-identity-test",
        "15;0",
    )
    .unwrap();
    let pid = child.id();
    let at = fibo_platform::inspect::started_micros(pid).unwrap();
    let ours = has_original_member(pid as i32, &[(pid, at)]);
    let reused = has_original_member(pid as i32, &[(pid, at + 1)]);
    let other = std::process::id();
    let elsewhere = has_original_member(
        pid as i32,
        &[(
            other,
            fibo_platform::inspect::started_micros(other).unwrap(),
        )],
    );
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ours);
    assert!(!reused, "a PID with a different birth time is not ours");
    assert!(
        !elsewhere,
        "a live original member outside the group is not enough"
    );
    assert!(!has_original_member(pid as i32, &[(pid, at)]));
}

#[test]
fn a_reaped_leader_does_not_spare_its_hangup_ignoring_child() {
    let (mut leader, master) = pty::spawn(
        &[
            "/bin/sh".into(),
            "-c".into(),
            "trap 'exit 0' HUP; sh -c 'trap \"\" HUP; echo $$; exec sleep 600' & wait".into(),
        ],
        Path::new("/tmp"),
        80,
        24,
        "hangup-orphan-test",
        "15;0",
    )
    .unwrap();
    let group = leader.id() as i32;
    // Clean up this fixture even if an assertion fails, never another terminal's processes.
    struct Cleanup(i32);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fibo_platform::process::kill_group(self.0);
        }
    }
    let _cleanup = Cleanup(group);
    let mut reader = master.reader().unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    let mut output = Vec::new();
    while !output.contains(&b'\n') && Instant::now() < until {
        let mut bytes = [0; 128];
        if let term::Read::Data(n) = reader
            .next(&mut bytes, Some(Duration::from_millis(100)))
            .unwrap()
        {
            output.extend_from_slice(&bytes[..n]);
        }
    }
    let child: i32 = String::from_utf8(output).unwrap().trim().parse().unwrap();
    let terminal = terminal(leader.id(), master);
    stop_terminal(&terminal).unwrap();
    assert_eq!(leader.wait().unwrap(), Some(0));
    assert!(running(child), "the child must survive its leader's hangup");
    assert_eq!(fibo_platform::inspect::started_micros(group as u32), None);
    eprintln!(
        "leader {group} reaped; child {child} still running; foreground {:?}",
        lock(&terminal).unwrap().master.foreground_group()
    );
    // The tab and read pump can be dropped before the delayed stop runs.
    drop(reader);
    drop(terminal);
    let until = Instant::now() + HANGUP_GRACE + Duration::from_secs(3);
    while running(child) && Instant::now() < until {
        thread::sleep(Duration::from_millis(50));
    }
    assert!(!running(child), "the child outlived the delayed group kill");
    eprintln!("child {child} stopped after its leader was reaped");
}

fn running(pid: i32) -> bool {
    // A Linux container's PID 1 may leave an orphan zombie unreaped after SIGKILL.
    #[cfg(target_os = "linux")]
    if std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| {
            s.rsplit_once(')')
                .map(|(_, rest)| rest.trim_start().starts_with('Z'))
        })
        == Some(true)
    {
        return false;
    }
    fibo_platform::process::exists(pid)
}

#[test]
fn a_failed_hangup_still_force_stops_its_live_terminal() {
    let (mut child, master) = pty::spawn(
        &["/bin/sh".into(), "-c".into(), "exec sleep 600".into()],
        Path::new("/tmp"),
        80,
        24,
        "hangup-error-test",
        "15;0",
    )
    .unwrap();
    let terminal = terminal(child.id(), master);
    let began = Instant::now();
    let error = stop_terminal_with(&terminal, |_| {
        Err(io::Error::from_raw_os_error(libc::EPERM))
    })
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EPERM));
    let pid = child.id() as i32;
    assert!(
        fibo_platform::process::exists(pid),
        "the failed SIGHUP did not stop it"
    );
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait());
    });
    match rx.recv_timeout(HANGUP_GRACE + Duration::from_secs(3)) {
        Ok(exit) => assert_eq!(exit.unwrap(), None, "the terminal was killed by a signal"),
        Err(_) => {
            let _ = fibo_platform::process::kill_group(pid);
            let _ = rx.recv_timeout(Duration::from_secs(1));
            panic!("a failed hangup cancelled the delayed force stop");
        }
    }
    assert!(began.elapsed() >= HANGUP_GRACE);
}

#[test]
fn a_failed_final_exit_is_retried_without_another_notification() {
    let (mut host, root) = crate::host::tests::fixture();
    let id = "exit-retry";
    let cwd = root.to_str().unwrap().to_owned();
    let argv = vec!["/bin/sh".into(), "-c".into(), "exit 7".into()];
    let (mut child, master) = pty::spawn(&argv, &root, 80, 24, id, "15;0").unwrap();
    let terminal = terminal(child.id(), master);
    let exit_code = child.wait().unwrap();
    assert_eq!(exit_code, Some(7));
    let mut next = host.workspace.clone();
    next.spaces.push(Space {
        id: "s".into(),
        name: "fixture".into(),
        cwd: cwd.clone(),
    });
    next.terminals.push(Terminal {
        id: id.into(),
        space: "s".into(),
        argv,
        cwd,
        pid: Some(lock(&terminal).unwrap().pid),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    let mut agent = crate::host::tests::session("a", 0, Some(id), true);
    agent.transcript = "fixture.jsonl".into();
    next.agent_sessions.push(agent);
    host.commit(next).unwrap();

    let (tx, rx) = sync_channel(4);
    let (socket, _viewer) = local::Stream::pair().unwrap();
    {
        let mut live = lock(&terminal).unwrap();
        live.subscribers.insert(
            1,
            Subscriber {
                tx,
                socket,
                lagged: Default::default(),
            },
        );
        live.exited = true;
        live.exit_code = exit_code;
    }
    host.terminals.insert(id.into(), Arc::clone(&terminal));
    host.closed.insert(id.into());
    let shared = Arc::new(Mutex::new(host));
    super::super::runtime::start_deadlines(&shared).unwrap();

    // The child was reaped first. Its output reader is all that remains.
    record_exit(&shared, &terminal, id).unwrap();
    let (kind, status) = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert_eq!(kind, STATUS);
    let status: Terminal = serde_json::from_slice(&status).unwrap();
    assert_eq!(status.state, ProcessState::Draining);
    assert!(lock(&shared).unwrap().terminals.contains_key(id));
    record_exit(&shared, &terminal, id).unwrap();
    assert!(matches!(
        rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));

    // The final notification arrives just once, and SQLite rejects its transaction.
    lock(&shared).unwrap().store.refuse_next_commit();
    lock(&terminal).unwrap().drained = true;
    let er = record_exit(&shared, &terminal, id).unwrap_err();
    assert!(er.get_ref().unwrap().is::<rusqlite::Error>());
    let (kind, status) = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("no more PTY events can trigger this final STATUS");
    assert_eq!(kind, STATUS);
    let status: Terminal = serde_json::from_slice(&status).unwrap();
    assert_eq!(
        (status.state, status.exit_code),
        (ProcessState::Exited, Some(7))
    );

    {
        let host = lock(&shared).unwrap();
        assert!(!host.terminals.contains_key(id), "released the closed PTY");
        assert!(!host.closed.contains(id));
        assert!(!host.workspace.agent_sessions[0].current);
    }
    let (db, _) = Store::open(&root).unwrap();
    let stored = db.stored_terminal(id).unwrap().unwrap();
    assert_eq!(
        (stored.state, stored.exit_code),
        (ProcessState::Exited, Some(7))
    );
    record_exit(&shared, &terminal, id).unwrap();
    assert!(matches!(
        rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    drop((db, terminal, shared));
    fs::remove_dir_all(root).unwrap();
}

fn terminal(pid: u32, master: term::Master) -> LiveTerminal {
    Arc::new(Mutex::new(Live {
        vt: Vt::new(80, 24).unwrap(),
        input: Input::start(&master, "test").unwrap(),
        master,
        cols: 80,
        rows: 24,
        pid,
        owner: None,
        subscribers: HashMap::new(),
        watchers: Default::default(),
        exited: false,
        drained: false,
        status: None,
        exit_code: None,
        title: String::new(),
        cell: CellPixels::default(),
        input_at: None,
        marks: Default::default(),
        reports: Default::default(),
        pwd: String::new(),
    }))
}
