//! A conversation changing panes, and asking which conversation a pane shows.
use super::*;

fn type_in(h: &Host, pane: &str, text: &str) {
    let (mut stream, _, _) = h.attach(pane, false);
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: text.as_bytes().to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut stream), Reply::Ok));
}
/// Waits until `pane` is a recognized Codex no conversation is tied to.
fn wait_unbound(h: &Host, pane: &str) {
    // Hints are computed after a submitted command; the fixture has none, so probe via input.
    type_in(h, pane, "\n");
    let until = Instant::now() + Duration::from_secs(10);
    while !recognition(h).terminal_hints.contains_key(pane) {
        assert!(
            Instant::now() < until,
            "{pane} never unbound: {:?}",
            recognition(h).terminal_hints
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn a_thread_resumed_in_a_new_pane_moves_there_with_its_next_prompt() {
    let h = Host::start();
    h.setup();
    let [thread] = ids();
    let gate = h.root.join("resumed");
    daemon_codex(
        &h,
        "old",
        "proj",
        &[step("先把登录页的跳转回归问题查清楚", &thread, None)],
        "",
    );
    wait_for(&h, "the first pane binds", |w| {
        current_is(w, "old", &thread)
    });
    daemon_codex(
        &h,
        "new",
        "proj",
        &[step("接着把这个问题提交成一个 issue", &thread, Some(&gate))],
        "",
    );
    wait_unbound(&h, "new");
    fs::write(&gate, "").unwrap();
    wait_for(&h, "the thread follows its newest prompt", |w| {
        current_is(w, "new", &thread) && w.current_agent("old").is_none()
    });
    assert!(!recognition(&h).terminal_hints.contains_key("new"));
    wait_unbound(&h, "old");
    // The pane it left says it is unbound, and its exit does not end the moved session;
    // with the prompt scrolled out of reach nothing could bind it back.
    // (Past SUBMIT_WINDOW, or this Enter would count as evidence for the prompt.)
    std::thread::sleep(Duration::from_secs(5));
    for _ in 0..21 {
        type_in(&h, "new", &"say:……\n".repeat(10));
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_secs(1));
    h.call(Request::Stop { id: "old".into() }).unwrap();
    wait_for(&h, "the old pane is gone", |w| {
        w.terminals
            .iter()
            .any(|t| t.id == "old" && t.state != ProcessState::Running)
    });
    std::thread::sleep(Duration::from_secs(1));
    assert!(current_is(&h.workspace(), "new", &thread));
}

#[test]
fn a_thread_shown_on_both_panes_stays_put_until_its_pane_ends() {
    use std::io::Write;
    let h = Host::start();
    h.setup();
    let [thread] = ids();
    let prompt = "把通知投递日志整理成一张证据表格";
    daemon_codex(&h, "old", "proj", &[step(prompt, &thread, None)], "");
    wait_for(&h, "the first pane binds", |w| {
        current_is(w, "old", &thread)
    });
    daemon_codex(&h, "new", "proj", &[step(prompt, &thread, None)], "");
    wait_unbound(&h, "new");
    // A long turn keeps appending: this prompt's screen comparisons run out while it stays.
    let log = h.root.join(format!("logs/codex/{thread}.jsonl"));
    for _ in 0..8 {
        let mut file = fs::OpenOptions::new().append(true).open(&log).unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({"type":"event_msg","payload":{"type":"agent_reasoning","text":"…"}})
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(current_is(&h.workspace(), "old", &thread));
    }
    h.call(Request::Stop { id: "old".into() }).unwrap();
    wait_for(&h, "the pane still showing it takes over at once", |w| {
        current_is(w, "new", &thread)
    });
    assert!(!recognition(&h).terminal_hints.contains_key("new"));
}

#[test]
fn a_pane_ending_hours_after_the_last_prompt_hands_over_to_the_pane_showing_it() {
    let h = Host::start();
    h.setup();
    let [thread] = ids();
    let prompt = "把这个问题提交成 issue 并注明来源";
    let long_ago = mooring_core::content::now() - 3 * 3600;
    let logs = h.root.join("logs/codex");
    fs::create_dir_all(&logs).unwrap();
    let log = logs.join(format!("{thread}.jsonl"));
    fs::write(
        &log,
        format!(
            "{}\n{}\n",
            serde_json::json!({"type":"session_meta","payload":{"id":thread,"cwd":"/tmp"}}),
            serde_json::json!({"timestamp":long_ago,"type":"event_msg","payload":{"type":"user_message","message":prompt}}),
        ),
    )
    .unwrap();
    let stamp = std::process::Command::new("/bin/date")
        .args(["-r", &long_ago.to_string(), "+%Y%m%d%H%M.%S"])
        .output()
        .unwrap();
    assert!(std::process::Command::new("/usr/bin/touch")
        .args(["-t", String::from_utf8_lossy(&stamp.stdout).trim()])
        .arg(&log)
        .status()
        .unwrap()
        .success());
    // Its TUI holds the log open, so the old pane holds it without any recent prompt.
    let cli = h.root.join("codex");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let script = format!(
        "exec 3<{}; while read -r line; do :; done",
        shell_quote(&log.to_string_lossy())
    );
    h.call(h.launch(
        "old",
        &format!(
            "exec {} -c {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    type_in(&h, "old", "\n");
    wait_for(&h, "the open file binds the old log", |w| {
        current_is(w, "old", &thread)
    });
    daemon_codex(&h, "new", "proj", &[], "");
    wait_unbound(&h, "new");
    type_in(&h, "new", &format!("say:› {prompt}\n"));
    std::thread::sleep(Duration::from_millis(500));
    assert!(h.workspace().current_agent("new").is_none());
    h.call(Request::Stop { id: "old".into() }).unwrap();
    wait_for(
        &h,
        "the old conversation moves to the pane showing it",
        |w| current_is(w, "new", &thread),
    );
}

fn associate(h: &Host, pane: &str) -> Option<String> {
    match h
        .call(Request::AssociateScreen { id: pane.into() })
        .unwrap()
    {
        Reply::ScreenAssociated { session } => session,
        other => panic!("{other:?}"),
    }
}
/// A thread's log with a prompt the pane never painted, its comparisons used up by a turn
/// that kept appending: only a user asking can still tie it to a screen.
fn exhausted_thread(h: &Host, thread: &str, prompt: &str) -> PathBuf {
    use std::io::Write;
    let log = h.root.join(format!("logs/codex/{thread}.jsonl"));
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    fs::write(
        &log,
        format!(
            "{}\n{}\n",
            serde_json::json!({"type":"session_meta","payload":{"id":thread,"cwd":"/tmp"}}),
            serde_json::json!({"timestamp":mooring_core::content::now(),"type":"event_msg","payload":{"type":"user_message","message":prompt}}),
        ),
    )
    .unwrap();
    for _ in 0..8 {
        let mut file = fs::OpenOptions::new().append(true).open(&log).unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({"type":"event_msg","payload":{"type":"agent_reasoning","text":"…"}})
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(300));
    }
    log
}

#[test]
fn asking_ties_an_unbound_pane_to_the_conversation_on_its_screen() {
    let h = Host::start();
    h.setup();
    let [thread] = ids();
    let prompt = "把这个问题提交成 issue 并注明来源";
    daemon_codex(&h, "p", "proj", &[], "");
    wait_unbound(&h, "p");
    // Past SUBMIT_WINDOW, or the Enter above would count as evidence for the prompt.
    std::thread::sleep(Duration::from_secs(4));
    exhausted_thread(&h, &thread, prompt);
    // Words it has not shown name nothing.
    assert_eq!(associate(&h, "p"), None);
    assert!(h.workspace().current_agent("p").is_none());
    type_in(&h, "p", &format!("say:› {prompt}\n"));
    std::thread::sleep(Duration::from_millis(500));
    assert!(h.workspace().current_agent("p").is_none());
    let session = associate(&h, "p").expect("the screen names the conversation");
    let w = h.workspace();
    let bound = w.current_agent("p").unwrap();
    assert_eq!(bound.id, session);
    assert_eq!(bound.cli_session_id, thread);
    assert!(!recognition(&h).terminal_hints.contains_key("p"));
    // Asked again it answers the same, and marks nothing done by itself.
    assert_eq!(associate(&h, "p"), Some(session));
    assert_eq!(bound.outcome, mooring_core::model::Outcome::Open);
}

#[test]
fn asking_does_not_guess_between_panes_or_threads() {
    let h = Host::start();
    h.setup();
    let [two_panes, first, second] = ids();
    let shared = "两个窗格都显示这同一句提问内容";
    daemon_codex(&h, "a", "proj", &[], "");
    daemon_codex(&h, "b", "proj", &[], "");
    daemon_codex(&h, "c", "proj", &[], "");
    for pane in ["a", "b", "c"] {
        wait_unbound(&h, pane);
    }
    std::thread::sleep(Duration::from_secs(4));
    exhausted_thread(&h, &two_panes, shared);
    type_in(&h, "a", &format!("say:› {shared}\n"));
    type_in(&h, "b", &format!("say:› {shared}\n"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(associate(&h, "a"), None);
    assert_eq!(associate(&h, "b"), None);
    // One screen showing two threads' newest prompts. (Past SUBMIT_WINDOW, or the Enters
    // above would make these threads fit the other panes too, and hide the case.)
    std::thread::sleep(Duration::from_secs(4));
    let (one, other) = (
        "同一个窗格里第一条对话的提问",
        "同一个窗格里第二条对话的提问",
    );
    exhausted_thread(&h, &first, one);
    exhausted_thread(&h, &second, other);
    type_in(&h, "c", &format!("say:› {one}\n"));
    type_in(&h, "c", &format!("say:› {other}\n"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(associate(&h, "c"), None);
    let w = h.workspace();
    for pane in ["a", "b", "c"] {
        assert!(w.current_agent(pane).is_none(), "{pane}");
    }
}

#[test]
fn an_older_conversations_words_on_the_screen_are_not_what_the_pane_runs() {
    let h = Host::start();
    h.setup();
    let [older] = ids();
    let stale = "这是启动之前就有的旧对话最后一句";
    let log = h.root.join(format!("logs/codex/{older}.jsonl"));
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    // Made old outside the watched folder: the index must never see it as just written, which
    // is not what an old conversation looks like.
    let making = h.root.join("older-log");
    let long_ago = mooring_core::content::now() - 3 * 3600;
    fs::write(
        &making,
        format!(
            "{}\n{}\n",
            serde_json::json!({"type":"session_meta","payload":{"id":older,"cwd":"/tmp"}}),
            serde_json::json!({"timestamp":long_ago,"type":"event_msg","payload":{"type":"user_message","message":stale}}),
        ),
    )
    .unwrap();
    let stamp = std::process::Command::new("/bin/date")
        .args(["-r", &long_ago.to_string(), "+%Y%m%d%H%M.%S"])
        .output()
        .unwrap();
    assert!(std::process::Command::new("/usr/bin/touch")
        .args(["-t", String::from_utf8_lossy(&stamp.stdout).trim()])
        .arg(&making)
        .status()
        .unwrap()
        .success());
    fs::rename(&making, &log).unwrap();
    daemon_codex(&h, "d", "proj", &[], "");
    wait_unbound(&h, "d");
    type_in(&h, "d", &format!("say:› {stale}\n"));
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(associate(&h, "d"), None);
    assert!(h.workspace().current_agent("d").is_none());
}
