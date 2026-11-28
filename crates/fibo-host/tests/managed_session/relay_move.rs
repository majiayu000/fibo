use super::*;

/// docs/decisions/2026-11-26-two-relays.md, 11-28: replacing the main relay keeps the old
/// one on until phones paired through it have connected once, so they learn the new one
/// instead of losing the computer.
#[test]
fn a_new_main_relay_keeps_the_old_one_until_paired_phones_have_moved() {
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
    let (a, _stop_a, _relay_a) = local_relay("127.0.0.1:0", Default::default());
    let (c, _stop_c, _relay_c) = local_relay("127.0.0.1:0", Default::default());
    cli(&["--enable", "--relay", &a]);
    until("on A", &|s| s.live.state == RemoteState::Connected);
    let Reply::Text(text) = h
        .call(Request::RemotePair {
            name: "iPhone".into(),
            role: "full".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    let old = fibo_remote::link::Link::parse(&text).unwrap();
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
