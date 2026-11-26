use super::*;

/// docs/decisions/2026-11-26-two-relays.md: the host stays on two relays at once, a phone
/// reaches it through either, and one relay going away leaves the other working.
#[test]
fn the_host_stays_on_two_relays_and_a_phone_gets_through_either() {
    use fibo_core::protocol::{RemoteState, RemoteStatus};
    use serde_json::json;
    let h = Host::start();
    h.setup();
    let root = h.root.to_string_lossy().into_owned();
    let cli = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_fibo"))
            .arg("remote")
            .args(args)
            .args(["--runtime", &root])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    let until = |what: &str, ok: &dyn Fn(&RemoteStatus) -> bool| -> RemoteStatus {
        let start = Instant::now();
        loop {
            let status = recognition(&h).remote;
            if ok(&status) {
                return status;
            }
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "{what}: {status:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let keyed = |key: &str| fibo_relay::server::Config {
        key: Some(key.into()),
        ..Default::default()
    };
    let (a, stop_a, relay_a) = local_relay("127.0.0.1:0", keyed("ka"));
    let (b, _stop_b, _relay_b) = local_relay("127.0.0.1:0", keyed("kb"));

    // Each relay has its own key; a wrong one stops only that relay.
    cli(&[
        "--enable",
        "--relay",
        &a,
        "--relay-key",
        "ka",
        "--also",
        &b,
        "--also-key",
        "wrong",
    ]);
    let half = until("one up, one refused", &|s| {
        s.live.state == RemoteState::Connected && s.live.detail.contains("拒绝")
    });
    assert_eq!(half.also, std::slice::from_ref(&b));
    cli(&["--enable", "--relay", &a, "--also", &b, "--also-key", "kb"]);
    until("both up", &|s| {
        s.live.state == RemoteState::Connected && s.live.detail.is_empty()
    });
    assert!(
        cli(&["--status"]).contains(&b),
        "status names the second relay"
    );

    // The pairing link names both relays; hello names them for phones paired before.
    let Reply::Text(text) = h
        .call(Request::RemotePair {
            name: "iPhone".into(),
            role: "full".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    let link = fibo_remote::link::Link::parse(&text).unwrap();
    assert_eq!(
        (link.relay.as_str(), link.alts.clone()),
        (a.as_str(), vec![b.clone()])
    );
    let via_b = fibo_remote::link::Link {
        relay: b.clone(),
        alts: vec![],
        ..link.clone()
    };
    for (n, link) in [&link, &via_b].into_iter().enumerate() {
        let mut phone = Phone::connect(link);
        phone.say(json!({ "id": n, "op": "hello", "version": fibo_remote::protocol::VERSION }));
        let hello = phone.next(|v| v["id"] == n);
        assert_eq!(hello["ok"]["relays"], json!([a, b]), "{hello}");
    }

    // Relay A goes away: phones still get through B, and the app says which one is down.
    drop(stop_a);
    relay_a.join().unwrap();
    let down = until("A down", &|s| !s.live.detail.is_empty());
    assert_eq!(down.live.state, RemoteState::Connected, "{down:?}");
    assert!(
        down.live.detail.contains(a.trim_start_matches("ws://")),
        "{down:?}"
    );
    let mut phone = Phone::connect(&via_b);
    phone.say(json!({ "id": 9, "op": "hello", "version": fibo_remote::protocol::VERSION }));
    phone.next(|v| v["id"] == 9);

    // `--also ""` takes the second relay off again.
    cli(&["--enable", "--relay", &a, "--also", ""]);
    let one = until("one relay", &|s| s.also.is_empty());
    assert!(
        !fs::read_to_string(h.root.join("remote-relay.json"))
            .unwrap()
            .contains("also"),
        "{one:?}"
    );
}
