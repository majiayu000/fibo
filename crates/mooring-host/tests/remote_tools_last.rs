//! `toolcalls` with `last`: what a running turn did just now, for the phone's live block,
//! and `sendnow`, which only a phone in full control may send.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn user(cwd: &std::path::Path, u: &str, text: &str) -> Value {
    json!({"type":"user","uuid":u,"cwd":cwd,"entrypoint":"cli","userType":"external","message":{"content":text}})
}
fn call(u: &str, id: &str, command: &str) -> Value {
    json!({"type":"assistant","uuid":u,"message":{"content":[{"type":"tool_use","id":id,"name":"Bash","input":{"command":command}}]}})
}
fn result(u: &str, id: &str) -> Value {
    json!({"type":"user","uuid":u,"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}})
}

#[test]
fn last_gives_a_running_turns_latest_calls_oldest_first() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let mut rows = vec![user(&work, "u1", "go")];
    for i in 1..=5 {
        rows.push(call(
            &format!("a{i}"),
            &format!("t{i}"),
            &format!("cargo test step{i}"),
        ));
        if i < 5 {
            rows.push(result(&format!("r{i}"), &format!("t{i}")));
        }
    }
    h.claude("c", &work, &rows, "sleep 60");
    let mut r = Remote::open(&h, Some("view"), &[]);
    assert!(r.ops().iter().any(|o| o == "toolslast"));
    assert!(
        !r.ops().iter().any(|o| o == "sendnow"),
        "sendnow types into the terminal"
    );

    let until = Instant::now() + Duration::from_secs(30);
    let list = loop {
        let list = r.ok(json!({ "op": "toolcalls", "terminal": "c", "last": 2 }));
        if list["tools"]["calls"] == 5 {
            break list;
        }
        assert!(Instant::now() < until, "never indexed: {list}");
        std::thread::sleep(Duration::from_millis(100));
    };
    let ids: Vec<&str> = list["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["t4", "t5"], "the last two, oldest first: {list}");
    assert_eq!(list["calls"][1]["open"], true, "t5 still runs");
    assert!(list.get("next").is_none(), "nothing to page: {list}");

    let full = Remote::open(&h, Some("full"), &[]);
    assert!(full.ops().iter().any(|o| o == "sendnow"));
}
