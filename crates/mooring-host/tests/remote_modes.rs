//! Changing the model and permission mode through `mooring remote --stdio`, on fake CLIs that
//! draw what the real ones drew (fixtures/mode-screens, Claude Code 2.1.319, Codex 0.167) and
//! write every key they get to a file: what was pressed is what these tests read.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use mooring_core::model::shell_quote;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const FAKE: &str = include_str!("remote_modes_fake.py");

/// A fake `which` (claude or codex), starting as the flags say, bound to a conversation.
fn start(h: &Host, id: &str, which: &str, flags: &str) -> std::path::PathBuf {
    start_with(h, id, which, flags, true)
}
fn start_with(h: &Host, id: &str, which: &str, flags: &str, ended: bool) -> std::path::PathBuf {
    let fake = h.root.join("fake.py");
    std::fs::write(&fake, FAKE).unwrap();
    let keys = h.root.join(format!("{id}.keys"));
    let run = format!(
        "python3 {} {which} {} {flags}",
        shell_quote(&fake.to_string_lossy()),
        shell_quote(&keys.to_string_lossy()),
    );
    let cwd = std::path::Path::new("/tmp");
    match which {
        "claude" => {
            let mut rows = vec![
                json!({"type":"user","uuid":"u1","cwd":"/tmp","entrypoint":"cli","userType":"external","permissionMode":"default","message":{"content":"hi"}}),
                json!({"type":"assistant","uuid":"a1","message":{"model":"claude-opus-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"ok"}]}}),
            ];
            rows.truncate(if ended { 2 } else { 1 });
            h.claude(id, cwd, &rows, &run);
        }
        _ => {
            h.codex(id, cwd, &[], &run);
        }
    }
    h.wait_screen(id, if which == "claude" { "❯" } else { "›" });
    keys
}
fn pressed(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}
fn card(r: &mut Remote, id: &str) -> Value {
    let list = r.ok(json!({ "op": "list" }));
    let cards = list["terminals"].as_array().unwrap();
    cards.iter().find(|c| c["id"] == id).unwrap().clone()
}
fn labels(o: &Value) -> Vec<String> {
    (o["options"].as_array().unwrap().iter())
        .map(|x| x["key"].as_str().unwrap().to_owned())
        .collect()
}
fn set(r: &mut Remote, id: &str, kind: &str, choice: &str) -> Value {
    r.ok(json!({ "op": "setmode", "terminal": id, "kind": kind, "choice": choice }))
}
fn refused(r: &mut Remote, id: &str, kind: &str, choice: &str) -> String {
    r.error(json!({ "op": "setmode", "terminal": id, "kind": kind, "choice": choice }))
}
fn full(r: &mut Remote, id: &str, choice: &str) -> Value {
    r.ok(json!({ "op": "setmode", "terminal": id, "kind": "permission", "choice": choice, "allow_full": true }))
}

#[test]
fn claude_model_comes_from_its_menu_and_changes_for_this_session_only() {
    let h = Host::start();
    let keys = start(&h, "c", "claude", "");
    let mut r = Remote::open(&h, None, &[]);
    let o = r.ok(json!({ "op": "modes", "terminal": "c", "kind": "model" }));
    assert_eq!(
        labels(&o),
        ["Default (recommended)", "Opus 5.5", "Sonnet 5.5"],
        "{o}"
    );
    assert_eq!(o["options"][1]["current"], true);
    assert_eq!(
        o["options"][2]["detail"],
        "Most efficient for simpler tasks"
    );
    h.wait_screen("c", "manual mode on");
    let now = set(&mut r, "c", "model", "Sonnet 5.5");
    assert_eq!(now["model"], "Sonnet 5.5");
    // `s`, never Enter on the row: Enter makes it the default for every new session.
    assert!(
        pressed(&keys).contains("PICK-SESSION"),
        "{}",
        pressed(&keys)
    );
    assert!(
        !pressed(&keys).contains("PICK-DEFAULT"),
        "{}",
        pressed(&keys)
    );
    assert!(!pressed(&keys).contains("SENT"), "{}", pressed(&keys));
    assert_eq!(card(&mut r, "c")["model"], "Sonnet 5.5");
}

/// Real Claude Code asks once a conversation is cached on the model in use.
#[test]
fn claudes_question_about_a_cached_model_is_answered_for_that_model() {
    let h = Host::start();
    let keys = start(&h, "c", "claude", "--cached");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "manual mode on");
    let now = set(&mut r, "c", "model", "Sonnet 5.5");
    assert_eq!(now["model"], "Sonnet 5.5");
    assert!(pressed(&keys).contains("SWITCH-YES"), "{}", pressed(&keys));
    assert!(
        !pressed(&keys).contains("PICK-DEFAULT"),
        "{}",
        pressed(&keys)
    );
}

/// A half-written line must not go out as a message when Enter takes `/model`.
#[test]
fn text_waiting_in_the_box_is_left_there_and_nothing_is_sent() {
    let h = Host::start();
    let keys = start(&h, "c", "claude", "--draft=half");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "❯ half");
    let e = refused(&mut r, "c", "model", "Sonnet 5.5");
    assert!(e.contains("没发的字"), "{e}");
    assert!(!pressed(&keys).contains("SENT"), "{}", pressed(&keys));
    h.wait_screen("c", "❯ half");
    assert!(!h.screen("c").contains("half/model"), "{}", h.screen("c"));
}

#[test]
fn nothing_is_pressed_while_it_works_or_asks() {
    let h = Host::start();
    let keys = start(&h, "c", "claude", "--busy");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "esc to interrupt");
    for (kind, choice) in [("model", "Sonnet 5.5"), ("permission", "plan")] {
        let e = refused(&mut r, "c", kind, choice);
        assert!(e.contains("等它停下"), "{e}");
    }
    let e = r.error(json!({ "op": "modes", "terminal": "c", "kind": "model" }));
    assert!(e.contains("等它停下"), "{e}");
    assert_eq!(pressed(&keys), "", "not one key");

    let keys = start(&h, "d", "claude", "--dialog");
    h.wait_screen("d", "Esc to cancel");
    let e = refused(&mut r, "d", "permission", "plan");
    assert!(e.contains("别的提示") || e.contains("答完再换"), "{e}");
    assert_eq!(pressed(&keys), "", "not one key");
}

/// The state says so though the screen shows a prompt: a turn just asked for, say.
#[test]
fn a_turn_the_log_says_is_running_is_left_alone() {
    let h = Host::start();
    let keys = start_with(&h, "c", "claude", "", false);
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "manual mode on");
    let until = Instant::now() + Duration::from_secs(20);
    while card(&mut r, "c")["phase"] != "working" {
        assert!(Instant::now() < until, "{}", card(&mut r, "c"));
        std::thread::sleep(Duration::from_millis(50));
    }
    let e = refused(&mut r, "c", "permission", "plan");
    assert!(e.contains("等它停下"), "{e}");
    assert_eq!(pressed(&keys), "");
}

#[test]
fn claude_permission_walks_shift_tab_and_reads_the_footer() {
    let h = Host::start();
    let keys = start(&h, "c", "claude", "");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "manual mode on");
    let o = r.ok(json!({ "op": "modes", "terminal": "c", "kind": "permission" }));
    assert_eq!(labels(&o), ["manual", "acceptEdits", "plan", "auto"], "{o}");
    assert!(
        o["note"].as_str().unwrap().contains("--allow-dangerously"),
        "{o}"
    );
    assert_eq!(pressed(&keys), "", "reading its modes presses nothing");
    let now = set(&mut r, "c", "permission", "plan");
    assert_eq!(now["permission"], "只做计划");
    h.wait_screen("c", "plan mode on");
    assert_eq!(pressed(&keys).matches("SHIFT-TAB").count(), 2);
    assert_eq!(card(&mut r, "c")["permission"], "只做计划");
}

/// The mode that stops asking: only with `allow_full`, only if the session was started able.
#[test]
fn claude_goes_to_bypass_only_when_asked_and_able() {
    let h = Host::start();
    let keys = start(&h, "n", "claude", "");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("n", "manual mode on");
    let e = refused(&mut r, "n", "permission", "bypassPermissions");
    assert!(e.contains("确认"), "{e}");
    let e = r.error(json!({ "op": "setmode", "terminal": "n", "kind": "permission", "choice": "bypassPermissions", "allow_full": true }));
    assert!(e.contains("--allow-dangerously"), "{e}");
    assert_eq!(pressed(&keys), "", "not one key");

    let keys = start(&h, "y", "claude", "--allow-dangerously-skip-permissions");
    h.wait_screen("y", "manual mode on");
    let o = r.ok(json!({ "op": "modes", "terminal": "y", "kind": "permission" }));
    let bypass = (o["options"].as_array().unwrap().iter())
        .find(|x| x["key"] == "bypassPermissions")
        .unwrap();
    assert_eq!(bypass["full"], true);
    for loose in [json!("true"), json!(1), json!(null), json!(false)] {
        let e = r.error(json!({ "op": "setmode", "terminal": "y", "kind": "permission", "choice": "bypassPermissions", "allow_full": loose }));
        assert!(e.contains("确认"), "{loose}: {e}");
    }
    assert_eq!(pressed(&keys), "");
    // On the way past it would pass the mode that stops asking: refused, not walked.
    let e = refused(&mut r, "y", "permission", "auto");
    assert!(e.contains("经过全部放行"), "{e}");
    assert_eq!(pressed(&keys), "");
    let now = full(&mut r, "y", "bypassPermissions");
    assert_eq!(now["permission"], "全部放行");
    h.wait_screen("y", "bypass permissions on");
    assert_eq!(card(&mut r, "y")["permission"], "全部放行");
    // Leaving it needs no confirmation.
    let now = set(&mut r, "y", "permission", "auto");
    assert_eq!(now["permission"], "自动判断");
    h.wait_screen("y", "auto mode on");
}

/// A cycle that holds the bypass mode though nothing said so is never left resting on it.
#[test]
fn an_unforeseen_bypass_on_the_way_is_passed_not_stopped_at() {
    let h = Host::start();
    let keys = start(&h, "c", "claude", "--bypass-unseen");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "plan mode on");
    let now = set(&mut r, "c", "permission", "auto");
    assert_eq!(now["permission"], "自动判断");
    h.wait_screen("c", "auto mode on");
    assert_eq!(pressed(&keys).matches("SHIFT-TAB").count(), 2);
}

#[test]
fn a_mode_this_session_lacks_comes_back_to_where_it_was() {
    let h = Host::start();
    let keys = start(&h, "c", "claude", "--no-auto");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "manual mode on");
    let e = refused(&mut r, "c", "permission", "auto");
    assert!(e.contains("没有这一档"), "{e}");
    h.wait_screen("c", "manual mode on");
    assert_eq!(pressed(&keys).matches("SHIFT-TAB").count(), 3);
    assert_eq!(card(&mut r, "c")["permission"], "逐个确认", "unchanged");
}

#[test]
fn codex_model_scrolls_its_menu_and_reads_the_footer() {
    let h = Host::start();
    let keys = start(&h, "c", "codex", "");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "GPT-6.1-Sol default");
    let o = r.ok(json!({ "op": "modes", "terminal": "c", "kind": "model" }));
    assert_eq!(
        labels(&o).len(),
        4,
        "the row scrolled into view counts: {o}"
    );
    assert!(labels(&o).contains(&"GPT-5.6-Luna".to_owned()), "{o}");
    assert_eq!(o["options"][0]["current"], true);
    h.wait_screen("c", "GPT-6.1-Sol default");
    let now = set(&mut r, "c", "model", "GPT-6-Luna");
    assert_eq!(now["model"], "GPT-6-Luna medium");
    assert!(
        pressed(&keys).contains("PICK-SESSION"),
        "{}",
        pressed(&keys)
    );
    assert!(
        !pressed(&keys).contains("PICK-DEFAULT"),
        "{}",
        pressed(&keys)
    );
    assert_eq!(card(&mut r, "c")["model"], "GPT-6-Luna medium");
}

#[test]
fn codex_full_access_answers_its_question_and_reads_back_what_is_current() {
    let h = Host::start();
    let keys = start(&h, "c", "codex", "");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "GPT-6.1-Sol default");
    let o = r.ok(json!({ "op": "modes", "terminal": "c", "kind": "permission" }));
    assert_eq!(
        labels(&o),
        [
            "Ask for approval",
            "Approve for me",
            "Full Access",
            "Read Only"
        ],
        "{o}"
    );
    assert_eq!(o["options"][2]["full"], true);
    assert_eq!(o["options"][0]["full"], false);
    assert_eq!(o["options"][2]["label"], "全部放行");

    let before = pressed(&keys);
    let e = refused(&mut r, "c", "permission", "Full Access");
    assert!(e.contains("确认"), "{e}");
    let e = refused(&mut r, "c", "permission", "A row it never had");
    assert!(e.contains("确认"), "an unknown row counts as full: {e}");
    assert_eq!(pressed(&keys), before, "not one key");

    let now = full(&mut r, "c", "Full Access");
    assert_eq!(now["permission"], "全部放行");
    assert!(pressed(&keys).contains("FULL-YES"), "{}", pressed(&keys));
    assert_eq!(card(&mut r, "c")["permission"], "全部放行");

    let now = set(&mut r, "c", "permission", "Read Only");
    assert_eq!(now["permission"], "只读");
    assert_eq!(card(&mut r, "c")["permission"], "只读");
}

/// It says the key did nothing rather than that it worked.
#[test]
fn a_codex_that_stays_where_it_was_is_reported_as_that() {
    let h = Host::start();
    let _keys = start(&h, "c", "codex", "--stuck");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "GPT-6.1-Sol default");
    let e = refused(&mut r, "c", "permission", "Read Only");
    assert!(e.contains("它现在是"), "{e}");
    assert!(e.contains("逐个确认"), "says what it really is: {e}");
    assert!(card(&mut r, "c")["permission"].is_null());
}

#[test]
fn a_question_unlike_the_one_read_gets_no_yes() {
    let h = Host::start();
    let keys = start(&h, "c", "codex", "--other-confirm");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "GPT-6.1-Sol default");
    let e = r.error(json!({ "op": "setmode", "terminal": "c", "kind": "permission", "choice": "Full Access", "allow_full": true }));
    assert!(e.contains("没有放行") || e.contains("没见过"), "{e}");
    assert!(!pressed(&keys).contains("FULL-YES"), "{}", pressed(&keys));
}

#[test]
fn other_terminals_and_lesser_devices_cannot() {
    let h = Host::start();
    h.launch("s", std::path::Path::new("/tmp"), "exec sleep 60");
    let mut r = Remote::open(&h, None, &[]);
    let e = r.error(json!({ "op": "modes", "terminal": "s", "kind": "model" }));
    assert!(e.contains("没有 Agent"), "{e}");
    let keys = start(&h, "c", "claude", "");
    for role in ["view", "answer"] {
        let mut lesser = Remote::open(&h, Some(role), &[]);
        let e = lesser.error(json!({ "op": "modes", "terminal": "c", "kind": "model" }));
        assert!(e.contains("不能 modes"), "{e}");
        let e = lesser.error(
            json!({ "op": "setmode", "terminal": "c", "kind": "permission", "choice": "plan" }),
        );
        assert!(e.contains("不能 setmode"), "{e}");
    }
    assert_eq!(pressed(&keys), "");
}

/// `keys` is the other way to the same footer, so it cannot be one for an Agent.
#[test]
fn shift_tab_through_keys_is_refused_for_an_agent() {
    let h = Host::start();
    let keys = start(&h, "c", "claude", "--allow-dangerously-skip-permissions");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "manual mode on");
    let e = r.error(json!({ "op": "keys", "terminal": "c", "keys": ["shift-tab"] }));
    assert!(e.contains("权限标签"), "{e}");
    assert_eq!(pressed(&keys), "");
    r.ok(json!({ "op": "keys", "terminal": "c", "keys": ["down"] }));
    h.launch("s", std::path::Path::new("/tmp"), "exec sleep 60");
    r.ok(json!({ "op": "keys", "terminal": "s", "keys": ["shift-tab"] }));
}

#[test]
fn a_resend_of_a_named_change_does_not_press_again() {
    let h = Host::start();
    let keys = start(&h, "c", "claude", "");
    let mut r = Remote::open(&h, None, &[]);
    h.wait_screen("c", "manual mode on");
    let ask = json!({ "op": "setmode", "terminal": "c", "kind": "permission", "choice": "acceptEdits", "request": "m1" });
    r.ok(ask.clone());
    let once = pressed(&keys);
    r.ok(ask);
    let until = Instant::now() + Duration::from_millis(400);
    while Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(pressed(&keys), once);
}
