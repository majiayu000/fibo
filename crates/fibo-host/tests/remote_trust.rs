//! An Agent asking to be trusted with a new folder, through `mooring remote --stdio`: the
//! card says it waits, and the keys go out only as the screen allows. The screens are the
//! real ones (fixtures/screens/*/trust-real.txt); Claude Code's starts on 「No, exit」.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use fibo_core::model::shell_quote;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

const FOLDER: &str = "~/t/new-folder";

fn screen(cli: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/screens/{cli}/trust-real.txt"));
    std::fs::read_to_string(path).unwrap()
}

/// A fake CLI showing `first`; each arrow past the first swallowed `swallow` shows `then`.
/// It names what Enter or Esc chose: the cursor's line on the screen it last drew.
fn fake(h: &Host, id: &str, first: &str, then: &str, swallow: u32, yes_first: bool) {
    let write = |name: &str, text: &str| {
        let path = h.root.join(format!("{id}-{name}"));
        std::fs::write(&path, text).unwrap();
        shell_quote(&path.to_string_lossy())
    };
    let (a, b) = (write("a", first), write("b", then));
    let (start, moved) = if yes_first {
        ("yes", "no")
    } else {
        ("no", "yes")
    };
    let script = write(
        "sh",
        &format!(
            "stty -icanon -echo -isig; at={start}; n=0; cat {a}; while kill -0 $PPID 2>/dev/null; do k=$(dd bs=3 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n'); case \"$k\" in 1b) printf 'CHOSE=esc\\n';; 1b5b4?|1b4f4?) n=$((n+1)); if [ $n -gt {swallow} ]; then at={moved}; printf '\\033[2J\\033[H'; cat {b}; fi;; 0a|0d) printf 'CHOSE=%s\\n' $at;; esac; done"
        ),
    );
    h.launch(id, Path::new("/tmp"), &format!("exec /bin/sh {script}"));
}

fn card(r: &mut Remote, id: &str) -> Value {
    let list = r.ok(json!({ "op": "list" }));
    let cards = list["terminals"].as_array().unwrap();
    cards.iter().find(|c| c["id"] == id).unwrap().clone()
}
fn wait_trust(r: &mut Remote, id: &str) -> Value {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let c = card(r, id);
        if !c["trust"].is_null() {
            return c;
        }
        assert!(Instant::now() < until, "never asked: {c}");
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn claude_moved() -> String {
    screen("claude")
        .replace("❯ No, exit", "  No, exit")
        .replace("  Yes, I trust", "❯ Yes, I trust")
}

/// No conversation yet, and still 等你回答: the words are core's, not the phone's.
#[test]
fn a_trust_prompt_makes_the_card_wait_with_its_question_and_folder() {
    let h = Host::start();
    fake(&h, "c", &screen("claude"), &claude_moved(), 0, false);
    let mut r = Remote::open(&h, None, &[]);
    let c = wait_trust(&mut r, "c");
    assert_eq!(c["phase"], "waiting");
    assert_eq!(c["status"], "waiting");
    assert_eq!(
        c["status_label"],
        fibo_core::status::Status::Waiting.label()
    );
    assert_eq!(c["needs_you"], true);
    assert_eq!(c["agent"], "claude");
    assert!(c["session"].is_null());
    assert_eq!(c["trust"]["folder"], FOLDER);
    assert!(c["trust"]["question"]
        .as_str()
        .unwrap()
        .starts_with("Quick safety check"));
    // Enter here would quit: text does not get through.
    let e = r.error(
        json!({ "op": "send", "terminal": "c", "text": "hi", "enter": true, "request": "s1" }),
    );
    assert!(e.contains("信任"), "{e}");
    // Who may answer, and about which folder.
    let mut lesser = Remote::open(&h, Some("answer"), &[]);
    let e = lesser.error(
        json!({ "op": "trust", "terminal": "c", "folder": FOLDER, "trust": true, "request": "t0" }),
    );
    assert!(e.contains("不能 trust"), "{e}");
    let e = r.error(json!({ "op": "trust", "terminal": "c", "folder": "~/other", "trust": true, "request": "t1" }));
    assert!(e.contains("没有这个信任提示"), "{e}");
    assert!(!h.screen("c").contains("CHOSE"));
}

/// Enter only once the cursor is seen on 「Yes」, even when the CLI swallowed the first arrow.
#[test]
fn trusting_claude_moves_the_cursor_first_and_never_presses_enter_on_no() {
    let h = Host::start();
    fake(&h, "c", &screen("claude"), &claude_moved(), 1, false);
    let mut r = Remote::open(&h, None, &[]);
    wait_trust(&mut r, "c");
    r.ok(
        json!({ "op": "trust", "terminal": "c", "folder": FOLDER, "trust": true, "request": "t" }),
    );
    let s = h.screen("c");
    assert!(s.contains("CHOSE=yes") && !s.contains("CHOSE=no"), "{s}");
}

/// A cursor that never gets there: no Enter at all.
#[test]
fn a_cursor_that_will_not_move_gets_no_enter() {
    let h = Host::start();
    fake(&h, "c", &screen("claude"), &screen("claude"), 99, false);
    let mut r = Remote::open(&h, None, &[]);
    wait_trust(&mut r, "c");
    let e = r.error(
        json!({ "op": "trust", "terminal": "c", "folder": FOLDER, "trust": true, "request": "t" }),
    );
    assert!(e.contains("没有按回车"), "{e}");
    assert!(!h.screen("c").contains("CHOSE"));
}

#[test]
fn not_trusting_is_esc_on_both_and_trusting_codex_is_enter_on_its_first_option() {
    let h = Host::start();
    fake(&h, "x", &screen("codex"), &screen("codex"), 0, true);
    fake(&h, "no", &screen("claude"), &claude_moved(), 0, false);
    let mut r = Remote::open(&h, None, &[]);
    assert_eq!(wait_trust(&mut r, "x")["agent"], "codex");
    wait_trust(&mut r, "no");
    r.ok(
        json!({ "op": "trust", "terminal": "x", "folder": FOLDER, "trust": true, "request": "t1" }),
    );
    assert!(h.screen("x").contains("CHOSE=yes"));
    r.ok(json!({ "op": "trust", "terminal": "no", "folder": FOLDER, "trust": false, "request": "t2" }));
    assert!(h.screen("no").contains("CHOSE=esc"));
}

/// The real answer ends the prompt: the card stops waiting and `trust` replies at once.
#[test]
fn the_card_stops_asking_once_the_prompt_is_gone() {
    let h = Host::start();
    let first = h.root.join("prompt");
    std::fs::write(&first, screen("codex")).unwrap();
    let script = format!(
        "stty -icanon -echo; cat {}; dd bs=1 count=1 2>/dev/null >/dev/null; printf '\\033[2J\\033[Hanswered\\n'; sleep 60",
        shell_quote(&first.to_string_lossy())
    );
    h.launch("x", Path::new("/tmp"), &script);
    let mut r = Remote::open(&h, None, &[]);
    wait_trust(&mut r, "x");
    r.ok(
        json!({ "op": "trust", "terminal": "x", "folder": FOLDER, "trust": true, "request": "t" }),
    );
    let until = Instant::now() + Duration::from_secs(10);
    while !card(&mut r, "x")["trust"].is_null() {
        assert!(Instant::now() < until, "still asking");
        std::thread::sleep(Duration::from_millis(100));
    }
    let c = card(&mut r, "x");
    assert_eq!(c["phase"], "shell");
    assert_eq!(c["needs_you"], false);
}
