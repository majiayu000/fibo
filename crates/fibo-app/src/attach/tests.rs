use super::paths::{dir_name, machine_root};
use super::*;

#[test]
fn a_host_is_one_short_folder_of_its_own() {
    let alias = dir_name("alice-server");
    assert_ne!(
        dir_name("alice@server"),
        alias,
        "two destinations, one socket"
    );
    assert_eq!(alias, dir_name("alice-server"), "the same name every run");
    assert!(!dir_name("../x").contains(['/', '.']));
    // The longest path under it, the window's lock socket, stays under macOS's 104
    // bytes for a long host under a usual home.
    let root = Path::new("/Users/someone-long/.local/share/mooring-workbench/m")
        .join(dir_name(
            "lifcc@agent-os.tailc3193d.ts.net.example.internal",
        ))
        .join("client/client.sock");
    assert!(root.as_os_str().len() < 104, "{}", root.display());
}
#[test]
fn the_socket_is_asked_for_then_found_as_an_older_fibo_finds_it() {
    let spec = Spec {
        host: "far".into(),
        path: Some("/opt/fibo".into()),
        quiet: false,
        via: Via::Ssh,
        auto: false,
    };
    let argv = spec.ssh(&format!("{}; \"$p\" socket", spec.find_program()));
    assert_eq!(argv[0], "ssh");
    assert!(argv.contains(&"BatchMode=yes".to_owned()));
    assert!(argv.last().unwrap().starts_with("p='/opt/fibo'"));
}

fn answer(id: &str) -> Answer {
    (
        format!("/{id}/workbench.sock"),
        format!("/{id}"),
        "/bin/sh".into(),
        (id.into(), id.into()),
        None,
    )
}

#[test]
fn mismatched_relay_does_not_reject_ssh_or_change_its_identity() {
    let forward = Forward::new(
        Spec::new("dummy".into()),
        PathBuf::from("/unused/workbench.sock"),
        true,
    );
    forward.learn(Via::Ssh, answer("ssh-id")).unwrap();
    forward.learn(Via::Relay, answer("different")).unwrap();
    let ways = forward.ways.as_ref().unwrap();
    assert!(ways.relay_rejected());
    assert!(!ways.is_stopped());
    assert!(ready(&forward, Via::Ssh).is_ok());
    assert!(ready(&forward, Via::Ssh).is_ok(), "SSH can still restart");
    assert_eq!(forward.info().as_ref().unwrap().machine.0, "ssh-id");
}

#[test]
fn ssh_takes_over_a_mismatched_relay_that_never_served_a_workspace() {
    let forward = Forward::new(
        Spec::new("dummy".into()),
        PathBuf::from("/unused/workbench.sock"),
        true,
    );
    forward
        .learn(Via::Relay, answer("paired-before-ssh"))
        .unwrap();
    forward.learn(Via::Ssh, answer("different-ssh")).unwrap();
    let ways = forward.ways.as_ref().unwrap();
    assert!(ways.relay_rejected());
    assert!(!ways.is_stopped());
    assert_eq!(forward.info().as_ref().unwrap().machine.0, "different-ssh");
}

#[test]
fn a_cancelled_discovery_does_not_bind_or_reuse_its_replacement() {
    let root = std::env::temp_dir().join(format!("fibo-stale-forward-{}", std::process::id()));
    assert!(!root.exists());
    let socket = root.join("workbench.sock");
    let old = Arc::new(Forward::new(
        Spec::new("dummy".into()),
        socket.clone(),
        false,
    ));
    let attempt = Attempt::default();
    attempt.cancel();
    assert!(
        matches!(register(&old, &root, &attempt, None), Err(e) if e.kind() == io::ErrorKind::Interrupted)
    );
    assert!(!root.exists(), "no path created by a stale discovery");
    let replacement = Arc::new(Forward::new(Spec::new("dummy".into()), socket, false));
    FORWARDS.lock().unwrap().push(replacement.clone());
    assert!(
        matches!(register(&old, &root, &attempt, None), Err(e) if e.kind() == io::ErrorKind::Interrupted)
    );
    assert!(
        matches!(Host(Some(old)).workspace_connection(&Attempt::default()), Err(e) if e.kind() == io::ErrorKind::Interrupted)
    );
    let found = Host::of(&root).0.unwrap();
    assert!(Arc::ptr_eq(&found, &replacement));
    FORWARDS
        .lock()
        .unwrap()
        .retain(|f| !Arc::ptr_eq(f, &replacement));
}

#[test]
fn embedded_and_standalone_socket_names_do_not_overlap() {
    let runtime = Path::new("/Users/someone-long/.local/share/mooring-workbench");
    let host = "lifcc@agent-os.tailc3193d.ts.net.example.internal";
    let standalone = machine_root(runtime, host, None);
    assert_eq!(standalone, runtime.join("m").join(dir_name(host)));
    let embedded = machine_root(runtime, host, Some(u32::MAX));
    assert_ne!(standalone, embedded);
    assert_ne!(embedded, machine_root(runtime, host, Some(u32::MAX - 1)));
    assert_ne!(
        embedded,
        machine_root(runtime, "lifcc@another-host", Some(u32::MAX))
    );
    let runtime = PathBuf::from(format!("/{}/fibo-workbench", "x".repeat(54)));
    assert_eq!(runtime.as_os_str().len(), 70);
    for alias in ["x", "名", host] {
        let old = machine_root(&runtime, alias, None).join("workbench.sock");
        let new = machine_root(&runtime, alias, Some(u32::MAX)).join("workbench.sock");
        assert!(new.as_os_str().len() <= old.as_os_str().len());
        assert!(new.as_os_str().len() < 104, "{}", new.display());
    }
}

#[test]
fn the_relay_sidecar_keeps_the_machine_id_discovered_for_its_route() {
    let forward = Forward::new(
        Spec::new("alias".into()),
        PathBuf::from("/unused/workbench.sock"),
        true,
    );
    let program = Path::new("fibo");
    assert!(relay_argv(&forward, program).is_err());
    forward.learn(Via::Ssh, answer("machine-a")).unwrap();
    let (path, home, shell, machine, _) = answer("machine-a");
    forward
        .learn(Via::Relay, (path, home, shell, machine, Some(27)))
        .unwrap();
    for _ in 0..2 {
        let args = relay_argv(&forward, program).unwrap();
        assert!(args.windows(2).any(|kv| kv == ["--machine", "alias"]));
        assert!(args.windows(2).any(|kv| kv == ["--prefer-relay", "27"]));
        assert!(
            args.windows(2)
                .any(|kv| kv == ["--expect-machine-id", "machine-a"])
        );
    }
}

#[test]
fn automatic_relay_requires_an_identity_without_disabling_ssh() {
    let forward = Forward::new(
        Spec::new("alias".into()),
        PathBuf::from("/unused/workbench.sock"),
        true,
    );
    assert!(forward.learn(Via::Relay, answer("")).is_err());
    assert!(
        forward.info().is_none(),
        "never publish an unidentified relay"
    );
    let ways = forward.ways.as_ref().unwrap();
    assert!(ways.relay_rejected());
    assert!(!ways.is_stopped());
    forward.learn(Via::Ssh, answer("ssh-id")).unwrap();
    assert!(ready(&forward, Via::Ssh).is_ok());
    assert_eq!(forward.info().as_ref().unwrap().machine.0, "ssh-id");
    assert!(relay_argv(&forward, Path::new("fibo")).is_err());
}

#[test]
fn explicit_relay_can_still_use_a_pairing_without_a_machine_id() {
    let spec = Spec {
        via: Via::Relay,
        ..Spec::new("alias".into())
    };
    let forward = Forward::new(spec, PathBuf::from("/unused/workbench.sock"), false);
    forward.learn(Via::Relay, answer("")).unwrap();
    assert!(ready(&forward, Via::Relay).is_ok());
    let args = relay_argv(&forward, Path::new("fibo")).unwrap();
    assert!(!args.iter().any(|arg| arg == "--expect-machine-id"));
}

#[test]
fn relay_discovery_can_finish_after_the_ssh_deadline() {
    let forward = Forward::new(
        Spec::new("alias".into()),
        PathBuf::from("/unused/workbench.sock"),
        true,
    );
    let (_ssh_tx, ssh) = std::sync::mpsc::channel();
    let (relay_tx, relay) = std::sync::mpsc::channel();
    let ssh_until = Instant::now();
    let standby = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(25));
        let _ = relay_tx.send(Ok(answer("standby-machine")));
    });
    let pending = prepare_from_answers(&forward, &Attempt::default(), ssh, relay, ssh_until)
        .expect("the relay owns its standby deadline");
    assert!(
        pending.is_none(),
        "relay already answered when SSH was unavailable"
    );
    assert_eq!(
        forward.info().as_ref().unwrap().machine.0,
        "standby-machine"
    );
    standby.join().unwrap();
}

#[test]
fn a_known_ssh_identity_does_not_wait_for_relay_discovery() {
    let forward = Forward::new(
        Spec::new("alias".into()),
        PathBuf::from("/unused/workbench.sock"),
        true,
    );
    let (ssh_tx, ssh) = std::sync::mpsc::channel();
    ssh_tx.send(Ok(answer("ssh-machine"))).unwrap();
    let (relay_tx, relay) = std::sync::mpsc::channel();
    let pending = prepare_from_answers(&forward, &Attempt::default(), ssh, relay, Instant::now())
        .unwrap()
        .expect("SSH can proceed with the relay answer still pending");
    assert_eq!(forward.info().as_ref().unwrap().machine.0, "ssh-machine");
    relay_tx.send(Ok(answer("relay-finished"))).unwrap();
    assert_eq!(pending.recv().unwrap().unwrap().3.0, "relay-finished");
}

#[test]
fn cancelling_an_attempt_ends_a_pending_relay_discovery() {
    let attempt = Arc::new(Attempt::default());
    let (_relay_tx, relay) = std::sync::mpsc::channel();
    let (done, result) = std::sync::mpsc::channel();
    let waiting = attempt.clone();
    let worker = std::thread::spawn(move || {
        let _ = done.send(receive_about(relay, &waiting, None));
    });
    attempt.cancel();
    let error = result
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    worker.join().unwrap();
}
