use super::*;

fn ms(v: &[Option<u64>]) -> Vec<Option<Duration>> {
    v.iter().map(|m| m.map(Duration::from_millis)).collect()
}

#[test]
fn the_way_taken_is_left_at_once_when_it_fails_and_the_other_answers() {
    let relay = ms(&[Some(139), Some(140), Some(138)]);
    assert_eq!(
        choose(Via::Ssh, &ms(&[Some(16), Some(17), None]), &relay),
        Via::Relay
    );
    // Neither answers: stay, nothing better to go to.
    assert_eq!(
        choose(Via::Ssh, &ms(&[None]), &ms(&[Some(1), None])),
        Via::Ssh
    );
}

#[test]
fn the_other_way_is_taken_only_after_winning_three_probes_by_a_fifth_and_5_ms() {
    let relay = ms(&[Some(139), Some(140), Some(138)]);
    assert_eq!(
        choose(Via::Relay, &ms(&[Some(16), Some(17), Some(15)]), &relay),
        Via::Ssh
    );
    // Two wins are not enough, and a lost one in between restarts the count.
    assert_eq!(
        choose(Via::Relay, &ms(&[Some(16), Some(17)]), &relay),
        Via::Relay
    );
    assert_eq!(
        choose(Via::Relay, &ms(&[Some(16), Some(150), Some(15)]), &relay),
        Via::Relay
    );
    // A fifth faster but under 5 ms, or 5 ms faster but not a fifth: stay.
    let near = ms(&[Some(10), Some(10), Some(10)]);
    assert_eq!(
        choose(Via::Ssh, &near, &ms(&[Some(7), Some(7), Some(7)])),
        Via::Ssh
    );
    let slow = ms(&[Some(100), Some(100), Some(100)]);
    assert_eq!(
        choose(Via::Ssh, &slow, &ms(&[Some(90), Some(90), Some(90)])),
        Via::Ssh
    );
}

fn echo(path: &Path, word: &'static [u8]) {
    let listener = local::Listener::bind(path).unwrap();
    std::thread::spawn(move || {
        while let Ok(mut s) = listener.accept() {
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut b = [0u8; 1];
                while s.read_exact(&mut b).is_ok() {
                    if s.write_all(word).is_err() {
                        return;
                    }
                }
            });
        }
    });
}

fn ask(s: &mut local::Stream) -> Option<Vec<u8>> {
    use std::io::{Read, Write};
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    s.write_all(b"?").ok()?;
    let mut b = [0u8; 5];
    s.read_exact(&mut b[..3]).ok()?;
    Some(b[..3].to_vec())
}

/// A short folder of its own: socket paths stay under the 104-byte limit.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from("/tmp").join(format!("fr-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_switch_closes_links_on_the_way_left_and_new_ones_take_the_new_way() {
    let dir = scratch(&format!("{}", line!()));
    let local = dir.join("workbench.sock");
    echo(&beside(&local, Via::Ssh), b"ssh");
    echo(&beside(&local, Via::Relay), b"rly");
    let ways = Arc::new(Ways::default());
    ways.probed(Via::Relay, Some(Duration::from_millis(139)));
    split(ways.clone(), local.clone(), true, Arc::new(|| false)).unwrap();

    let mut old = local::Stream::connect(&local).unwrap();
    assert_eq!(ask(&mut old).as_deref(), Some(&b"rly"[..]));
    for _ in 0..3 {
        ways.probed(Via::Relay, Some(Duration::from_millis(139)));
    }
    let mut moved = None;
    for _ in 0..3 {
        moved = ways
            .probed(Via::Ssh, Some(Duration::from_millis(16)))
            .or(moved);
    }
    assert_eq!(moved, Some(Via::Ssh));
    assert_eq!(ask(&mut old), None, "a link on the way left is closed");
    let mut new = local::Stream::connect(&local).unwrap();
    assert_eq!(ask(&mut new).as_deref(), Some(&b"ssh"[..]));
    let route = ways.route(true).unwrap();
    assert_eq!(route.via, Via::Ssh);
    assert_eq!(
        route.other,
        Some((Via::Relay, Ok(Duration::from_millis(139))))
    );
}

#[test]
fn a_way_whose_socket_is_gone_hands_new_links_to_the_other() {
    let dir = scratch(&format!("{}", line!()));
    let local = dir.join("workbench.sock");
    echo(&beside(&local, Via::Relay), b"rly");
    let ways = Arc::new(Ways::default());
    ways.probed(Via::Ssh, Some(Duration::from_millis(16)));
    ways.probed(Via::Relay, Some(Duration::from_millis(139)));
    split(ways.clone(), local.clone(), true, Arc::new(|| false)).unwrap();
    let mut s = local::Stream::connect(&local).unwrap();
    assert_eq!(ask(&mut s).as_deref(), Some(&b"rly"[..]));
    // Without a relay pairing it is not tried.
    let ways = Arc::new(Ways::default());
    ways.probed(Via::Ssh, Some(Duration::from_millis(16)));
    ways.probed(Via::Relay, Some(Duration::from_millis(139)));
    let only = dir.join("only/workbench.sock");
    std::fs::create_dir_all(only.parent().unwrap()).unwrap();
    echo(&beside(&only, Via::Relay), b"rly");
    split(ways.clone(), only.clone(), false, Arc::new(|| false)).unwrap();
    let mut s = local::Stream::connect(&only).unwrap();
    assert_eq!(ask(&mut s), None);
    assert_eq!(
        ways.route(false).unwrap().other,
        Some((Via::Relay, Err(t!("没配对").to_owned())))
    );
}

#[test]
fn a_route_switch_during_connect_registers_only_the_current_route() {
    let ways = Ways::default();
    ways.probed(Via::Relay, Some(Duration::from_millis(139)));
    ways.probed(Via::Ssh, Some(Duration::from_millis(16)));
    let (inbound, _client) = local::Stream::pair().unwrap();
    let tried = std::cell::RefCell::new(Vec::new());
    let registered = register(&ways, true, &inbound, |via| {
        tried.borrow_mut().push(via);
        if via == Via::Relay {
            assert_eq!(ways.probed(Via::Relay, None), Some(Via::Ssh));
        }
        Some(local::Stream::pair().unwrap().0)
    });
    assert!(registered.is_some());
    assert_eq!(*tried.borrow(), [Via::Relay, Via::Ssh]);
    assert_eq!(ways.state.lock().unwrap().links[0].1, Via::Ssh);
}

#[test]
fn a_rejected_relay_drops_existing_links_and_is_never_a_fallback() {
    let dir = scratch(&format!("{}", line!()));
    let path = dir.join("workbench.sock");
    echo(&beside(&path, Via::Relay), b"rly");
    let ways = Arc::new(Ways::default());
    ways.probed(Via::Relay, Some(Duration::from_millis(11)));
    ways.probed(Via::Ssh, Some(Duration::from_millis(16)));
    split(ways.clone(), path.clone(), true, Arc::new(|| false)).unwrap();
    let mut old = local::Stream::connect(&path).unwrap();
    assert_eq!(ask(&mut old).as_deref(), Some(&b"rly"[..]));
    ways.reject_relay("pairing mismatch".to_owned(), false);
    assert_eq!(ask(&mut old), None);
    ways.probed(Via::Relay, Some(Duration::from_millis(1)));
    assert_eq!(
        ways.route(true).unwrap().other,
        Some((Via::Relay, Err("pairing mismatch".into())))
    );
    let mut no_ssh = local::Stream::connect(&path).unwrap();
    assert_eq!(
        ask(&mut no_ssh),
        None,
        "never fall back to the rejected listener"
    );
    echo(&beside(&path, Via::Ssh), b"ssh");
    assert_eq!(
        ask(&mut local::Stream::connect(&path).unwrap()).as_deref(),
        Some(&b"ssh"[..])
    );
}

#[test]
fn startup_waits_for_a_supported_workbench_hello() {
    use fibo_core::protocol::{CONTROL, MAJOR, Reply, Request, receive, send};
    let dir = scratch(&format!("{}", line!()));
    let socket = dir.join("ssh.sock");
    let listener = local::Listener::bind(&socket).unwrap();
    let serve = std::thread::spawn(move || {
        for hello in [
            Reply::Ok,
            Reply::Hello {
                major: MAJOR + 1,
                instance: String::new(),
                build: Default::default(),
            },
            Reply::Hello {
                major: MAJOR,
                instance: String::new(),
                build: Default::default(),
            },
        ] {
            let mut stream = listener.accept().unwrap();
            assert!(matches!(
                receive::<Request>(&mut stream).unwrap(),
                Request::Hello { major: MAJOR }
            ));
            send(&mut stream, CONTROL, &hello).unwrap();
            if matches!(hello, Reply::Hello { major: MAJOR, .. }) {
                assert!(
                    matches!(receive::<Request>(&mut stream).unwrap(), Request::TerminalCwd { id } if id.is_empty())
                );
                send(
                    &mut stream,
                    CONTROL,
                    &Reply::Error("no such terminal".into()),
                )
                .unwrap();
            }
        }
    });
    let ways = Ways::default();
    for _ in 0..2 {
        let (rtt, _) = round_trip(&socket, None, Duration::from_secs(2));
        ways.probed(Via::Ssh, rtt);
        assert!(!ways.wait_ready(Instant::now()));
    }
    let (rtt, _) = round_trip(&socket, None, Duration::from_secs(2));
    ways.probed(Via::Ssh, rtt);
    assert!(ways.wait_ready(Instant::now()));
    ways.stop();
    assert!(!ways.wait_ready(Instant::now()));
    serve.join().unwrap();
}

#[test]
fn a_stale_relay_identity_cannot_redirect_its_existing_workspace_to_ssh() {
    let ways = Ways::default();
    ways.probed(Via::Relay, Some(Duration::from_millis(139)));
    let (inbound, client) = local::Stream::pair().unwrap();
    assert!(
        register(&ways, true, &inbound, |_| Some(
            local::Stream::pair().unwrap().0
        ))
        .is_some()
    );
    ways.reject_relay("pairing mismatch".to_owned(), true);
    assert!(ways.is_stopped());
    assert!(!client.is_open());
    ways.probed(Via::Ssh, Some(Duration::from_millis(16)));
    assert!(
        register(&ways, true, &inbound, |_| panic!(
            "must not expose other identity"
        ))
        .is_none()
    );
}
