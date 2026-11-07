//! `claude-quota`'s status line wrapper, run for real: each conversation keeps its own copy, so
//! each pane can say how full its own context is, and the status line set before still prints.
#![cfg(target_os = "macos")]
use mooring_core::context_fill::from_claude_status;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn run(data: &Path, payload: &str) -> String {
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/claude-quota");
    let mut child = Command::new("/bin/sh")
        .arg(plugin.join("statusline.sh"))
        .arg(data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
}
fn payload(id: &str, read: u64) -> String {
    serde_json::json!({"session_id": id, "context_window": {"context_window_size": 1_000_000,
        "current_usage": {"input_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": read}}})
    .to_string()
}

#[test]
fn each_conversation_keeps_its_own_reading_and_the_old_status_line_still_prints() {
    let data = std::env::temp_dir().join(format!("mooring-contexts-{}", std::process::id()));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("original.sh"), "cat >/dev/null; echo mine\n").unwrap();
    assert_eq!(run(&data, &payload("aaa-1", 600_000)), "mine\n");
    assert_eq!(run(&data, &payload("bbb-2", 900_000)), "mine\n");
    let percent = |id: &str| {
        let doc = std::fs::read(data.join(format!("contexts/{id}.json"))).unwrap();
        from_claude_status(&serde_json::from_slice(&doc).unwrap())
            .unwrap()
            .percent()
    };
    assert_eq!((percent("aaa-1"), percent("bbb-2")), (60, 90));
    assert_eq!(run(&data, &payload("aaa-1", 700_000)), "mine\n");
    assert_eq!(percent("aaa-1"), 70);
    // Nothing but the readings is left, and an id that could leave the folder writes nothing.
    assert_eq!(std::fs::read_dir(data.join("contexts")).unwrap().count(), 2);
    assert_eq!(run(&data, &payload("../escape", 1)), "mine\n");
    assert!(!data.join("escape.json").exists());
    assert_eq!(std::fs::read_dir(data.join("contexts")).unwrap().count(), 2);
    std::fs::remove_dir_all(&data).unwrap();
}
