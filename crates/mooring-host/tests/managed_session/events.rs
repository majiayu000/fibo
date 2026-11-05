use super::*;

/// `mooring events`: the state now, then every change in order, none twice: a terminal
/// starting, its Claude waiting at an approval, the approval answered and gone, the turn
/// done, shell commands passing and failing, the terminal exiting. A subscriber that never
/// reads costs the others nothing and is told `lagged` when it reads again.
#[test]
fn events_stream_every_change_once_in_order_and_a_stalled_reader_blocks_nobody() {
    use {mooring_core::hook::Notice, std::io::BufRead};
    let h = Host::start();
    h.setup();
    let subscribe = |h: &Host| {
        let mut s = UnixStream::connect(h.socket()).unwrap();
        send(&mut s, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
        assert!(matches!(
            receive::<Reply>(&mut s).unwrap(),
            Reply::Hello { .. }
        ));
        send(&mut s, CONTROL, &Request::Subscribe).unwrap();
        s
    };
    // The command line, as a script would run it.
    let mut cli = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["events", "--json", "--runtime", &h.root.to_string_lossy()])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut cli_out = io::BufReader::new(cli.stdout.take().unwrap()).lines();
    let mut fast = subscribe(&h);
    let snapshot = match receive::<Reply>(&mut fast).unwrap() {
        Reply::Events(n) => n,
        x => panic!("{x:?}"),
    };
    assert!(snapshot.is_empty(), "nothing runs yet: {snapshot:?}");
    let (tx, rx) = std::sync::mpsc::channel::<(Instant, Notice)>();
    std::thread::spawn(move || {
        while let Ok(Reply::Events(batch)) = receive::<Reply>(&mut fast) {
            for n in batch {
                if tx.send((Instant::now(), n)).is_err() {
                    return;
                }
            }
        }
    });
    let next = |what: &str, ok: &dyn Fn(&Notice) -> bool| -> (Instant, Notice) {
        let until = Instant::now() + Duration::from_secs(90);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let (at, n) = rx
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("no {what}"));
            if ok(&n) {
                return (at, n);
            }
            // Anything else in between must be one of the expected kinds, not a repeat.
            assert!(
                !matches!(
                    n.event.as_str(),
                    "done" | "approval_gone" | "terminal_exited" | "lagged"
                ),
                "unexpected before {what}: {n:?}"
            );
        }
    };
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let claude = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &claude).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let log = logs.join(format!("{id}.jsonl"));
    let seed = h.root.join("seed");
    let user = serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":mooring_core::content::now(),"message":{"content":"发布前检查"}});
    fs::write(&seed, format!("{user}\n")).unwrap();
    let done = h.root.join("done");
    let reply = serde_json::json!({"type":"assistant","sessionId":id,"uuid":"a1","timestamp":mooring_core::content::now(),"message":{"model":"claude","stop_reason":"end_turn","content":[{"type":"text","text":"推送完成"}]}});
    fs::write(&done, format!("{reply}\n")).unwrap();
    let script = format!(
        "cat {seed} > {log}; printf '%s\\n' 'Bash command' '  git push' 'Do you want to proceed?' '> 1. Yes' '  2. No'; read -r answer; printf '\\033[2J\\033[Hpushed\\n'; cat {done} >> {log}; read -r end",
        seed = shell_quote(&seed.to_string_lossy()),
        log = shell_quote(&log.to_string_lossy()),
        done = shell_quote(&done.to_string_lossy()),
    );
    h.call(h.launch(
        "ev",
        &format!(
            "exec {} -c {} fixture --session-id {}",
            shell_quote(&claude.to_string_lossy()),
            shell_quote(&script),
            id
        ),
    ))
    .unwrap();
    let mine = |n: &Notice| {
        n.terminal.as_deref() == Some("ev") || n.session.contains(&id) || {
            let w = h.workspace();
            w.agent_sessions
                .iter()
                .any(|s| s.id == n.session && s.cli_session_id == id)
        }
    };
    next("terminal_started", &|n| {
        n.event == "terminal_started" && n.terminal.as_deref() == Some("ev")
    });
    let (_, waiting) = next("waiting", &|n| n.event == "waiting" && mine(n));
    // Status events say which terminal they came from, as skill.md promises.
    assert_eq!(waiting.terminal.as_deref(), Some("ev"));
    let (_, approval) = next("approval", &|n| n.event == "approval" && mine(n));
    assert_eq!(approval.detail, "Do you want to proceed?");
    assert_eq!(approval.terminal.as_deref(), Some("ev"));
    let first = cli_out.next().unwrap().unwrap(); // It has subscribed: it said something.
    let asked = Instant::now();
    h.call(Request::AnswerApproval {
        id: "ev".into(),
        question: "Do you want to proceed?".into(),
        allow: true,
    })
    .unwrap();
    let (gone_at, _) = next("approval_gone", &|n| n.event == "approval_gone" && mine(n));
    let latency = gone_at.duration_since(asked);
    next("done", &|n| n.event == "done" && mine(n));
    // Shell commands, marked the way an integrated shell marks them.
    h.call(h.launch(
        "sh",
        "printf '\\033]133;C\\007'; sleep 0.2; printf '\\033]133;D;0\\007'; printf '\\033]133;C\\007'; printf '\\033]133;D;3\\007'; sleep 60",
    ))
    .unwrap();
    let (_, ok) = next("command_done", &|n| {
        n.event == "command_done" && n.terminal.as_deref() == Some("sh")
    });
    assert!(ok.duration_ms.is_some_and(|d| d >= 150), "{ok:?}");
    let (_, failed) = next("command_failed", &|n| n.event == "command_failed");
    assert_eq!(failed.exit_code, Some(3));
    // A subscriber that stops reading while a burst goes by.
    let mut stalled = subscribe(&h);
    let marks = |n: u32| {
        format!("i=0; while [ $i -lt {n} ]; do printf '\\033]133;C\\007\\033]133;D;0\\007'; i=$((i+1)); done")
    };
    h.call(h.launch("burst", &(marks(3000) + "; sleep 60")))
        .unwrap();
    let mut burst = 0;
    let started = Instant::now();
    while burst < 3000 {
        let (_, n) = next("burst", &|n| n.terminal.as_deref() == Some("burst"));
        if n.event == "command_done" {
            burst += 1;
        }
    }
    let rpc_at = Instant::now();
    h.workspace();
    assert!(
        rpc_at.elapsed() < Duration::from_millis(500),
        "the host is held up"
    );
    eprintln!(
        "3000 command events to the reading subscriber in {:?}",
        started.elapsed()
    );
    let _ = cli.kill().and_then(|()| cli.wait());
    let lines: Vec<serde_json::Value> = std::iter::once(Ok(first))
        .chain(cli_out)
        .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
        .collect();
    let kinds: Vec<&str> = lines
        .iter()
        .filter_map(|l| l["event"].as_str())
        .filter(|e| *e != "command_done")
        .collect();
    for want in [
        "terminal_started",
        "waiting",
        "approval",
        "approval_gone",
        "done",
        "command_failed",
    ] {
        assert!(
            kinds.contains(&want),
            "{want} missing from the command line: {kinds:?}"
        );
    }
    assert!(lines
        .iter()
        .any(|l| l["event"] == "approval" && l["detail"] == "Do you want to proceed?"));
    h.call(Request::Stop { id: "ev".into() }).unwrap();
    next("terminal_exited", &|n| {
        n.event == "terminal_exited" && n.terminal.as_deref() == Some("ev")
    });
    // Twice the stalled one's backlog (4096 notices; the burst stayed under it: one in its
    // writer's stuck frame, one queued) plus its socket. A terminal ends once its output is read.
    h.call(h.launch("flood", &marks(12288))).unwrap();
    let ended = |t: &Terminal| t.id == "flood" && t.state == ProcessState::Exited;
    wait_for(&h, "the flood ends", |w| w.terminals.iter().any(ended));
    let timeout = Some(Duration::from_secs(10));
    stalled.set_read_timeout(timeout).unwrap();
    let lagged = std::iter::from_fn(|| receive::<Reply>(&mut stalled).ok())
        .any(|r| matches!(r, Reply::Events(b) if b.iter().any(|n| n.event == "lagged")));
    assert!(lagged, "the stalled subscriber was never told it lagged");
    eprintln!("approval answered -> approval_gone in {latency:?}");
    assert!(latency < Duration::from_millis(200), "{latency:?}");
}

/// The host's CPU with four spinning terminals, with no subscriber and then with ten.
#[test]
#[ignore = "measurement"]
fn ten_event_subscribers_cost_the_host_almost_nothing() {
    let h = Host::measured();
    h.setup();
    for i in 0..4 {
        h.call(h.launch(
            &format!("spin{i}"),
            "while :; do for c in '|' '/' '-' '\\\\'; do printf '\\r\\033[2K%s working… (esc to interrupt)' \"$c\"; sleep 0.08; done; done",
        ))
        .unwrap();
    }
    let cpu = |h: &Host| {
        let out = Command::new("/bin/ps")
            .args(["-o", "time=", "-p", &h.child.id().to_string()])
            .output()
            .unwrap();
        let t = String::from_utf8(out.stdout).unwrap();
        let (m, s) = t.trim().split_once(':').unwrap();
        m.parse::<f64>().unwrap() * 60. + s.parse::<f64>().unwrap()
    };
    std::thread::sleep(Duration::from_secs(3));
    let window = Duration::from_secs(20);
    let a = cpu(&h);
    std::thread::sleep(window);
    let none = (cpu(&h) - a) / window.as_secs_f64() * 100.;
    let mut subs = vec![];
    for _ in 0..10 {
        let mut s = UnixStream::connect(h.socket()).unwrap();
        send(&mut s, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
        receive::<Reply>(&mut s).unwrap();
        send(&mut s, CONTROL, &Request::Subscribe).unwrap();
        receive::<Reply>(&mut s).unwrap();
        subs.push(s);
    }
    let b = cpu(&h);
    std::thread::sleep(window);
    let ten = (cpu(&h) - b) / window.as_secs_f64() * 100.;
    eprintln!("host CPU, 4 spinners: no subscriber {none:.2}%, ten subscribers {ten:.2}%");
    assert!(ten - none < 1., "{none:.2}% -> {ten:.2}%");
}
