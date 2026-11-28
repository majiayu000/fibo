use super::*;

fn cli(h: &Host, args: &[&str]) {
    let out = Command::new(env!("CARGO_BIN_EXE_fibo"))
        .arg("remote")
        .args(args)
        .args(["--runtime", &h.root.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn until(
    h: &Host,
    what: &str,
    ok: &dyn Fn(&fibo_core::protocol::RemoteStatus) -> bool,
) -> fibo_core::protocol::RemoteStatus {
    let start = Instant::now();
    loop {
        let status = recognition(h).remote;
        if ok(&status) {
            return status;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "{what}: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn pair(h: &Host) -> fibo_remote::link::Link {
    let Reply::Text(text) = h
        .call(Request::RemotePair {
            name: "iPhone".into(),
            role: "full".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    fibo_remote::link::Link::parse(&text).unwrap()
}

/// docs/decisions/2026-11-26-two-relays.md, 11-28: replacing the main relay keeps the old
/// one on until phones paired through it have connected once, so they learn the new one
/// instead of losing the computer.
#[test]
fn a_new_main_relay_keeps_the_old_one_until_paired_phones_have_moved() {
    use fibo_core::protocol::{RemoteState, RemoteStatus};
    use serde_json::json;
    let h = Host::start();
    h.setup();
    let cli = |args: &[&str]| cli(&h, args);
    let until = |what: &str, ok: &dyn Fn(&RemoteStatus) -> bool| until(&h, what, ok);
    let (a, _stop_a, _relay_a) = local_relay("127.0.0.1:0", Default::default());
    let (c, _stop_c, _relay_c) = local_relay("127.0.0.1:0", Default::default());
    cli(&["--enable", "--relay", &a]);
    until("on A", &|s| s.live.state == RemoteState::Connected);
    let old = pair(&h);
    assert!(old.alts.is_empty(), "paired knowing A only");

    // The main relay moves to C; A stays on, waiting for this one phone.
    cli(&["--enable", "--relay", &c]);
    let moving = until("A kept", &|s| {
        s.moving.as_deref() == Some(a.as_str())
            && s.live.relays.len() == 2
            && s.live
                .relays
                .iter()
                .all(|x| x.state == RemoteState::Connected)
    });
    assert_eq!(moving.moving_left, 1);

    // The phone that knows only A still gets through, and is told only C.
    let mut phone = Phone::connect(&old);
    phone.say(json!({ "id": 1, "op": "hello", "version": fibo_remote::protocol::VERSION }));
    let hello = phone.next(|v| v["id"] == 1);
    assert_eq!(hello["ok"]["relays"], json!([c]), "{hello}");
    until("no phone left to move", &|s| s.moving_left == 0);
    drop(phone);

    // With every phone moved, the next save drops A.
    cli(&["--enable", "--relay", &c]);
    let done = until("A dropped", &|s| {
        s.moving.is_none() && s.live.state == RemoteState::Connected
    });
    assert!(done.live.relays.iter().all(|x| x.relay == c), "{done:?}");
    let learned = fibo_remote::link::Link {
        relay: c.clone(),
        alts: vec![],
        ..old
    };
    Phone::connect(&learned);
}

/// PR 675 review: the main relay changed A→B, then B→C while a phone that knows only A was
/// off. A stays on beside B, and that phone still gets through.
#[test]
fn changing_the_relay_twice_while_a_phone_is_off_keeps_its_way_back() {
    use fibo_core::protocol::RemoteState;
    use serde_json::json;
    let h = Host::start();
    h.setup();
    let (a, _stop_a, _relay_a) = local_relay("127.0.0.1:0", Default::default());
    let (b, _stop_b, _relay_b) = local_relay("127.0.0.1:0", Default::default());
    let (c, _stop_c, _relay_c) = local_relay("127.0.0.1:0", Default::default());
    cli(&h, &["--enable", "--relay", &a]);
    until(&h, "on A", &|s| s.live.state == RemoteState::Connected);
    let old = pair(&h);
    cli(&h, &["--enable", "--relay", &b]);
    until(&h, "A kept", &|s| s.moving.as_deref() == Some(a.as_str()));
    cli(&h, &["--enable", "--relay", &c]);
    let moving = until(&h, "A and B kept", &|s| {
        s.moving.as_deref() == Some(b.as_str())
            && s.moving_earlier == [a.clone()]
            && s.live.relays.len() == 3
            && (s.live.relays.iter()).all(|x| x.state == RemoteState::Connected)
    });
    assert_eq!(moving.moving_left, 1);

    let mut phone = Phone::connect(&old);
    phone.say(json!({ "id": 1, "op": "hello", "version": fibo_remote::protocol::VERSION }));
    let hello = phone.next(|v| v["id"] == 1);
    assert_eq!(hello["ok"]["relays"], json!([c]), "{hello}");
    until(&h, "no phone left to move", &|s| s.moving_left == 0);
    drop(phone);
    cli(&h, &["--enable", "--relay", &c]);
    until(&h, "A and B dropped", &|s| {
        s.moving.is_none() && s.moving_earlier.is_empty() && s.live.relays.len() <= 1
    });
}
