//! Codex's 「Update available」 picker at start, through `fibo remote --stdio`: the card
//! waits while it is on the screen and stops as soon as it is gone, even when what follows
//! never lets the screen go quiet. The screen is the real one (fixtures/screens/codex).
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use fibo_core::model::shell_quote;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

fn needs_you(r: &mut Remote, id: &str) -> bool {
    let list = r.ok(json!({ "op": "list" }));
    let cards = list["terminals"].as_array().unwrap();
    let c: &Value = cards.iter().find(|c| c["id"] == id).unwrap();
    c["needs_you"] == true
}

/// v0.1.4 RC1: after Esc, Codex's working line ticks every second, the screen is never
/// quiet, and the card kept 「要不要更新 Codex」 for the whole turn (16-37 s, 4 of 4).
#[test]
fn skipping_the_update_picker_into_a_running_turn_stops_the_wait_at_once() {
    let h = Host::start();
    let page =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens/codex/update-real.txt");
    // Esc, then a turn that redraws its working line every 0.2 s, like Codex's timer.
    let script = format!(
        "stty -icanon -echo -isig; cat {}; dd bs=1 count=1 2>/dev/null >/dev/null; n=0; while kill -0 $PPID 2>/dev/null; do n=$((n+1)); printf '\\033[2J\\033[H• Working (%ss • esc to interrupt)\\n\\n› Ask Codex to do anything\\n' $n; sleep 0.2; done",
        shell_quote(&page.to_string_lossy())
    );
    h.launch("x", Path::new("/tmp"), &script);
    let mut r = Remote::open(&h, None, &[]);
    let until = Instant::now() + Duration::from_secs(30);
    while !needs_you(&mut r, "x") {
        assert!(Instant::now() < until, "never asked");
        std::thread::sleep(Duration::from_millis(100));
    }
    r.ok(json!({ "op": "keys", "terminal": "x", "keys": ["esc"], "request": "k" }));
    h.wait_screen("x", "Working");
    let gone = Instant::now() + Duration::from_secs(3);
    while needs_you(&mut r, "x") {
        assert!(
            Instant::now() < gone,
            "still asks to update: {}",
            h.screen("x")
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
