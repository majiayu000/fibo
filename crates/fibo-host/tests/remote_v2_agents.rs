//! The phone app v2's protocol additions that read an Agent's screen or log, through
//! `fibo remote --stdio`: answer always, stop, chat tools (docs/phone-app/v2-plan.md).
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use fibo_core::{model::*, protocol::*};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn remote_answer_always_takes_dont_ask_again_and_refuses_a_menu_without_it() {
    let h = Host::start();
    let user = json!({"type":"user","uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","message":{"content":"push it"}});
    // Claude Code's menu: option 2 is chosen by its number, without Enter. Then a menu
    // with no such option.
    let script = "stty -icanon -echo; printf '%s\\n' 'Bash command' '  git push origin main' 'Do you want to proceed?' '❯ 1. Yes' \"  2. Yes, and don't ask again for git push commands\" '  3. No, and tell Claude what to do differently (esc)'; k=$(dd bs=1 count=1 2>/dev/null); printf '\\033[2J\\033[Hkey=[%s]\\n' \"$k\"; printf '%s\\n' 'Bash command' '  rm -rf build' 'Do you want to remove?' '❯ 1. Yes' '  2. No'; k=$(dd bs=1 count=1 2>/dev/null); printf 'second=[%s]\\n' \"$k\"; sleep 60";
    h.claude("approve", Path::new("/tmp"), &[user], script);
    let approval = |h: &Host| {
        let w = h.workspace();
        w.current_agent("approve")
            .and_then(|s| w.content.get(&s.id))
            .map(|c| c.approval.clone())
            .unwrap_or_default()
    };
    let wait = |q: &str| {
        let until = Instant::now() + Duration::from_secs(90);
        while approval(&h) != q {
            assert!(
                Instant::now() < until,
                "{q} never showed: {:?}",
                approval(&h)
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    wait("Do you want to proceed?");
    let mut r = Remote::open(&h, None, &[]);
    let answer = |q: &str, req: &str| json!({ "op": "answer", "terminal": "approve", "question": q, "allow": true, "always": true, "request": req });
    let e = r.error(json!({ "op": "answer", "terminal": "approve", "question": "Do you want to proceed?", "allow": false, "always": true }));
    assert!(e.contains("allow: true"), "{e}");
    r.ok(answer("Do you want to proceed?", "a1"));
    h.wait_screen("approve", "key=[2]");
    // Sent again after a lost reply: the first answer comes back, nothing is typed.
    r.ok(answer("Do you want to proceed?", "a1"));
    // Asked until the host reads the new menu off the screen.
    let until = Instant::now() + Duration::from_secs(30);
    for n in 0.. {
        let e = r.error(answer("Do you want to remove?", &format!("a2-{n}")));
        if e.contains("不再询问") {
            break;
        }
        assert!(
            e.contains("已经没有这个审批") && Instant::now() < until,
            "{e}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !h.screen("approve").contains("second="),
        "a refused always types nothing"
    );
    // A plain allow still answers it.
    r.ok(json!({ "op": "answer", "terminal": "approve", "question": "Do you want to remove?", "allow": true, "request": "a3" }));
    h.wait_screen("approve", "second=[]");
}

/// The real Claude Code screen a phone showed the wrong card for: `ask` names the command,
/// an answer about any other command is refused, plain allow is Enter on option 1 and
/// `always` is option 2, never 「switch to auto mode」.
#[test]
fn remote_ask_names_the_command_and_answers_only_about_it() {
    let h = Host::start();
    let user = json!({"type":"user","uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","message":{"content":"touch it"}});
    let screen = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/screens/claude/approval-bash-tip.txt");
    let draw = format!("printf '\\033[2J\\033[H'; cat {}", screen.display());
    let script = format!(
        "stty -icanon -echo; {draw}; k=$(dd bs=1 count=1 2>/dev/null | od -An -c | tr -d ' '); {draw}; printf 'plain=[%s]\\n' \"$k\"; k=$(dd bs=1 count=1 2>/dev/null); printf 'always=[%s]\\n' \"$k\"; sleep 60"
    );
    h.claude("tip", Path::new("/tmp"), &[user], &script);
    let until = Instant::now() + Duration::from_secs(90);
    let mut r = Remote::open(&h, None, &[]);
    let ask = loop {
        let got = r.ask(json!({ "op": "ask", "terminal": "tip" }));
        if got.get("error").is_none() {
            break got["ok"].clone();
        }
        assert!(Instant::now() < until, "ask never answered");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(ask["subject"], "touch second.txt");
    assert_eq!(ask["kind"], "Bash command");
    assert_eq!(ask["note"], "Create empty second.txt file");
    // Its scope in the menu's words, not a blanket 「不再询问」.
    let label = "允许，这个项目以后都能访问 /Users/x/t";
    assert_eq!(ask["always"], label);
    let answer = |subject: &str, always: bool, req: &str| json!({ "op": "answer", "terminal": "tip", "question": "Do you want to proceed?", "allow": true, "always": always, "subject": subject, "request": req });
    // Waits out the settle after input: the host answers only what it has read.
    let e = r.error(answer("git push origin main", false, "w1"));
    assert!(e.contains("换了一条"), "{e}");
    assert!(
        !h.screen("tip").contains("plain="),
        "a refused answer types nothing"
    );
    r.ok(answer("touch second.txt", false, "w2"));
    h.wait_screen("tip", "plain=[\\n]");
    let labelled = |said: &str, req: &str| json!({ "op": "answer", "terminal": "tip", "question": "Do you want to proceed?", "allow": true, "always": true, "subject": "touch second.txt", "always_label": said, "request": req });
    let e = r.error(labelled("允许，git push 命令以后不再询问", "w3"));
    assert!(e.contains("这一项已经变了"), "{e}");
    r.ok(labelled(label, "w4"));
    h.wait_screen("tip", "always=[2]");
}

/// A menu without the option has no words for it: the phone shows no button.
#[test]
fn remote_ask_has_no_always_words_on_a_menu_without_the_option() {
    let h = Host::start();
    let user = json!({"type":"user","uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","message":{"content":"touch it"}});
    let script = "printf '\\033[2J\\033[HBash command\\n\\n  touch second.txt\\n\\nDo you want to proceed?\\n❯ 1. Yes\\n  2. No\\n'; sleep 60";
    h.claude("plain", Path::new("/tmp"), &[user], script);
    let until = Instant::now() + Duration::from_secs(90);
    let mut r = Remote::open(&h, None, &[]);
    let ask = loop {
        let got = r.ask(json!({ "op": "ask", "terminal": "plain" }));
        if got.get("error").is_none() {
            break got["ok"].clone();
        }
        assert!(Instant::now() < until, "ask never answered");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(ask.get("always").is_none(), "{ask}");
}

#[test]
fn remote_stop_interrupts_a_working_agent_and_refuses_anything_else() {
    let h = Host::start();
    let user = json!({"type":"user","uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","message":{"content":"run the tests"}});
    // On Ctrl-C the fake logs an ended turn, as an interrupted Agent's card would settle.
    let ended = h.root.join("ended.jsonl");
    let row = json!({"type":"assistant","uuid":"a9","timestamp":fibo_core::content::now(),"message":{"model":"claude","stop_reason":"end_turn","content":[{"type":"text","text":"stopped"}]}});
    std::fs::write(&ended, format!("{row}\n")).unwrap();
    let script = format!(
        "trap 'echo INTERRUPTED; cat {} >> \"$LOG\"' INT; echo WORKING; while kill -0 $PPID 2>/dev/null; do sleep 1; done",
        shell_quote(&ended.to_string_lossy())
    );
    h.claude("busy", Path::new("/tmp"), &[user], &script);
    h.wait_screen("busy", "WORKING");
    let mut r = Remote::open(&h, None, &[]);
    let phase = |r: &mut Remote| {
        let list = r.ok(json!({ "op": "list" }));
        let cards = list["terminals"].as_array().unwrap();
        let card = cards.iter().find(|c| c["id"] == "busy").cloned();
        card.map_or(Value::Null, |c| c["phase"].clone())
    };
    let until = Instant::now() + Duration::from_secs(30);
    while phase(&mut r) != "working" {
        assert!(Instant::now() < until, "never working");
        std::thread::sleep(Duration::from_millis(100));
    }
    r.ok(json!({ "op": "stop", "terminal": "busy", "request": "s1" }));
    h.wait_screen("busy", "INTERRUPTED");
    // The Agent survives an interrupt: the terminal still runs.
    assert!(h
        .workspace()
        .terminals
        .iter()
        .any(|t| t.id == "busy" && t.state == ProcessState::Running));
    let until = Instant::now() + Duration::from_secs(30);
    while phase(&mut r) == "working" {
        assert!(Instant::now() < until, "the ended turn never showed");
        std::thread::sleep(Duration::from_millis(100));
    }
    // It no longer works: a resend after a lost reply still gets the first reply and types
    // nothing, a new stop is refused.
    r.ok(json!({ "op": "stop", "terminal": "busy", "request": "s1" }));
    let e = r.error(json!({ "op": "stop", "terminal": "busy", "request": "s1b" }));
    assert!(e.contains("没有要中断的"), "{e}");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(h.screen("busy").matches("INTERRUPTED").count(), 1);

    h.launch("plain", Path::new("/tmp"), "sleep 60");
    let e = r.error(json!({ "op": "stop", "terminal": "plain", "request": "s2" }));
    assert!(e.contains("没有 Agent 对话"), "{e}");
    let mut answer = Remote::open(&h, Some("answer"), &[]);
    let e = answer.error(json!({ "op": "stop", "terminal": "busy", "request": "s3" }));
    assert!(e.contains("不能 stop"), "{e}");
}

/// A fake Codex 0.166 at work: it names each key it gets, logs an aborted turn on `ends`,
/// and on Ctrl-C draws the menu the real one does, its cursor on `cursor`.
fn fake_codex(h: &Host, id: &str, ends: &[&str], cursor: u8) {
    fake_codex_saying(h, id, ends, cursor, "")
}
/// With `esc` printed on Esc: what the real one draws once it has interrupted.
fn fake_codex_saying(h: &Host, id: &str, ends: &[&str], cursor: u8, esc: &str) {
    let now = fibo_core::content::now();
    let started = json!({"type":"event_msg","timestamp":now,"payload":{"type":"task_started","turn_id":"t1"}});
    let aborted = json!({"type":"event_msg","timestamp":now,"payload":{"type":"turn_aborted"}});
    let ended = h.root.join(format!("ended-{id}"));
    std::fs::write(&ended, format!("{aborted}\n")).unwrap();
    let mark = |n: u8| if n == cursor { "›" } else { " " };
    let menu = format!(
        "printf '\\n  Task is still running\\n  Choose what happens to the current task.\\n\\n{} 1. Cancel task        Stop the current task and stay in Codex\\n{} 2. Run in background  Exit Codex and leave the task running\\n{} 3. Exit               Stop the current task and exit Codex\\n\\n  enter select · esc back\\n'",
        mark(1), mark(2), mark(3)
    );
    let path = h.root.join("logs/codex");
    let rollout = format!("$(grep -l 'marker-{id}\"' {}/* | head -1)", path.display());
    // Its own file: a `$(...)` in the fake itself would be a second process named codex,
    // taken for the Agent and gone at every key.
    let keys = h.root.join(format!("keys-{id}.sh"));
    let body = format!(
        "stty -icanon -echo -isig; printf '• Working (1s • esc to interrupt)\\n'; while kill -0 $PPID 2>/dev/null; do k=$(dd bs=1 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n'); keys=\"$keys $k\"; [ \"$k\" = 1b ] && printf '\\033[2J\\033[H{esc}\\n'; printf 'KEY=%s\\n' \"$k\"; [ \"$k\" = 03 ] && {menu}; case ' {} ' in *\" $k \"*) printf '\\033[2J\\033[H{esc}\\nkeys:%s\\n' \"$keys\"; cat {} >> {rollout};; esac; done",
        ends.join(" "),
        shell_quote(&ended.to_string_lossy()),
    );
    std::fs::write(&keys, body).unwrap();
    let script = format!("/bin/sh {}", shell_quote(&keys.to_string_lossy()));
    let marker = json!({"type":"response_item","timestamp":now,"payload":{"type":"reasoning","summary":[],"id":format!("marker-{id}")}});
    h.codex(id, Path::new("/tmp"), &[marker, started], &script);
}
/// Asks until the list says `want` about terminal `id`.
fn wait_phase(r: &mut Remote, id: &str, want: &str) {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let list = r.ok(json!({ "op": "list" }));
        let cards = list["terminals"].as_array().unwrap();
        let card = cards.iter().find(|c| c["id"] == id);
        if card.is_some_and(|c| c["phase"] == want) {
            return;
        }
        assert!(Instant::now() < until, "{id} never {want}: {card:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Codex: Esc, never Ctrl-C, which opens a menu on a running turn and quits an idle one.
/// The reply waits until the turn is over, and the Agent is still there.
#[test]
fn remote_stop_on_codex_presses_esc_and_replies_once_it_stopped() {
    let h = Host::start();
    fake_codex(&h, "esc", &["1b"], 1);
    let mut r = Remote::open(&h, None, &[]);
    wait_phase(&mut r, "esc", "working");
    r.ok(json!({ "op": "stop", "terminal": "esc", "request": "s1" }));
    let screen = h.screen("esc");
    assert!(screen.contains("keys: 1b"), "{screen}");
    let list = r.ok(json!({ "op": "list" }));
    let cards = list["terminals"].as_array().unwrap();
    let card = cards.iter().find(|c| c["id"] == "esc").unwrap();
    assert_ne!(card["phase"], "working", "replied before it stopped");
    assert!(card["session"].is_string(), "the Agent stays");
}

/// The window's 中断这一轮 sends these same two requests straight to the host: the key is the
/// CLI's, the reply comes when the turn has ended, and an idle Codex gets no key at all
/// (Ctrl-C there quits it).
#[test]
fn the_windows_interrupt_on_codex_is_esc_and_hears_when_it_stopped() {
    let h = Host::start();
    fake_codex(&h, "esc", &["1b"], 1);
    let mut r = Remote::open(&h, None, &[]);
    wait_phase(&mut r, "esc", "working");
    let id = "esc".to_owned();
    h.call(Request::Interrupt { id: id.clone() }).unwrap();
    let ended = h.call(Request::AwaitInterrupt { id: id.clone() }).unwrap();
    assert!(
        matches!(
            ended,
            Reply::Interrupted {
                closed_background: false
            }
        ),
        "{ended:?}"
    );
    let screen = h.screen("esc");
    assert!(screen.contains("keys: 1b"), "{screen}");
    // The state says what happened: stopped by you, not a turn waiting for you to check.
    let list = r.ok(json!({ "op": "list" }));
    let cards = list["terminals"].as_array().unwrap();
    let card = cards.iter().find(|c| c["id"] == "esc").unwrap();
    assert_eq!(card["status_label"], "停下了", "{card}");
    let e = h.call(Request::Interrupt { id: id.clone() }).unwrap_err();
    assert!(e.to_string().contains("没有要中断的"), "{e}");
    std::thread::sleep(Duration::from_millis(300));
    let screen = h.screen("esc");
    assert!(
        !screen.contains("KEY="),
        "a refused one presses nothing: {screen}"
    );
}

/// The menu a Ctrl-C from elsewhere left up: only Enter, only with the cursor on Cancel
/// task. Esc there would go back to the running turn; Enter elsewhere could quit it.
#[test]
fn remote_stop_answers_the_cancel_menu_only_with_the_cursor_on_cancel() {
    let h = Host::start();
    fake_codex(&h, "on", &["0a", "0d"], 1);
    fake_codex(&h, "exit", &[], 3);
    let mut r = Remote::open(&h, None, &[]);
    wait_phase(&mut r, "on", "working");
    wait_phase(&mut r, "exit", "working");
    for id in ["on", "exit"] {
        r.ok(json!({ "op": "keys", "terminal": id, "keys": ["ctrl-c"], "request": format!("k-{id}") }));
        h.wait_screen(id, "enter select");
    }
    r.ok(json!({ "op": "stop", "terminal": "on", "request": "s1" }));
    let screen = h.screen("on");
    assert!(
        screen.contains("keys: 03 0a") || screen.contains("keys: 03 0d"),
        "{screen}"
    );
    assert!(!screen.contains("KEY=1b"), "{screen}");

    let e = r.error(json!({ "op": "stop", "terminal": "exit", "request": "s2" }));
    assert!(e.contains("Cancel task"), "{e}");
    let screen = h.screen("exit");
    assert!(
        !screen.contains("KEY=0a") && !screen.contains("KEY=0d"),
        "{screen}"
    );
    assert!(!screen.contains("KEY=1b"), "{screen}");
}

const LEFT_RUNNING: &str = "1 background terminal running · /ps to view · /stop to close";

/// Esc ends the turn on screen but its command runs on (real Codex 0.166): `sleep 60` is
/// only over once `/stop` closes it, and the log says the turn runs until then.
#[test]
fn remote_stop_closes_the_command_an_interrupted_turn_left_running() {
    let h = Host::start();
    fake_codex_saying(&h, "left", &["0a", "0d"], 1, LEFT_RUNNING);
    let mut r = Remote::open(&h, None, &[]);
    wait_phase(&mut r, "left", "working");
    let ok = r.ok(json!({ "op": "stop", "terminal": "left", "request": "s1" }));
    assert_eq!(ok["closed_background"], true);
    let screen = h.screen("left");
    assert!(screen.contains("keys: 1b 2f 73 74 6f 70"), "{screen}");
}

/// A server from an earlier turn: the turn ends by itself at Esc, so nothing is closed.
#[test]
fn remote_stop_leaves_a_server_alone_when_the_turn_ended_by_itself() {
    let h = Host::start();
    fake_codex_saying(&h, "server", &["1b"], 1, LEFT_RUNNING);
    let mut r = Remote::open(&h, None, &[]);
    wait_phase(&mut r, "server", "working");
    let ok = r.ok(json!({ "op": "stop", "terminal": "server", "request": "s1" }));
    assert_eq!(ok["closed_background"], false);
    std::thread::sleep(Duration::from_millis(2000));
    let screen = h.screen("server");
    assert!(
        screen.contains("keys: 1b") && !screen.contains("2f"),
        "{screen}"
    );
}

/// A key is not a stop: an Agent that keeps working gets an error, not a quiet success.
#[test]
fn remote_stop_says_so_when_the_agent_keeps_working() {
    let h = Host::start();
    fake_codex(&h, "deaf", &[], 1);
    let mut r = Remote::open(&h, None, &[]);
    wait_phase(&mut r, "deaf", "working");
    let e = r.error(json!({ "op": "stop", "terminal": "deaf", "request": "s1" }));
    assert!(e.contains("没有停"), "{e}");
    assert!(h.screen("deaf").contains("KEY=1b"));
}

#[test]
fn remote_chat_tools_sum_what_the_agent_did_before_each_message() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let cwd = work.to_string_lossy().into_owned();
    let user = |u: &str, text: &str| json!({"type":"user","uuid":u,"cwd":cwd,"entrypoint":"cli","userType":"external","message":{"content":text}});
    let said = |u: &str, content: Value| json!({"type":"assistant","uuid":u,"message":{"content":content}});
    let result = |u: &str, id: &str| json!({"type":"user","uuid":u,"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}});
    let rows = [
        user("u1", "fix it"),
        // Claude Code logs each block of a reply on a line of its own.
        said("a1", json!([{"type":"text","text":"looking"}])),
        said(
            "a1b",
            json!([{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cargo test"}}]),
        ),
        result("r1", "t1"),
        said(
            "a2",
            json!([{"type":"tool_use","id":"t2","name":"Edit","input":{"file_path":"/r/a.rs","old_string":"x","new_string":"y\nz"}}]),
        ),
        result("r2", "t2"),
        said(
            "a3",
            json!([{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"cargo build"}}]),
        ),
        result("r3", "t3"),
        // An edit that failed changed nothing.
        said(
            "a3b",
            json!([{"type":"tool_use","id":"t4","name":"Edit","input":{"file_path":"/r/failed.rs","old_string":"x","new_string":"y"}}]),
        ),
        json!({"type":"user","uuid":"r4","message":{"content":[{"type":"tool_result","tool_use_id":"t4","is_error":true,"content":"no match"}]}}),
        said("a4", json!([{"type":"text","text":"done"}])),
        user("u2", "thanks"),
        said("a5", json!([{"type":"text","text":"bye"}])),
    ];
    h.claude("chat", &work, &rows, "sleep 60");
    let mut r = Remote::open(&h, None, &[("HOME", h.root.as_path())]);
    let until = Instant::now() + Duration::from_secs(30);
    let page = loop {
        let page = r.ok(json!({ "op": "chat", "terminal": "chat", "tools": true }));
        if page["messages"].as_array().unwrap().len() == 5 {
            break page;
        }
        assert!(Instant::now() < until, "chat never filled: {page}");
        std::thread::sleep(Duration::from_millis(100));
    };
    let m = page["messages"].as_array().unwrap();
    let texts: Vec<&str> = m.iter().map(|m| m["text"].as_str().unwrap()).collect();
    assert_eq!(texts, ["fix it", "looking", "done", "thanks", "bye"]);
    assert!(
        m[0].get("tools").is_none() && m[3].get("tools").is_none(),
        "users carry none"
    );
    assert_eq!(m[1]["tools"], json!({ "commands": 0, "files": [] }));
    assert_eq!(
        m[2]["tools"],
        json!({ "commands": 2, "calls": 4, "files": [{ "path": "/r/a.rs", "added": 2, "removed": 1 }] })
    );
    assert_eq!(m[4]["tools"], json!({ "commands": 0, "files": [] }));
    // A page that starts mid-turn still counts from the message before it.
    let seq = m[2]["seq"].as_i64().unwrap();
    let one = r.ok(
        json!({ "op": "chat", "terminal": "chat", "tools": true, "before": seq + 1, "limit": 1 }),
    );
    assert_eq!(one["messages"][0]["tools"], m[2]["tools"]);
    let bare = r.ok(json!({ "op": "chat", "terminal": "chat" }));
    assert!(bare["messages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m.get("tools").is_none()));

    // The folder picker counts this live conversation where it works.
    let dirs = r.ok(json!({ "op": "dirs", "path": h.root.to_string_lossy() }));
    let entry = dirs["dirs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "work")
        .unwrap()
        .clone();
    assert_eq!(
        (entry["sessions"].as_u64(), entry["recent"].as_bool()),
        (Some(1), Some(true)),
        "{entry}"
    );
}
