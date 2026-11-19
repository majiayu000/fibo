use super::*;

/// The turn clock leaves out a wait up to the moment the user answered in the terminal, which
/// the host sees as input: not up to the approved tool's PostToolUse, minutes of work later.
#[test]
fn an_answer_typed_in_the_terminal_ends_the_wait_for_the_turn_clock() {
    let h = Host::start();
    h.setup();
    let claude = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &claude).unwrap();
    let log = h.root.join("answer.jsonl");
    // A real log's prompt: the turn starts when it was written.
    let prompt = serde_json::json!({"type": "user", "sessionId": "answer-clock", "cwd": "/tmp",
        "timestamp": fibo_core::content::now(), "message": {"content": "test"}});
    fs::write(&log, format!("{prompt}\n")).unwrap();
    let pid = h.root.join("pid");
    let script = format!(
        "echo $$ > {}; stty raw -echo; cat > /dev/null",
        pid.display()
    );
    h.call(h.launch(
        "clock",
        &format!(
            "exec {} -c {}",
            shell_quote(&claude.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    let pid: u32 = wait_file(&pid).trim().parse().unwrap();
    let hook = |event: &str| Request::Hook {
        runtime: Agent::Claude,
        pid,
        cli_session_id: "answer-clock".into(),
        transcript: log.to_string_lossy().into_owned(),
        ended: false,
        event: Some(event.into()),
        facts: Default::default(),
        terminal: Some("clock".into()),
        cwd: "/tmp".into(),
    };
    let card = || {
        let w = h.workspace();
        let s = w.current_agent("clock").expect("bound").clone();
        w.content.get(&s.id).cloned().unwrap_or_default()
    };
    h.call(hook("UserPromptSubmit")).unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    h.call(hook("PermissionRequest")).unwrap();
    let asked = card();
    assert_eq!(asked.turn, fibo_core::content::Turn::Waiting, "{asked:?}");
    assert!(!asked.answered());
    std::thread::sleep(Duration::from_millis(2100));
    let (mut stream, _, writable) = h.attach("clock", true);
    assert!(writable);
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: b"1".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut stream), Reply::Ok));
    let answered = card();
    assert!(
        answered.answered(),
        "the host noted the answer: {:?}",
        answered.waited
    );
    let waited = answered.waited.answered - answered.signal_at;
    assert!((2..=4).contains(&waited), "waited {waited}s");
    // The approved tool runs on; its PostToolUse comes later and is not waiting.
    std::thread::sleep(Duration::from_millis(2100));
    h.call(hook("PostToolUse")).unwrap();
    let ran = card();
    assert_eq!(ran.turn, fibo_core::content::Turn::Running);
    assert_eq!(ran.running_since(), Some(ran.turn_started + waited));
}
