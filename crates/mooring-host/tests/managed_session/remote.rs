use super::*;

/// The desktop's client of another machine (docs/multi-machine/spec.md, step 1): through
/// `mooring remote --stdio`, as ssh would run it, it lists the terminals and rebuilds a
/// watched screen from its replay, then follows the output after it.
#[test]
fn the_desktop_client_lists_and_watches_another_machine() {
    use base64::Engine;
    use mooring_remote::client::Client;
    use mooring_remote::protocol::{List, Op, StreamBody};
    let b64 = base64::engine::general_purpose::STANDARD;
    let h = Host::start();
    h.setup();
    h.call(h.launch(
        "far",
        r#"printf 'hello from mac2\r\n'; read -r line; printf 'later %s\r\n' "$line"; exec cat"#,
    ))
    .unwrap();
    let (mut app, mut app_vt, _) = h.attach("far", false);
    read_until(&mut app, &mut app_vt, "hello from mac2");
    let client = Client::spawn(&[
        env!("CARGO_BIN_EXE_mooring").to_owned(),
        "remote".into(),
        "--stdio".into(),
        "--runtime".into(),
        h.root.to_string_lossy().into_owned(),
    ])
    .unwrap();
    let t = Duration::from_secs(10);
    client
        .request(
            Op::Hello {
                version: Some(1),
                client: None,
            },
            t,
        )
        .unwrap();
    let list: List = serde_json::from_value(client.request(Op::List, t).unwrap()).unwrap();
    assert!(list.terminals.iter().any(|c| c.id == "far"), "{list:?}");
    let (tx, rx) = std::sync::mpsc::channel();
    client
        .stream(
            Op::Watch {
                terminal: "far".into(),
                take: false,
            },
            move |body| {
                let _ = tx.send(body);
            },
            t,
        )
        .unwrap();
    let mut vt = None;
    let until = Instant::now() + Duration::from_secs(10);
    let shown = |vt: &Option<Vt>| {
        vt.as_ref()
            .map(|v: &Vt| v.tail_text(24).unwrap())
            .unwrap_or_default()
    };
    while !shown(&vt).contains("hello from mac2") {
        assert!(
            Instant::now() < until,
            "screen never came: {:?}",
            shown(&vt)
        );
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(StreamBody::Screen {
                cols,
                rows,
                vt: replay,
            }) => {
                let mut fresh = Vt::new(cols, rows).unwrap();
                fresh.feed(&b64.decode(replay).unwrap());
                vt = Some(fresh);
            }
            Ok(StreamBody::Output { vt: bytes }) => {
                if let Some(v) = vt.as_mut() {
                    v.feed(&b64.decode(bytes).unwrap());
                }
            }
            _ => (),
        }
    }
    // What happens there after the screen arrived follows it.
    h.call(Request::SendText {
        session: None,
        id: "far".into(),
        text: "ok".into(),
        submit: true,
    })
    .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while !shown(&vt).contains("later ok") {
        assert!(
            Instant::now() < until,
            "output never followed: {:?}",
            shown(&vt)
        );
        if let Ok(StreamBody::Output { vt: bytes }) = rx.recv_timeout(Duration::from_secs(1)) {
            if let Some(v) = vt.as_mut() {
                v.feed(&b64.decode(bytes).unwrap());
            }
        }
    }
}

/// `mooring remote --stdio` as a phone or another machine runs it over ssh: it watches
/// a terminal the workbench owns without taking it or resizing it, types named keys and
/// text into it, streams events, and closes streams and itself cleanly.
#[test]
fn remote_watches_and_drives_a_terminal_without_taking_or_resizing_it() {
    use base64::Engine;
    use serde_json::{json, Value};
    use std::io::{BufRead, Write};
    let b64 = base64::engine::general_purpose::STANDARD;
    let h = Host::start();
    h.setup();
    // Shows every byte it gets; application cursor keys on, as full-screen programs do.
    h.call(h.launch(
        "rt",
        r"stty raw -echo; printf '\033[?1hready\r\n'; exec cat -v",
    ))
    .unwrap();
    let (mut app, mut app_vt, writable) = h.attach("rt", false);
    assert!(writable);
    read_until(&mut app, &mut app_vt, "ready");

    let remote = || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mooring"))
            .args(["remote", "--stdio", "--runtime", &h.root.to_string_lossy()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel::<(Instant, Value)>();
        std::thread::spawn(move || {
            for line in io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if tx
                    .send((Instant::now(), serde_json::from_str(&line).unwrap()))
                    .is_err()
                {
                    return;
                }
            }
        });
        (child, rx)
    };
    let (mut child, rx) = remote();
    let mut stdin = child.stdin.take().unwrap();
    let mut say = move |v: Value| writeln!(stdin, "{v}").unwrap();
    // The next message `pick` accepts; others are kept for later picks.
    let mut kept: Vec<Value> = vec![];
    let mut next = |pick: &dyn Fn(&Value) -> bool| -> (Instant, Value) {
        if let Some(i) = kept.iter().position(pick) {
            return (Instant::now(), kept.remove(i));
        }
        loop {
            let (at, v) = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("no message");
            if pick(&v) {
                return (at, v);
            }
            kept.push(v);
        }
    };
    let reply = |id: u64| move |v: &Value| v["id"] == id;

    say(json!({ "id": 1, "op": "list" }));
    assert!(next(&reply(1)).1["error"]
        .as_str()
        .unwrap()
        .contains("hello"));
    say(json!({ "id": 2, "op": "hello", "version": 1 }));
    let hello = next(&reply(2)).1;
    assert_eq!(hello["ok"]["version"], 1);
    let machine = hello["ok"]["machine"]["id"].as_str().unwrap().to_owned();
    assert!(!machine.is_empty());

    say(json!({ "id": 3, "op": "list" }));
    let list = next(&reply(3)).1;
    let rt = list["ok"]["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "rt")
        .cloned()
        .expect("rt listed");
    assert_eq!(rt["phase"], "shell");
    assert_eq!(list["ok"]["machine"]["id"], machine.as_str());

    say(json!({ "id": 4, "op": "watch", "terminal": "rt" }));
    let opened = next(&reply(4)).1;
    assert_eq!(
        (opened["ok"]["cols"].as_u64(), opened["ok"]["rows"].as_u64()),
        (Some(80), Some(24))
    );
    let screen = next(&|v: &Value| v["stream"] == 4 && v["type"] == "screen").1;
    let mut phone = Vt::new(80, 24).unwrap();
    phone.feed(&b64.decode(screen["vt"].as_str().unwrap()).unwrap());
    assert!(
        phone.tail_text(24).unwrap().contains("ready"),
        "{}",
        phone.tail_text(24).unwrap()
    );
    assert!(
        phone
            .terminal()
            .mode(mooring_core::terminal::libghostty_vt::terminal::Mode::DECCKM)
            .unwrap(),
        "the replay carries the cursor-key mode"
    );

    // A bad key sends nothing; good ones arrive in order, arrows in application mode.
    say(json!({ "id": 5, "op": "keys", "terminal": "rt", "keys": ["x", "nope"] }));
    assert!(next(&reply(5)).1["error"]
        .as_str()
        .unwrap()
        .contains("nope"));
    let sent = Instant::now();
    say(json!({ "id": 6, "op": "keys", "terminal": "rt", "keys": ["up", "esc", "ctrl-c", "y"] }));
    say(json!({ "id": 7, "op": "send", "terminal": "rt", "text": "hi" }));
    assert_eq!(next(&reply(6)).1["ok"], json!({}));
    assert_eq!(next(&reply(7)).1["ok"], json!({}));
    let mut first_output = None;
    let until = Instant::now() + Duration::from_secs(10);
    while !phone.tail_text(24).unwrap().contains("^[OA^[^Cyhi") {
        assert!(Instant::now() < until, "{}", phone.tail_text(24).unwrap());
        let (at, out) = next(&|v: &Value| v["stream"] == 4 && v["type"] == "output");
        first_output.get_or_insert(at);
        phone.feed(&b64.decode(out["vt"].as_str().unwrap()).unwrap());
    }
    assert!(
        !phone.tail_text(24).unwrap().contains('x'),
        "the rejected batch sent nothing"
    );
    let latency = first_output.unwrap() - sent;
    eprintln!("keys → watcher output {latency:?}");
    assert!(latency < Duration::from_millis(500), "{latency:?}");

    // The workbench still owns it; with the workbench gone the watcher does not take
    // over, so the next workbench to attach writes again without a takeover.
    send(
        &mut app,
        CONTROL,
        &Request::Input {
            bytes: b"z".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut app), Reply::Ok));
    drop(app);
    std::thread::sleep(Duration::from_millis(100));
    let (_app, _, writable) = h.attach("rt", false);
    assert!(writable, "a watcher became the writer");

    say(json!({ "id": 8, "op": "events" }));
    assert_eq!(next(&reply(8)).1["ok"]["stream"], 8);
    let started = next(&|v: &Value| v["stream"] == 8).1;
    assert_eq!(started["event"]["event"], "terminal_started");
    assert_eq!(started["event"]["terminal"], "rt");

    say(json!({ "id": 9, "op": "close", "stream": 4 }));
    assert_eq!(next(&reply(9)).1["ok"], json!({}));
    let end = next(&|v: &Value| v["stream"] == 4 && v["type"] == "end").1;
    assert_eq!(end["reason"], "closed");

    say(json!({ "id": 10, "op": "watch", "terminal": "absent" }));
    assert!(
        next(&reply(10)).1["error"].is_string(),
        "watching nothing fails the request"
    );
    say(json!({ "id": 11, "op": "frobnicate" }));
    assert!(next(&reply(11)).1["error"].is_string());

    // Closed right after asking, possibly before it opened: it ends either way.
    say(json!({ "id": 12, "op": "watch", "terminal": "rt" }));
    say(json!({ "id": 13, "op": "close", "stream": 12 }));
    assert_eq!(next(&reply(13)).1["ok"], json!({}));
    let opened = next(&reply(12)).1;
    if opened["ok"].is_object() {
        next(&|v: &Value| v["stream"] == 12 && v["type"] == "end");
    } else {
        assert_eq!(opened["error"], "closed");
    }

    h.call(Request::Stop { id: "rt".into() }).unwrap();
    let exited = next(&|v: &Value| v["stream"] == 8 && v["event"]["event"] == "terminal_exited").1;
    assert_eq!(exited["event"]["terminal"], "rt");

    let rest: Vec<Value> = kept
        .into_iter()
        .chain(rx.try_iter().map(|(_, v)| v))
        .collect();
    assert!(
        !rest.iter().any(|v| v["type"] == "resized"),
        "the watcher resized it: {rest:?}"
    );
    // The client hanging up ends the gateway and its streams.
    drop(say);
    let until = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < until,
            "remote kept running after its client left"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // The same machine id next time.
    let (mut again, rx) = remote();
    writeln!(
        again.stdin.as_mut().unwrap(),
        "{}",
        json!({ "id": 1, "op": "hello" })
    )
    .unwrap();
    let (_, hello) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(hello["ok"]["machine"]["id"], machine.as_str());
    drop(again.stdin.take());
    again.wait().unwrap();
}

/// A remote client that stops reading (a phone gone to sleep mid-stream) slows neither
/// the terminal nor the workbench beside it; once it reads again it gets the screen anew.
#[test]
fn a_stalled_remote_watcher_slows_nobody_and_gets_the_screen_again() {
    use serde_json::{json, Value};
    use std::io::{BufRead, Write};
    let h = Host::start();
    h.setup();
    h.call(h.launch(
        "flood",
        "read go; seq 1 400000; echo alone-done; read go; seq 1 400000; echo flood-done; exec cat",
    ))
    .unwrap();
    let (mut app, mut vt, _) = h.attach("flood", false);
    app.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    // A viewer this busy falls behind too; the host then sends it the screen again.
    let flood = |app: &mut UnixStream, vt: &mut Vt, done: &str| {
        let started = Instant::now();
        send(
            app,
            CONTROL,
            &Request::Input {
                bytes: b"\r".to_vec(),
                lossy: false,
            },
        )
        .unwrap();
        let mut snapshot = Vec::new();
        while !vt.tail_text(24).unwrap().contains(done) {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "{done} never appeared"
            );
            match read_frame(app).unwrap() {
                (OUTPUT, bytes) => vt.feed(&bytes),
                (SNAPSHOT, bytes) => snapshot.extend(bytes),
                (READY, _) => *vt = Vt::restore(&std::mem::take(&mut snapshot)).unwrap(),
                _ => (),
            }
        }
        started.elapsed()
    };
    let alone = flood(&mut app, &mut vt, "alone-done");
    let mut child = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["remote", "--stdio", "--runtime", &h.root.to_string_lossy()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    writeln!(stdin, "{}", json!({ "id": 1, "op": "hello" })).unwrap();
    writeln!(
        stdin,
        "{}",
        json!({ "id": 2, "op": "watch", "terminal": "flood" })
    )
    .unwrap();
    // Taken a line at a time only when asked: this is what stalls the client, and a client that
    // has nothing more to say must fail the test, not hang it.
    let (ask, asked) = std::sync::mpsc::channel::<()>();
    let (tell, told) = std::sync::mpsc::channel::<Value>();
    std::thread::spawn(move || {
        let mut stdout = io::BufReader::new(stdout);
        let mut line = String::new();
        while asked.recv().is_ok() {
            line.clear();
            if stdout.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            if tell.send(serde_json::from_str(&line).unwrap()).is_err() {
                return;
            }
        }
    });
    let read = || -> Option<Value> {
        ask.send(()).unwrap();
        told.recv_timeout(Duration::from_secs(20)).ok()
    };
    let b64 = base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let mut every = Vt::new(80, 24).unwrap();
    loop {
        let v = read().expect("the remote client went quiet");
        if v["type"] == "screen" {
            every.feed(&b64.decode(v["vt"].as_str().unwrap()).unwrap());
            break;
        }
    }
    // From here the remote client reads nothing.
    let stalled = flood(&mut app, &mut vt, "flood-done");
    eprintln!(
        "400k lines to the workbench: alone {alone:?}, beside a stalled remote watcher {stalled:?}"
    );
    assert!(
        stalled < alone * 2 + Duration::from_millis(500),
        "{alone:?} → {stalled:?}"
    );
    // Reading again: the backlog it missed is skipped for the screen as it was then, and what
    // came after that follows as output, as for any phone.
    let until = Instant::now() + Duration::from_secs(20);
    let mut screens = 0;
    let mut outputs = 0;
    let mut max_gap = Duration::ZERO;
    let mut phone = Vt::new(80, 24).unwrap();
    while !phone.tail_text(24).unwrap().contains("flood-done") {
        assert!(Instant::now() < until, "never caught up");
        let asked_at = Instant::now();
        let got = read();
        max_gap = max_gap.max(asked_at.elapsed());
        let Some(v) = got else {
            // Says which of two things this was: everything arrived and only the screen did not
            // come again, or the last output was never sent.
            panic!(
                "quiet with flood-done unseen: {screens} screens, {outputs} outputs, slowest read \
                 {max_gap:?}, all output in order {} it; phone {:?}, all output {:?}",
                if every.tail_text(24).unwrap().contains("flood-done") {
                    "has"
                } else {
                    "lacks"
                },
                phone.tail_text(24).unwrap(),
                every.tail_text(24).unwrap(),
            );
        };
        if v["type"] == "output" {
            outputs += 1;
            every.feed(&b64.decode(v["vt"].as_str().unwrap()).unwrap());
        }
        match v["type"].as_str() {
            Some("screen") => {
                screens += 1;
                phone = Vt::new(80, 24).unwrap();
                phone.feed(&b64.decode(v["vt"].as_str().unwrap()).unwrap());
            }
            Some("output") if screens > 0 => {
                phone.feed(&b64.decode(v["vt"].as_str().unwrap()).unwrap());
            }
            _ => (),
        }
    }
    eprintln!("caught up after {screens} screens, {outputs} outputs, slowest read {max_gap:?}");
    drop(stdin);
    child.wait().unwrap();
}
