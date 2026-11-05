use super::*;

#[test]
fn a_phone_on_a_relay_drives_a_terminal_through_a_channel_the_relay_cannot_read() {
    use base64::Engine;
    use serde_json::json;
    let b64 = base64::engine::general_purpose::STANDARD;
    let h = Host::start();
    h.setup();
    h.call(h.launch(
        "rt",
        r"stty raw -echo; printf '\033[?1hready\r\n'; exec cat -v",
    ))
    .unwrap();
    let (_app, mut app_vt, _) = h.attach("rt", false);
    let (mut app, _, _) = h.attach("rt", false);
    read_until(&mut app, &mut app_vt, "ready");

    let (base, stop, relay) = local_relay("127.0.0.1:0", Default::default());
    let root = h.root.to_string_lossy().into_owned();
    let full = pair(&root, &base, "全权", "full");
    let viewer = pair(&root, &base, "只看的平板", "view");
    // Stage 4: each phone its own route and key; none carries the machine's code.
    assert_ne!(full.code, viewer.code, "one route per phone");
    assert_ne!(full.key, viewer.key);
    let machine = || {
        Command::new(env!("CARGO_BIN_EXE_mooring"))
            .args(["remote", "--relay", &base, "--runtime", &root])
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let mut on_relay = machine();
    let log = {
        let stderr = on_relay.stderr.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in io::BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        rx
    };

    // A paired device with the wrong key, and a device never paired, get nowhere.
    let mut forged = full.clone();
    forged.key = mooring_remote::secure::Code::generate().unwrap();
    assert_eq!(Phone::try_connect(&forged).err(), Some(Some(4401)));
    let mut stranger = full.clone();
    stranger.device = "000000000000".into();
    assert_eq!(Phone::try_connect(&stranger).err(), Some(Some(4401)));

    // A view-only device sees but cannot type, and is told so.
    let mut view = Phone::connect(&viewer);
    view.say(json!({ "id": 1, "op": "hello" }));
    let hello = view.next(|v| v["id"] == 1);
    assert_eq!(hello["ok"]["role"], "view");
    assert!(!hello["ok"]["ops"]
        .as_array()
        .unwrap()
        .contains(&json!("keys")));
    view.say(json!({ "id": 2, "op": "keys", "terminal": "rt", "keys": ["z"] }));
    assert!(view.next(|v| v["id"] == 2)["error"]
        .as_str()
        .unwrap()
        .contains("只看"));
    view.say(json!({ "id": 3, "op": "watch", "terminal": "rt" }));
    view.next(|v| v["stream"] == 3 && v["type"] == "screen");

    let mut phone = Phone::connect(&full);
    phone.say(json!({ "id": 1, "op": "hello", "version": 1 }));
    let hello = phone.next(|v| v["id"] == 1);
    let typed: mooring_remote::protocol::Hello =
        serde_json::from_value(hello["ok"].clone()).expect("hello matches the protocol types");
    assert_eq!(
        (typed.version, typed.role),
        (1, mooring_remote::role::Role::Full)
    );
    phone.say(json!({ "id": 2, "op": "list" }));
    let list = phone.next(|v| v["id"] == 2);
    let typed: mooring_remote::protocol::List =
        serde_json::from_value(list["ok"].clone()).expect("list matches the protocol types");
    assert!(typed
        .terminals
        .iter()
        .any(|c| c.id == "rt" && c.phase == mooring_remote::protocol::Phase::Shell));
    assert!(list["ok"]["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == "rt"));
    phone.say(json!({ "id": 3, "op": "watch", "terminal": "rt" }));
    phone.next(|v| v["id"] == 3);
    let screen = phone.next(|v| v["stream"] == 3 && v["type"] == "screen");
    let mut vt = Vt::new(80, 24).unwrap();
    vt.feed(&b64.decode(screen["vt"].as_str().unwrap()).unwrap());
    assert!(vt.tail_text(24).unwrap().contains("ready"));

    // Keystroke to echoed screen, round trip through the relay, twenty times.
    let mut trips = vec![];
    for (i, key) in "abcdefghijklmnopqrst".char_indices() {
        let sent = Instant::now();
        phone.say(
            json!({ "id": 100 + i, "op": "keys", "terminal": "rt", "keys": [key.to_string()] }),
        );
        let want: String = "abcdefghijklmnopqrst"[..=i].into();
        while !vt.tail_text(24).unwrap().contains(&want) {
            let out = phone.next(|v| v["stream"] == 3 && v["type"] == "output");
            vt.feed(&b64.decode(out["vt"].as_str().unwrap()).unwrap());
        }
        trips.push(sent.elapsed());
    }
    trips.sort();
    let p95 = trips[18];
    eprintln!(
        "phone keystroke → echo through the relay: p50 {:?} p95 {p95:?}",
        trips[10]
    );
    assert!(p95 < Duration::from_millis(200), "{trips:?}");

    // A new role drops the open session; back in, the device has the new role.
    let changed = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args([
            "remote",
            "--device",
            "只看的平板",
            "--role",
            "answer",
            "--runtime",
            &root,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        view.closed(Duration::from_secs(5)),
        Some(mooring_remote::close::ROLE_CHANGED)
    );
    eprintln!("role changed → session dropped in {:?}", changed.elapsed());
    let mut view = Phone::connect(&viewer);
    view.say(json!({ "id": 1, "op": "hello" }));
    assert_eq!(view.next(|v| v["id"] == 1)["ok"]["role"], "answer");
    view.say(json!({ "id": 2, "op": "keys", "terminal": "rt", "keys": ["z"] }));
    assert!(view.next(|v| v["id"] == 2)["error"]
        .as_str()
        .unwrap()
        .contains("能答审批"));

    // Unpairing the viewer drops its open session within the recheck, and it cannot return.
    let revoked = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["remote", "--revoke", "只看的平板", "--runtime", &root])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(view.closed(Duration::from_secs(5)), Some(4401));
    let took = revoked.elapsed();
    eprintln!("unpaired → session dropped in {took:?}");
    assert!(took < Duration::from_secs(3), "{took:?}");
    assert_eq!(Phone::try_connect(&viewer).err(), Some(Some(4401)));

    // Stage 4's acceptance: the unpaired phone, registering on the relay as a machine with
    // everything it was given, takes nothing from this one; the other phone carries on.
    let impostor = {
        use futures_util::SinkExt;
        let rt = Phone::rt();
        let hash = mooring_remote::wire::token_hash(&viewer.code.token());
        let url = format!("{base}/agent?th={hash}");
        let mut ws = rt
            .block_on(tokio_tungstenite::connect_async(url))
            .unwrap()
            .0;
        let register = mooring_remote::wire::Frame::Register {
            token_hash: hash,
            name: None,
            ver: None,
            key: None,
            proto: Some(3),
            retired: vec![],
            routes: vec![],
            revoked: vec![],
        };
        let text = tokio_tungstenite::tungstenite::Message::text(register.to_text());
        rt.block_on(ws.send(text)).unwrap();
        (rt, ws)
    };
    std::thread::sleep(Duration::from_millis(300));
    let mut still = Phone::connect(&full);
    still.say(json!({ "id": 1, "op": "hello" }));
    assert_eq!(still.next(|v| v["id"] == 1)["ok"]["role"], "full");
    drop((still, impostor));

    // A phone paired while the machine is on gets in once its route reaches the relay;
    // 「断开」 keeps it out for five minutes.
    let late = pair(&root, &base, "后来的", "view");
    let mut l = Phone::connect(&late);
    l.say(json!({ "id": 1, "op": "hello" }));
    l.next(|v| v["id"] == 1);
    let out = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["remote", "--kick", "后来的", "--runtime", &root])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        l.closed(Duration::from_secs(5)),
        Some(mooring_remote::close::KICKED)
    );
    assert_eq!(
        Phone::try_connect(&late).err(),
        Some(Some(mooring_remote::close::KICKED)),
        "back at once after 断开"
    );
    let listed = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["remote", "--devices", "--runtime", &root])
        .output()
        .unwrap();
    let listed = String::from_utf8(listed.stdout).unwrap();
    assert!(
        listed.contains("全权") && listed.contains("完全控制") && listed.contains("刚刚"),
        "{listed}"
    );
    assert!(!listed.contains("只看的平板"), "{listed}");
    let entries: Vec<serde_json::Value> = fs::read_to_string(h.root.join("remote-log.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let has = |event: &str, extra: &dyn Fn(&serde_json::Value) -> bool| {
        entries.iter().any(|v| v["event"] == event && extra(v))
    };
    assert!(has("connected", &|v| v["name"] == "全权"));
    assert!(has("refused", &|v| v["device"] == "000000000000"));
    assert!(has("dropped", &|v| v["device"] == viewer.device.as_str()));
    assert!(has("op", &|v| v["op"] == "keys"
        && v["name"] == "全权"
        && v["terminal"] == "rt"
        && v["error"].is_null()));
    assert!(
        !has("op", &|v| v["name"] == "只看的平板"),
        "a refused op is not an act"
    );

    // A second phone stops reading in the middle of a flood: it gets at most a window,
    // the first phone keeps typing at full speed, and the reader gets the screen again.
    h.call(h.launch(
        "fl",
        "i=0; while [ $i -lt 400 ]; do yes 0123456789abcdefghij | head -n 2000; i=$((i+1)); done; echo flood-done; exec cat",
    ))
    .unwrap();
    let mut stalled = Phone::connect(&full);
    stalled.say(json!({ "id": 1, "op": "hello" }));
    stalled.next(|v| v["id"] == 1);
    stalled.ack = false;
    stalled.say(json!({ "id": 2, "op": "watch", "terminal": "fl" }));
    while stalled.line(Duration::from_millis(500)).is_some() {}
    let held = stalled.received;
    assert!(
        held <= mooring_remote::secure::WINDOW + mooring_remote::secure::CHUNK as u64 * 2,
        "a phone that stopped acknowledging got {held} bytes"
    );
    let mut beside = vec![];
    for (i, key) in "uvwxy".char_indices() {
        let sent = Instant::now();
        phone.say(
            json!({ "id": 200 + i, "op": "keys", "terminal": "rt", "keys": [key.to_string()] }),
        );
        while !vt
            .tail_text(24)
            .unwrap()
            .contains(&format!("t{}", &"uvwxy"[..=i]))
        {
            let out = phone.next(|v| v["stream"] == 3 && v["type"] == "output");
            vt.feed(&b64.decode(out["vt"].as_str().unwrap()).unwrap());
        }
        beside.push(sent.elapsed());
    }
    let worst = *beside.iter().max().unwrap();
    eprintln!("beside a stalled phone ({held} bytes held): worst keystroke {worst:?}");
    assert!(worst < Duration::from_millis(200), "{beside:?}");
    stalled.resume();
    let until = Instant::now() + Duration::from_secs(20);
    let mut fl = Vt::new(80, 24).unwrap();
    loop {
        assert!(Instant::now() < until, "the stalled phone never caught up");
        let v = stalled.next(|v| v["stream"] == 2);
        if let Some(vt) = v["vt"].as_str() {
            if v["type"] == "screen" {
                fl = Vt::new(80, 24).unwrap();
            }
            fl.feed(&b64.decode(vt).unwrap());
        }
        if fl.tail_text(24).unwrap().contains("flood-done") {
            break;
        }
    }

    // The relay restarts: the machine dials again and a phone gets back in.
    let addr = base.trim_start_matches("ws://").to_owned();
    drop(stop);
    relay.join().unwrap();
    drop((phone, stalled));
    let (_base, _stop, _relay) = local_relay(&addr, Default::default());
    let mut phone = Phone::connect(&full);
    phone.say(json!({ "id": 1, "op": "hello" }));
    assert_eq!(phone.next(|v| v["id"] == 1)["ok"]["version"], 1);

    // A second machine with the same code takes over; the first one exits.
    let mut twin = machine();
    let until = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(s) = on_relay.try_wait().unwrap() {
            break s;
        }
        assert!(Instant::now() < until, "the replaced machine kept running");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(!status.success());
    let said: Vec<String> = log.try_iter().collect();
    assert!(said.iter().any(|l| l.contains("已连上中继")), "{said:?}");
    assert!(
        said.iter().any(|l| l.contains("没有配对或密钥不对")),
        "{said:?}"
    );
    let _ = twin.kill();
    let _ = twin.wait();
    drop(app);
}

#[test]
fn the_host_keeps_phones_on_a_relay_and_shows_the_app_who_is_on() {
    use mooring_core::protocol::{RemoteChange, RemoteState, RemoteStatus};
    use serde_json::json;
    let h = Host::start();
    h.setup();
    h.call(h.launch("rt", "stty raw -echo; echo ready; exec cat"))
        .unwrap();
    let root = h.root.to_string_lossy().into_owned();
    let cli = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_mooring"))
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
    let until = |what: &str, ok: &dyn Fn(&RemoteStatus) -> bool| -> (RemoteStatus, Duration) {
        let start = Instant::now();
        loop {
            let status = recognition(&h).remote;
            if ok(&status) {
                return (status, start.elapsed());
            }
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "{what}: {status:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let running = |pattern: &str| -> Vec<i32> {
        let out = Command::new("pgrep")
            .args(["-f", pattern])
            .output()
            .unwrap();
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect()
    };
    let (base, _stop, _relay) = local_relay(
        "127.0.0.1:0",
        mooring_relay::server::Config {
            key: Some("right".into()),
            ..Default::default()
        },
    );
    let ours = format!("remote --relay {base} --report --runtime {root}");

    // A refused machine stops trying and says why, until the setting changes.
    assert_eq!(recognition(&h).remote.live.state, RemoteState::Off);
    cli(&["--enable", "--relay", &base, "--relay-key", "wrong"]);
    let (failed, _) = until("refused", &|s| s.live.state == RemoteState::Failed);
    assert!(failed.live.detail.contains("拒绝"), "{failed:?}");
    cli(&["--enable", "--relay", &base, "--relay-key", "right"]);
    let (on, took) = until("connected", &|s| s.live.state == RemoteState::Connected);
    eprintln!("enabled → connected in {took:?}");
    assert!(on.enabled && on.relay == base);
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(h.root.join("remote-relay.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    // Pairing through the host, as the app does; the app sees the device at once.
    let Reply::Text(text) = h
        .call(Request::RemotePair {
            name: "iPhone".into(),
            role: "full".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    let link = mooring_remote::link::Link::parse(&text).unwrap();
    until("listed", &|s| {
        s.devices
            .iter()
            .any(|d| d.name == "iPhone" && d.role == "full")
    });

    // Who is on, what it watches and what it last did reach the app.
    let mut phone = Phone::connect(&link);
    phone.say(json!({ "id": 1, "op": "hello" }));
    phone.next(|v| v["id"] == 1);
    phone.say(json!({ "id": 2, "op": "watch", "terminal": "rt" }));
    phone.next(|v| v["stream"] == 2 && v["type"] == "screen");
    let (_, took) = until("watching", &|s| {
        s.live.online.len() == 1
            && s.live.online[0].device == link.device
            && s.live.online[0].watching == ["rt"]
    });
    eprintln!("watch → the app sees it in {took:?}");
    phone.say(json!({ "id": 3, "op": "keys", "terminal": "rt", "keys": ["x"] }));
    phone.next(|v| v["id"] == 3);
    until("acted", &|s| {
        s.live
            .online
            .first()
            .and_then(|p| p.last.as_ref())
            .is_some_and(|a| a.op == "keys" && a.terminal.as_deref() == Some("rt"))
    });

    // "Disconnect" ends its session; it stays paired and may come back.
    h.call(Request::RemoteDevice {
        device: link.device.clone(),
        change: RemoteChange::Kick,
    })
    .unwrap();
    assert_eq!(
        phone.closed(Duration::from_secs(5)),
        Some(mooring_remote::close::KICKED)
    );
    until("gone", &|s| {
        s.live.online.is_empty() && s.devices.len() == 1
    });
    // Stage 4: it stays out for five minutes, then may come back.
    assert_eq!(
        Phone::try_connect(&link).err(),
        Some(Some(mooring_remote::close::KICKED))
    );
    let file = h.root.join("remote-devices.json");
    let mut devices: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    let kicked = devices[0]["kicked"].as_u64().unwrap();
    devices[0]["kicked"] = json!(kicked - mooring_host::devices::KICK_COOLDOWN_MS - 1);
    fs::write(&file, devices.to_string()).unwrap();
    let mut phone = Phone::connect(&link);
    phone.say(json!({ "id": 1, "op": "hello" }));
    phone.next(|v| v["id"] == 1);
    until("back", &|s| s.live.online.len() == 1);

    // The process on the relay dies: the host starts it again and the phone returns.
    let pid = running(&ours)[0];
    unsafe { libc::kill(pid, libc::SIGKILL) };
    until("restarting", &|s| s.live.state != RemoteState::Connected);
    let (_, took) = until("restarted", &|s| s.live.state == RemoteState::Connected);
    eprintln!("killed → connected again in {took:?}");
    drop(phone);
    let mut phone = Phone::connect(&link);
    phone.say(json!({ "id": 1, "op": "hello" }));
    phone.next(|v| v["id"] == 1);

    // Off: the host lets go of the relay; the pairing stays.
    cli(&["--disable"]);
    let (off, _) = until("off", &|s| s.live.state == RemoteState::Off);
    assert!(!off.enabled && off.relay == base && off.devices.len() == 1);
    let start = Instant::now();
    while !running(&ours).is_empty() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the relay process kept running"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(phone.closed(Duration::from_secs(5)).is_some());
}

/// Anyone who has seen the pairing code can open phone sessions; one that never
/// finishes its handshake is closed instead of holding one of the machine's slots.
#[test]
fn a_phone_that_never_finishes_its_handshake_is_closed() {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    let h = Host::start();
    h.setup();
    let (base, stop, relay) = local_relay("127.0.0.1:0", Default::default());
    let root = h.root.to_string_lossy().into_owned();
    let link = pair(&root, &base, "手机", "full");
    let mut machine = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["remote", "--relay", &base, "--runtime", &root])
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let rt = Phone::rt();
    let url = link.url().unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    let mut ws = loop {
        match rt.block_on(tokio_tungstenite::connect_async(&url)) {
            Ok((ws, _)) => break ws,
            Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!("phone never reached the machine: {e}"),
        }
    };
    // Says nothing: no half of a handshake ever comes.
    let code = rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                match ws.next().await {
                    Some(Ok(Message::Close(f))) => return f.map(|f| u16::from(f.code)),
                    Some(Ok(_)) => continue,
                    _ => return None,
                }
            }
        })
        .await
    });
    let _ = machine.kill();
    let _ = machine.wait();
    drop(stop);
    let _ = relay.join();
    assert_eq!(
        code,
        Ok(Some(1000)),
        "an idle unpaired session stayed open: {code:?}"
    );
}
