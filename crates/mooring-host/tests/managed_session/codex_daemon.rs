use super::*;

#[test]
fn daemon_codex_dialog_gone_from_screen_is_settled_by_a_read_only_thread_read() {
    use std::io::Write;
    let h = Host::start();
    h.setup();
    let methods = fake_codex_control(
        &h.root.join("codex-control.sock"),
        serde_json::json!({"type": "idle"}),
    );
    let [a] = ids();
    daemon_codex(
        &h,
        "p0",
        "proj",
        &[step("请检查发布脚本里的回滚步骤是否完整", &a, None)],
        "",
    );
    wait_for(&h, "the thread binds to its pane", |w| {
        current_is(w, "p0", &a)
    });
    // The log asks the user; the TUI never draws the question.
    {
        let mut log = fs::OpenOptions::new()
            .append(true)
            .open(h.root.join(format!("logs/codex/{a}.jsonl")))
            .unwrap();
        let row = serde_json::json!({"timestamp": mooring_core::content::now(), "type": "event_msg",
            "payload": {"type": "request_user_input", "questions": [{"question": "回滚到哪个版本？"}]}});
        writeln!(log, "{row}").unwrap();
    }
    let turn = |w: &Workspace| {
        w.current_agent("p0")
            .and_then(|s| w.content.get(&s.id))
            .map(|c| (c.turn, c.signal.clone()))
    };
    wait_for(&h, "the logged question waits on the user", |w| {
        turn(w).is_some_and(|t| t.0 == mooring_core::content::Turn::Waiting)
    });
    // A keystroke echoes, so the screen settles again without the question on it.
    let (mut stream, _vt, writable) = h.attach("p0", true);
    assert!(writable);
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: b"x".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut stream), Reply::Ok));
    let until = Instant::now() + Duration::from_secs(20);
    let settled = loop {
        let t = turn(&h.workspace()).unwrap();
        if t.0 != mooring_core::content::Turn::Waiting {
            break t;
        }
        assert!(
            Instant::now() < until,
            "idle thread status must settle the dialog"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(
        settled,
        (mooring_core::content::Turn::Interrupted, "官方状态".into())
    );
    let methods = methods.lock().unwrap().clone();
    assert!(methods.contains(&"thread/read".to_owned()), "{methods:?}");
    assert!(
        methods
            .iter()
            .all(|m| matches!(m.as_str(), "initialize" | "initialized" | "thread/read")),
        "read-only: never subscribes, got {methods:?}"
    );
    assert!(
        recognition(&h).official.is_empty(),
        "a working query leaves no notice"
    );
}

#[test]
fn an_unreachable_codex_daemon_is_a_visible_notice_not_a_silent_guess() {
    use std::io::Write;
    let h = Host::start();
    h.setup();
    // No control socket at all: the daemon is not running.
    let [a] = ids();
    daemon_codex(
        &h,
        "p0",
        "proj",
        &[step("检查一下回滚脚本是否覆盖了数据库迁移", &a, None)],
        "",
    );
    wait_for(&h, "the thread binds to its pane", |w| {
        current_is(w, "p0", &a)
    });
    {
        let mut log = fs::OpenOptions::new()
            .append(true)
            .open(h.root.join(format!("logs/codex/{a}.jsonl")))
            .unwrap();
        let row = serde_json::json!({"timestamp": mooring_core::content::now(), "type": "event_msg",
            "payload": {"type": "request_user_input", "questions": [{"question": "回滚到哪个版本？"}]}});
        writeln!(log, "{row}").unwrap();
    }
    let (mut stream, _vt, _) = h.attach("p0", true);
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: b"x".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut stream), Reply::Ok));
    let until = Instant::now() + Duration::from_secs(20);
    let notice = loop {
        if let Some(n) = recognition(&h).official.get("Codex").cloned() {
            break n;
        }
        assert!(Instant::now() < until, "the failed query must be reported");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(
        notice.starts_with("Codex 官方状态查询不可用，已改为只看日志和画面"),
        "{notice}"
    );
    let w = h.workspace();
    let s = w.current_agent("p0").unwrap();
    assert_eq!(
        w.content[&s.id].turn,
        mooring_core::content::Turn::Waiting,
        "without the official answer the logged question stands"
    );
}

#[test]
fn codex_subagent_logs_keep_the_parent_working_and_carry_its_dialog() {
    use std::io::Write;
    let h = Host::start();
    h.setup();
    let [a] = ids();
    daemon_codex(
        &h,
        "p0",
        "proj",
        &[step("把三个模块的迁移分给子 Agent 并行处理", &a, None)],
        "",
    );
    wait_for(&h, "the thread binds to its pane", |w| {
        current_is(w, "p0", &a)
    });
    let logs = h.root.join("logs/codex");
    let row = |payload: serde_json::Value| {
        format!(
            "{}\n",
            serde_json::json!({"timestamp": mooring_core::content::now(), "type": "event_msg", "payload": payload})
        )
    };
    // The parent's turn ends with its subagent still running.
    fs::OpenOptions::new()
        .append(true)
        .open(logs.join(format!("{a}.jsonl")))
        .unwrap()
        .write_all(
            row(serde_json::json!({"type": "task_complete", "last_agent_message": "已派出子任务"}))
                .as_bytes(),
        )
        .unwrap();
    // Like the real daemon, the subagent's log stays open while it works.
    let kid = uuid::Uuid::new_v4().to_string();
    let mut child = fs::File::create(logs.join(format!("{kid}.jsonl"))).unwrap();
    let header = serde_json::json!({"type": "session_meta", "payload": {"id": kid, "cwd": "/tmp",
        "source": {"subagent": {"thread_spawn": {"parent_thread_id": a, "depth": 1, "agent_path": "/root/migrate"}}},
        "agent_nickname": "Hypatia", "parent_thread_id": a}});
    child.write_all(format!("{header}\n").as_bytes()).unwrap();
    child
        .write_all(row(serde_json::json!({"type": "task_started", "turn_id": "k1"})).as_bytes())
        .unwrap();
    let card = |w: &Workspace| {
        w.current_agent("p0")
            .and_then(|s| w.content.get(&s.id).map(|c| (s.clone(), c.clone())))
    };
    wait_for(
        &h,
        "a running subagent keeps the stopped parent working",
        |w| {
            card(w).is_some_and(|(s, c)| {
                c.turn == mooring_core::content::Turn::Completed
                    && mooring_core::status::of(w, &s, mooring_core::content::now())
                        == mooring_core::status::Status::Running
            })
        },
    );
    let item = |payload: serde_json::Value| {
        format!(
            "{}\n",
            serde_json::json!({"timestamp": mooring_core::content::now(), "type": "response_item", "payload": payload})
        )
    };
    child
        .write_all(
            item(serde_json::json!({"type": "function_call", "name": "request_user_input", "call_id": "q1",
                "arguments": serde_json::json!({"questions": [{"question": "是否运行迁移？"}]}).to_string()}))
                .as_bytes(),
        )
        .unwrap();
    wait_for(&h, "the subagent's question is the parent's", |w| {
        card(w).is_some_and(|(_, c)| {
            c.turn == mooring_core::content::Turn::Waiting
                && c.waiting.starts_with("子 Agent Hypatia：")
        })
    });
    // The user answers in the parent's TUI; the subagent records the answer and finishes.
    child
        .write_all(
            item(serde_json::json!({"type": "function_call_output", "call_id": "q1", "output": "运行"}))
                .as_bytes(),
        )
        .unwrap();
    child
        .write_all(
            row(serde_json::json!({"type": "task_complete", "last_agent_message": "迁移完成"}))
                .as_bytes(),
        )
        .unwrap();
    wait_for(&h, "the finished subagent releases the parent", |w| {
        card(w).is_some_and(|(s, c)| {
            c.turn == mooring_core::content::Turn::Completed
                && mooring_core::status::of(w, &s, mooring_core::content::now())
                    != mooring_core::status::Status::Running
        })
    });
    drop(child);
}

#[test]
fn daemon_codex_same_cwd_panes_bind_to_their_own_screen_and_exit_clears_current() {
    let h = Host::start();
    h.setup();
    let [a, b] = ids();
    daemon_codex(
        &h,
        "p0",
        "proj",
        &[step("请检查登录页跳转逻辑并修复回归问题", &a, None)],
        "",
    );
    daemon_codex(
        &h,
        "p1",
        "proj",
        &[step("给设置页补上深色模式的截图测试用例", &b, None)],
        "",
    );
    wait_for(
        &h,
        "each same-cwd thread binds to the pane showing its prompt",
        |w| current_is(w, "p0", &a) && current_is(w, "p1", &b),
    );
    assert!(recognition(&h).terminal_hints.is_empty());
    h.call(Request::Stop { id: "p0".into() }).unwrap();
    wait_for(&h, "a screen-bound Codex exit clears current", |w| {
        w.current_agent("p0").is_none() && current_is(w, "p1", &b)
    });
}

#[test]
fn a_spinning_title_shows_a_turn_the_log_has_not_and_stops_with_it() {
    use mooring_core::content::Turn;
    use std::io::Write;
    let h = Host::start();
    h.setup();
    let [a] = ids();
    daemon_codex(
        &h,
        "p0",
        "proj",
        &[step("排查左栏滚动卡顿的原因并修复这个性能回归", &a, None)],
        "",
    );
    let turn = |w: &Workspace| {
        w.current_agent("p0")
            .and_then(|s| w.content.get(&s.id))
            .map(|c| (c.turn, c.signal.clone()))
    };
    wait_for(&h, "the prompt binds and its turn runs", |w| {
        turn(w).is_some_and(|(t, _)| t == Turn::Running)
    });
    let log = h.root.join(format!("logs/codex/{a}.jsonl"));
    fs::OpenOptions::new()
        .append(true)
        .open(&log)
        .unwrap()
        .write_all(
            format!(
                "{}\n",
                serde_json::json!({"timestamp":mooring_core::content::now(),"type":"event_msg","payload":{"type":"task_complete"}})
            )
            .as_bytes(),
        )
        .unwrap();
    wait_for(&h, "the log ends the turn", |w| {
        turn(w).is_some_and(|(t, _)| t == Turn::Completed)
    });
    let (mut stream, _, writable) = h.attach("p0", false);
    assert!(writable);
    let mut typed = |line: &str| {
        send(
            &mut stream,
            CONTROL,
            &Request::Input {
                bytes: format!("{line}\r").into_bytes(),
                lossy: false,
            },
        )
        .unwrap();
        assert!(matches!(ack(&mut stream), Reply::Ok));
    };
    // A turn the log has not recorded yet, or whose start fell before the part it read.
    typed("title:⠋ proj");
    wait_for(&h, "a spinning title shows the turn running", |w| {
        turn(w) == Some((Turn::Running, "标题".into()))
    });
    // The log never said this turn ran, so a still title leaves the log's word standing.
    typed("title:proj");
    wait_for(&h, "a still title gives the log its say back", |w| {
        turn(w).is_some_and(|(t, _)| t == Turn::Completed)
    });
    // With the title off, the working line the TUI ticks on screen says the same.
    typed("say:• Working (3s • esc to interrupt)");
    wait_for(&h, "a working line on screen shows the turn running", |w| {
        turn(w) == Some((Turn::Running, "画面在跑".into()))
    });
    typed("clear");
    wait_for(
        &h,
        "the working line gone gives the log its say back",
        |w| turn(w).is_some_and(|(t, _)| t == Turn::Completed),
    );
}

#[test]
fn daemon_codex_short_prompts_bind_to_the_pane_where_enter_was_pressed() {
    let h = Host::start();
    h.setup();
    let [a, b] = ids();
    let gates = [h.root.join("gate-0"), h.root.join("gate-1")];
    // Real Codex logs a prompt when Enter arrives; Enter goes in at +2 s and +6 s below.
    let start = mooring_core::content::now();
    let timed = |thread: &str, gate: &PathBuf, at: u64| DaemonStep {
        at: Some(at),
        ..step("继续", thread, Some(gate))
    };
    // "继续" is too short to be screen evidence; only the Enter press can tell the panes apart.
    daemon_codex(&h, "p0", "proj", &[timed(&a, &gates[0], start + 2)], "");
    daemon_codex(&h, "p1", "proj", &[timed(&b, &gates[1], start + 6)], "");
    std::thread::sleep(Duration::from_secs(2));
    for (pane, gate) in ["p0", "p1"].iter().zip(&gates) {
        let (mut stream, _, writable) = h.attach(pane, false);
        assert!(writable);
        send(
            &mut stream,
            CONTROL,
            &Request::Input {
                bytes: b"\r".to_vec(),
                lossy: false,
            },
        )
        .unwrap();
        assert!(matches!(ack(&mut stream), Reply::Ok));
        fs::write(gate, "").unwrap();
        // Far enough apart that each logged prompt meets only its own Enter.
        std::thread::sleep(Duration::from_secs(4));
    }
    wait_for(
        &h,
        "each short prompt binds to the pane that submitted it",
        |w| current_is(w, "p0", &a) && current_is(w, "p1", &b),
    );
}

#[test]
fn daemon_codex_threads_started_in_the_same_millisecond_bind_correctly() {
    let h = Host::start();
    h.setup();
    let gate = h.root.join("go");
    let [a, b] = ids();
    daemon_codex(
        &h,
        "p0",
        "proj",
        &[step(
            "同一时刻启动的第一个线程：整理发布说明",
            &a,
            Some(&gate),
        )],
        "",
    );
    daemon_codex(
        &h,
        "p1",
        "proj",
        &[step(
            "同一时刻启动的第二个线程：修复构建脚本",
            &b,
            Some(&gate),
        )],
        "",
    );
    wait_for(&h, "both panes running before the shared start", |w| {
        w.terminals
            .iter()
            .filter(|t| t.state == ProcessState::Running)
            .count()
            == 2
    });
    fs::write(&gate, "").unwrap();
    wait_for(&h, "simultaneous threads bind to their own panes", |w| {
        current_is(w, "p0", &a) && current_is(w, "p1", &b)
    });
}

#[test]
fn daemon_codex_new_hands_the_pane_to_the_newer_thread_only() {
    let h = Host::start();
    h.setup();
    let gate = h.root.join("new");
    let [old, new] = ids();
    daemon_codex(
        &h,
        "p0",
        "proj",
        &[
            step("先排查通知点击后没有跳转的原因", &old, None),
            step("新会话：把通知投递日志整理成证据表格", &new, Some(&gate)),
        ],
        "",
    );
    // An idle Codex in the same cwd never received a prompt and must not take the /new thread.
    daemon_codex(&h, "idle", "proj", &[], "");
    wait_for(&h, "first thread binds", |w| current_is(w, "p0", &old));
    fs::write(&gate, "").unwrap();
    wait_for(&h, "/new thread takes over its own pane", |w| {
        current_is(w, "p0", &new)
            && w.agent_sessions
                .iter()
                .any(|s| s.cli_session_id == old && !s.current)
    });
    assert!(h.workspace().current_agent("idle").is_none());
}

#[test]
fn daemon_codex_identical_prompts_stay_unbound_until_a_distinct_follow_up() {
    let h = Host::start();
    h.setup();
    let gate = h.root.join("follow");
    let [a, b] = ids();
    let same = "请对比两个模型对这段代码的重构建议";
    daemon_codex(
        &h,
        "p0",
        "proj",
        &[
            step(same, &a, None),
            step("第一个窗格的追问：只保留性能相关建议", &a, Some(&gate)),
        ],
        "",
    );
    daemon_codex(
        &h,
        "p1",
        "proj",
        &[
            step(same, &b, None),
            step("第二个窗格的追问：只保留可读性相关建议", &b, Some(&gate)),
        ],
        "",
    );
    wait_for(&h, "both identical threads are indexed", |w| {
        [&a, &b]
            .iter()
            .all(|id| w.agent_sessions.iter().any(|s| &s.cli_session_id == *id))
    });
    std::thread::sleep(Duration::from_secs(2));
    let w = h.workspace();
    assert!(
        w.current_agent("p0").is_none() && w.current_agent("p1").is_none(),
        "identical prompts must not bind"
    );
    fs::write(&gate, "").unwrap();
    wait_for(
        &h,
        "distinct follow-ups bind each thread to its own pane",
        |w| current_is(w, "p0", &a) && current_is(w, "p1", &b),
    );
}

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

#[test]
fn unbound_daemon_codex_shows_title_state_and_no_daemon_shows_nothing() {
    let h = Host::start();
    h.setup();
    daemon_codex(&h, "busy", "⠋ proj", &[], "");
    daemon_codex(&h, "plain", "⠋ proj", &[], "--no-daemon");
    // Hints are computed after a submitted command; the fixture has none, so probe via input.
    for pane in ["busy", "plain"] {
        let (mut stream, _, _) = h.attach(pane, false);
        send(
            &mut stream,
            CONTROL,
            &Request::Input {
                bytes: b"\n".to_vec(),
                lossy: false,
            },
        )
        .unwrap();
        assert!(matches!(ack(&mut stream), Reply::Ok));
    }
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if recognition(&h)
            .terminal_hints
            .get("busy")
            .map(String::as_str)
            == Some("仅显示状态：工作中；发下一句后自动关联对话")
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "{:?}",
            recognition(&h).terminal_hints
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(!recognition(&h).terminal_hints.contains_key("plain"));
}
