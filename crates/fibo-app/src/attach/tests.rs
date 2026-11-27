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
    )
}

#[test]
fn mismatched_relay_does_not_reject_ssh_or_change_its_identity() {
    let forward = Forward::new(
        Spec::new("dummy".into()),
        PathBuf::from("/unused/workbench.sock"),
        true,
    );
    forward.learn(Via::Ssh, answer("ssh-id"));
    forward.learn(Via::Relay, answer("different"));
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
    forward.learn(Via::Relay, answer("paired-before-ssh"));
    forward.learn(Via::Ssh, answer("different-ssh"));
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
    let socket = embedded.join("workbench.sock");
    assert_eq!(
        socket.as_os_str().len(),
        standalone.join("client/client.sock").as_os_str().len()
    );
    assert!(socket.as_os_str().len() < 104, "{}", socket.display());
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
    forward.learn(Via::Ssh, answer(""));
    forward.learn(Via::Relay, answer("machine-a"));
    for _ in 0..2 {
        let args = relay_argv(&forward, program).unwrap();
        assert!(args.windows(2).any(|kv| kv == ["--machine", "alias"]));
        assert!(
            args.windows(2)
                .any(|kv| kv == ["--expect-machine-id", "machine-a"])
        );
    }
    forward.learn(Via::Relay, answer(""));
    let args = relay_argv(&forward, program).unwrap();
    assert!(!args.iter().any(|arg| arg == "--expect-machine-id"));
}
