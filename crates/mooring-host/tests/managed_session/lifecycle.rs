use super::*;

#[test]
/// The revision moves with every line an Agent writes; no edit waits on it. An edit fails
/// only on its own object: gone, or (outcomes) changed since its sender looked.
fn edits_ignore_the_workspace_revision() {
    let h = Host::start();
    h.setup();
    h.call(h.launch("one", "sleep 30")).unwrap();
    let stale = h.workspace().revision;
    h.call(Request::Edit {
        request: None,
        edit: Edit::CreateTab {
            tab: Tab {
                id: "t".into(),
                space: "s".into(),
                default_task: None,
                title: "Test".into(),
                layout: Some(Layout::Leaf("one".into())),
            },
        },
    })
    .unwrap();
    assert!(h.workspace().revision > stale);
    h.call(Request::Edit {
        request: None,
        edit: Edit::CloseTab {
            id: "t".into(),
            expect: None,
        },
    })
    .unwrap();
    assert!(h.workspace().tabs.iter().all(|t| t.id != "t"));
    let missing = h
        .call(Request::Edit {
            request: None,
            edit: Edit::CloseTab {
                id: "gone".into(),
                expect: None,
            },
        })
        .unwrap_err();
    assert!(missing.to_string().contains("标签不存在"), "{missing}");
    let conflict = h
        .call(Request::Edit {
            request: None,
            edit: Edit::SetTaskOutcome {
                seen: None,
                id: "gone".into(),
                outcome: Outcome::Abandoned,
            },
        })
        .unwrap_err();
    assert!(conflict.to_string().contains("待办不存在"), "{conflict}");
}
#[test]
fn launch_is_durable_and_deduplicated_and_tab_close_stops_and_deletes() {
    let h = Host::start();
    h.setup();
    h.call(h.launch("one", "sleep 30")).unwrap();
    let pid = h.workspace().terminals[0].pid;
    h.call(h.launch("one", "sleep 30")).unwrap();
    assert_eq!(h.workspace().terminals.len(), 1);
    assert_eq!(h.workspace().terminals[0].pid, pid);
    assert!(h.call(h.launch("one", "sleep 31")).is_err());
    h.call(Request::Edit {
        request: None,
        edit: Edit::CreateTab {
            tab: Tab {
                id: "t".into(),
                space: "s".into(),
                default_task: None,
                title: "Test".into(),
                layout: Some(Layout::Leaf("one".into())),
            },
        },
    })
    .unwrap();
    h.call(Request::Edit {
        request: None,
        edit: Edit::CloseTab {
            id: "t".into(),
            expect: None,
        },
    })
    .unwrap();
    wait_for(&h, "closed tab's terminal exits", |w| {
        w.terminals[0].state == ProcessState::Exited
    });
    let connection = rusqlite::Connection::open(h.root.join("workbench.sqlite")).unwrap();
    let rows: i64 = connection
        .query_row(
            "SELECT (SELECT count(*) FROM tabs) + (SELECT count(*) FROM tab_layout_nodes)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0, "a closed tab leaves no rows behind");
    let count: i64 = connection
        .query_row("SELECT count(*) FROM agent_sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "a shell is not an Agent conversation");
}
/// What an interactive zsh does with an Agent: the shell ignores SIGTERM and runs the
/// program as its own foreground job, so a signal to the shell's group misses it.
#[test]
fn stopping_a_job_control_shell_ends_it_and_its_foreground_job() {
    let h = Host::start();
    h.setup();
    h.call(h.launch("job", "trap '' TERM; set -m; sleep 300; :"))
        .unwrap();
    let shell = h.workspace().terminals[0].pid.unwrap();
    let job = (0..100)
        .find_map(|_| {
            let out = std::process::Command::new("pgrep")
                .args(["-P", &shell.to_string()])
                .output()
                .unwrap();
            let pid = String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<i32>()
                .ok();
            if pid.is_none() {
                std::thread::sleep(Duration::from_millis(50));
            }
            pid
        })
        .expect("foreground job started");
    assert_ne!(
        unsafe { libc::getpgid(job) },
        shell as i32,
        "the job runs in its own process group"
    );
    h.call(Request::Stop { id: "job".into() }).unwrap();
    wait_for(&h, "stopped shell exits", |w| {
        w.terminals[0].state == ProcessState::Exited
    });
    let until = Instant::now() + Duration::from_secs(5);
    while unsafe { libc::kill(job, 0) } == 0 {
        assert!(
            Instant::now() < until,
            "the foreground job outlived its terminal"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[test]
fn process_exit_does_not_complete_user_work() {
    let h = Host::start();
    h.setup();
    h.call(h.launch("one", "printf done; exit 7")).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let w = h.workspace();
        if w.terminals[0].state == ProcessState::Exited {
            assert_eq!(w.terminals[0].exit_code, Some(7));
            assert!(w.agent_sessions.is_empty());
            break;
        }
        assert!(Instant::now() < until, "state {:?}", w.terminals[0].state);
        std::thread::sleep(Duration::from_millis(20));
    }
    let (_, _, writable) = h.attach("one", false);
    assert!(!writable);
}
#[test]
fn a_second_daemon_cannot_replace_the_live_socket() {
    let h = Host::start();
    let result = fixture_command(&h.root).output().unwrap();
    assert!(!result.status.success());
    assert!(h.call(Request::Workspace).is_ok());
}

#[test]
fn input_control_follows_live_connections_without_takeover_or_polling() {
    let h = Host::start();
    h.setup();
    h.call(h.launch("control", "stty -echo; cat")).unwrap();
    let (first, _, writable) = h.attach("control", false);
    assert!(writable);
    let (mut second, _, writable) = h.attach("control", false);
    assert!(!writable, "a live owner must be respected");
    // Reads block until the disconnect event promotes this already-connected reader.
    drop(first);
    loop {
        let (kind, data) = read_frame(&mut second).unwrap();
        if kind == WRITABLE && serde_json::from_slice::<bool>(&data).unwrap() {
            break;
        }
    }
    send(
        &mut second,
        CONTROL,
        &Request::Input {
            bytes: b"promoted\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut second), Reply::Ok));
    drop(second);
    let (mut reopened, _, writable) = h.attach("control", false);
    if !writable {
        loop {
            let (kind, data) = read_frame(&mut reopened).unwrap();
            if kind == WRITABLE && serde_json::from_slice::<bool>(&data).unwrap() {
                break;
            }
        }
    }
    send(
        &mut reopened,
        CONTROL,
        &Request::Input {
            bytes: b"reopened\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut reopened), Reply::Ok));
}

#[test]
fn nested_split_tree_and_ratios_survive_store_reopen() {
    let mut h = Host::start();
    h.setup();
    for id in ["a", "b", "c", "d"] {
        h.call(h.launch(id, "sleep 30")).unwrap();
    }
    let mut layout = Layout::Leaf("a".into());
    layout.split("a", "b".into(), Axis::Right);
    layout.split("a", "c".into(), Axis::Down);
    layout.split("b", "d".into(), Axis::Down);
    if let Layout::Split { ratio, .. } = &mut layout {
        *ratio = 6300;
    }
    if let Some(Layout::Split { ratio, .. }) = layout.at_mut(&[true]) {
        *ratio = 2700;
    }
    let tab = Tab {
        id: "tree".into(),
        space: "s".into(),
        default_task: None,
        title: "Nested".into(),
        layout: Some(layout.clone()),
    };
    h.call(Request::Edit {
        request: None,
        edit: Edit::CreateTab { tab },
    })
    .unwrap();
    // Stop only this isolated fixture's processes before restarting its store.
    for id in ["a", "b", "c", "d"] {
        h.call(Request::Stop { id: id.into() }).unwrap();
    }
    h.child.kill().unwrap();
    h.child.wait().unwrap();
    h.child = fixture_command(&h.root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn_tied()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    let workspace = loop {
        if let Ok(Reply::Workspace(w)) = h.call(Request::Workspace) {
            break w;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(workspace.tabs[0].layout, Some(layout));
    let db = rusqlite::Connection::open(h.root.join("workbench.sqlite")).unwrap();
    let nodes: u32 = db
        .query_row(
            "SELECT count(*) FROM tab_layout_nodes WHERE tab_id='tree'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(nodes, 7);
}

#[test]
fn split_directory_comes_from_the_live_shell_after_cd() {
    let h = Host::start();
    h.setup();
    let changed = h.root.join("changed-directory");
    fs::create_dir(&changed).unwrap();
    let ready = h.root.join("cwd-ready");
    h.call(h.launch(
        "cwd",
        &format!(
            "cd {}; printf ready > {}; read -r line",
            shell_quote(&changed.to_string_lossy()),
            shell_quote(&ready.to_string_lossy())
        ),
    ))
    .unwrap();
    wait_file(&ready);
    let Reply::Cwd(cwd) = h.call(Request::TerminalCwd { id: "cwd".into() }).unwrap() else {
        panic!("missing cwd")
    };
    assert_eq!(
        PathBuf::from(cwd).canonicalize().unwrap(),
        changed.canonicalize().unwrap()
    );
}

#[test]
fn large_unicode_input_survives_ack_disconnect_without_replay() {
    let h = Host::start();
    h.setup();
    let output = h.root.join("received");
    let ready = h.root.join("raw-ready");
    h.call(h.launch(
        "paste",
        &format!(
            "stty -echo -icanon; printf ready > {}; cat > {}",
            shell_quote(&ready.to_string_lossy()),
            shell_quote(&output.to_string_lossy())
        ),
    ))
    .unwrap();
    wait_file(&ready);
    let (mut first, _, writable) = h.attach("paste", false);
    assert!(writable);
    let bytes = "多行中文 日本語 한국어 e\u{301}\nsecond line\n"
        .repeat(2048)
        .into_bytes();
    send_input(&mut first, &bytes, false).unwrap();
    // Observe delivery independently, but deliberately never consume ACKs.
    // Closing before delivery could legitimately leave an unknown partial input.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if fs::metadata(&output).is_ok_and(|m| m.len() == bytes.len() as u64) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(25));
    }
    drop(first);
    let (_again, _, _) = h.attach("paste", true);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(fs::read(output).unwrap(), bytes);
}

#[test]
fn host_death_interrupts_pty_and_restart_preserves_no_fake_running_process() {
    let mut h = Host::start();
    h.setup();
    h.call(h.launch("dies", "exec cat")).unwrap();
    let pid = h.workspace().terminals[0].pid.unwrap();
    h.child.kill().unwrap();
    h.child.wait().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while unsafe { libc::kill(pid as i32, 0) } == 0 {
        assert!(Instant::now() < deadline, "PTY process survived host death");
        std::thread::sleep(Duration::from_millis(25));
    }
    h.child = fixture_command(&h.root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn_tied()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(Reply::Workspace(w)) = h.call(Request::Workspace) {
            assert_eq!(w.terminals[0].state, ProcessState::Interrupted);
            assert!(h.call(Request::Stop { id: "dies".into() }).is_err());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn socket_publication_waits_for_initialization_then_first_rpc_succeeds() {
    use std::os::unix::fs::DirBuilderExt;
    let lease = FixtureLease::Shared {
        _guard: FIXTURES.read().unwrap_or_else(|e| e.into_inner()),
    };
    let root = std::env::temp_dir().join(format!("kl-{}", uuid::Uuid::new_v4().simple()));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let db = rusqlite::Connection::open(root.join("workbench.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE startup_barrier(id INTEGER); BEGIN EXCLUSIVE;")
        .unwrap();
    let child = fixture_command(&root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn_tied()
        .unwrap();
    let h = Host {
        child,
        root,
        _lease: lease,
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    while !h.root.join("host.lock").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    // The daemon has entered initialization but cannot finish the database open.
    // The former code published the socket before this barrier and failed here.
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !h.socket().exists(),
        "a visible socket must mean ready, not merely bind called"
    );
    db.execute_batch("ROLLBACK").unwrap();
    while !h.socket().exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    // Exactly one connect/handshake/request; no retry can hide a readiness race.
    assert!(matches!(
        h.call(Request::Workspace).unwrap(),
        Reply::Workspace(_)
    ));
}

#[test]
fn every_terminal_tells_its_programs_which_terminal_and_host_they_run_in() {
    let h = Host::start();
    h.setup();
    let out = h.root.join("env-out");
    h.call(h.launch(
        "env",
        &format!(
            "printf '%s|%s|%s|%s' \"$MOORING_ENV\" \"$MOORING_TERMINAL\" \"$MOORING_RUNTIME\" \"${{MOORING_HOOK:+set}}\" > {}; sleep 5",
            shell_quote(&out.to_string_lossy())
        ),
    ))
    .unwrap();
    // The runtime as the host was given it: resolving links could overflow the socket path.
    assert_eq!(wait_file(&out), format!("1|env|{}|set", h.root.display()));
}

fn pty_fds(pid: u32) -> usize {
    let open = mooring_platform::inspect::open_files(&[pid]).unwrap();
    open.get(&pid)
        .into_iter()
        .flatten()
        .filter(|f| f.contains("ptmx") || f.contains("/dev/ttys") || f.contains("/dev/pts/"))
        .count()
}
fn close_in_tab(h: &Host, terminal: &str) {
    let tab = format!("tab-{terminal}");
    h.call(Request::Edit {
        request: None,
        edit: Edit::CreateTab {
            tab: Tab {
                id: tab.clone(),
                space: "s".into(),
                default_task: None,
                title: "Test".into(),
                layout: Some(Layout::Leaf(terminal.into())),
            },
        },
    })
    .unwrap();
    h.call(Request::Edit {
        request: None,
        edit: Edit::CloseTab {
            id: tab,
            expect: None,
        },
    })
    .unwrap();
}

/// A terminal whose tab was closed and
/// whose program ended must give its pty back. macOS has 511 ptys for the whole system.
#[test]
fn closed_and_exited_terminals_release_their_ptys() {
    let h = Host::start();
    h.setup();
    let pid = h.child.id();
    h.call(h.launch("warm", "sleep 60")).unwrap();
    let base = pty_fds(pid);
    for i in 0..8 {
        let id = format!("t{i}");
        h.call(h.launch(&id, "sleep 30")).unwrap();
        close_in_tab(&h, &id);
    }
    wait_for(&h, "closed tabs' terminals exit", |w| {
        w.terminals
            .iter()
            .filter(|t| t.id.starts_with('t'))
            .all(|t| t.state == ProcessState::Exited)
    });
    std::thread::sleep(Duration::from_secs(2));
    let after = pty_fds(pid);
    assert!(
        after <= base,
        "host holds {after} pty fds after closing 8 terminals (was {base} with one live)"
    );
}

/// orca#10475: closing a tab must end its program even when the
/// program ignores SIGHUP; Terminal.app revokes the tty, Mooring only signals.
#[test]
fn closing_a_tab_ends_a_program_that_ignores_hangup() {
    let h = Host::start();
    h.setup();
    let marker = h.root.join("hup-pid");
    h.call(h.launch(
        "one",
        &format!(
            "trap '' HUP; sleep 120 & echo $! > {}; wait",
            marker.display()
        ),
    ))
    .unwrap();
    let child: i32 = wait_file(&marker).trim().parse().unwrap();
    close_in_tab(&h, "one");
    let until = Instant::now() + Duration::from_secs(10);
    let mut alive = true;
    while Instant::now() < until {
        alive = unsafe { libc::kill(child, 0) } == 0;
        if !alive {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if alive {
        unsafe { libc::kill(child, libc::SIGKILL) };
    }
    assert!(
        !alive,
        "a SIGHUP-ignoring child of a closed tab is still running 10s later"
    );
}

/// A takeover that cannot finish starting must not take the terminals with it: the old
/// host lets go of them as soon as the new one says it has them, so whatever can be
/// checked first is checked before that.
#[test]
fn a_takeover_that_cannot_start_leaves_the_old_host_serving() {
    let mut h = Host::start();
    h.setup();
    h.call(h.launch("t", "sleep 600")).unwrap();
    let pid = h
        .workspace()
        .terminals
        .iter()
        .find(|t| t.id == "t")
        .and_then(|t| t.pid)
        .unwrap() as i32;
    // Something that is not a socket where the new host would bind.
    fs::write(h.root.join(".sock-start"), "not a socket").unwrap();
    let status = fixture_command(&h.root)
        .env("MOORING_FIXTURE_TAKEOVER", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "the takeover was expected to fail");
    std::thread::sleep(Duration::from_millis(300));
    assert!(h.child.try_wait().unwrap().is_none(), "the old host exited");
    assert_eq!(
        unsafe { libc::kill(pid, 0) },
        0,
        "the terminal's shell was hung up"
    );
    assert!(h.workspace().terminals.iter().any(|t| t.id == "t"));
}
