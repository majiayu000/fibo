//! `suggestion` through `mooring remote --stdio`, off a real PTY running a script that draws
//! Claude Code's input box.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn user(cwd: &std::path::Path) -> Value {
    json!({"type":"user","uuid":"u1","cwd":cwd,"entrypoint":"cli","userType":"external","message":{"content":"go"}})
}

/// A script drawing the box with `input` after the prompt, then waiting.
fn the_box(input: &str) -> String {
    let rule = "─".repeat(40);
    format!("printf '\\033[2J\\033[H{rule}\\r\\n❯ {input}\\033[0m\\r\\n{rule}\\r\\n'; sleep 60")
}

fn until_answer(r: &mut Remote, terminal: &str, wanted: &str) -> Value {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let got = r.ok(json!({ "op": "suggestion", "terminal": terminal }));
        if got["text"].as_str() == Some(wanted) {
            return got;
        }
        assert!(Instant::now() < until, "never offered {wanted:?}: {got}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_phone_reads_what_the_box_suggests_and_nothing_else() {
    let h = Host::start();
    let (a, b, c, work) = (
        scratch(&h, "a"),
        scratch(&h, "b"),
        scratch(&h, "c"),
        scratch(&h, "work"),
    );
    h.claude(
        "faint",
        &a,
        &[user(&a)],
        &the_box("\\033[2mrun the tests\\033[22m"),
    );
    h.claude("typed", &b, &[user(&b)], &the_box("run the tests"));
    h.claude(
        "hint",
        &c,
        &[user(&c)],
        &the_box("\\033[2mTry \"edit a.rs\"\\033[22m"),
    );
    h.wait_screen("typed", "run the tests");
    h.wait_screen("hint", "edit a.rs");

    // Read-only: a view-only phone may ask.
    let mut r = Remote::open(&h, Some("view"), &[]);
    assert!(r.ops().iter().any(|o| o == "suggestion"));
    let got = until_answer(&mut r, "faint", "run the tests");
    // The same name the phone's chat page holds, which is how it drops another turn's words.
    let page = r.ok(json!({ "op": "chat", "terminal": "faint" }));
    assert_eq!(got["session"], page["session"]);

    for terminal in ["typed", "hint"] {
        let got = r.ok(json!({ "op": "suggestion", "terminal": terminal }));
        assert!(got.get("text").is_none(), "{terminal}: {got}");
    }

    // A terminal with no Agent answers that it has none, not an error a phone would show.
    h.launch("shell", &work, &the_box("\\033[2mrun the tests\\033[22m"));
    h.wait_screen("shell", "run the tests");
    let got = r.ok(json!({ "op": "suggestion", "terminal": "shell" }));
    assert_eq!(got, json!({}));
    assert!(r
        .error(json!({ "op": "suggestion", "terminal": "gone" }))
        .contains("not found"));
}
