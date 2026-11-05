//! The phone app v2's protocol additions that read an Agent's screen or log, through
//! `mooring remote --stdio`: answer always, stop, chat tools (docs/phone-app/v2-plan.md).
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use mooring_core::model::*;
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

#[test]
fn remote_stop_interrupts_a_working_agent_and_refuses_anything_else() {
    let h = Host::start();
    let user = json!({"type":"user","uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","message":{"content":"run the tests"}});
    // On Ctrl-C the fake logs an ended turn, as an interrupted Agent's card would settle.
    let ended = h.root.join("ended.jsonl");
    let row = json!({"type":"assistant","uuid":"a9","timestamp":mooring_core::content::now(),"message":{"model":"claude","stop_reason":"end_turn","content":[{"type":"text","text":"stopped"}]}});
    std::fs::write(&ended, format!("{row}\n")).unwrap();
    let script = format!(
        "trap 'echo INTERRUPTED; cat {} >> \"$LOG\"' INT; echo WORKING; while :; do sleep 1; done",
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
