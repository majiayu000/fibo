//! Pictures picked on the phone's new task page: kept with `image`, then named in `open`.
//! Claude Code gets them as `@path` after the words, Codex as `--image=path`; a path that
//! `image` did not keep opens nothing.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use fibo_core::model::shell_quote;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const PNG: &str = "iVBORw0KGgo=";

/// A CLI that writes each argument on its own line, then says it is up.
fn fake(bin: &Path, name: &str, said: &Path) {
    let path = bin.join(name);
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\necho STARTED\nexec sleep 60\n",
        shell_quote(&said.to_string_lossy())
    );
    fs::write(&path, script).unwrap();
    Command::new("chmod").arg("+x").arg(&path).status().unwrap();
}

fn read(said: &Path) -> String {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let text = fs::read_to_string(said).unwrap_or_default();
        if !text.is_empty() || Instant::now() > end {
            return text;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_new_task_hands_its_pictures_to_the_agent_as_its_cli_takes_them() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let bin = scratch(&h, "bin");
    let (claude_said, codex_said) = (h.root.join("claude.txt"), h.root.join("codex.txt"));
    fake(&bin, "claude", &claude_said);
    fake(&bin, "codex", &codex_said);
    let shell = bin.join("fake-shell");
    fs::write(
        &shell,
        format!(
            "#!/bin/sh\nwhile [ \"$1\" = -l ] || [ \"$1\" = -i ]; do shift; done\nPATH={}:$PATH exec /bin/sh \"$@\"\n",
            shell_quote(&bin.to_string_lossy())
        ),
    )
    .unwrap();
    Command::new("chmod")
        .arg("+x")
        .arg(&shell)
        .status()
        .unwrap();
    let mut r = Remote::open(&h, None, &[("SHELL", shell.as_path())]);
    assert!(
        r.ops().iter().any(|o| o == "openimages"),
        "the phone asks before offering it"
    );
    let pic = |r: &mut Remote| {
        r.ok(json!({ "op": "image", "data": PNG }))["path"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let (a, b) = (pic(&mut r), pic(&mut r));
    let cwd = work.to_string_lossy();

    let opened = r.ok(json!({ "op": "open", "cwd": cwd, "agent": "claude",
        "prompt": "看这两张", "images": [a, b], "request": "c1" }));
    h.wait_screen(opened["terminal"].as_str().unwrap(), "STARTED");
    assert_eq!(read(&claude_said), format!("看这两张\n\n@{a} @{b}\n"));

    let opened = r.ok(json!({ "op": "open", "cwd": cwd, "agent": "codex",
        "prompt": "看图", "images": [a], "request": "x1" }));
    h.wait_screen(opened["terminal"].as_str().unwrap(), "STARTED");
    assert_eq!(read(&codex_said), format!("--image={a}\n看图\n"));

    let before = h.workspace().terminals.len();
    let refused = r.error(json!({ "op": "open", "cwd": cwd, "agent": "claude",
        "prompt": "x", "images": ["/etc/hosts"], "request": "bad" }));
    assert!(refused.contains("images"), "{refused}");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(h.workspace().terminals.len(), before, "nothing opened");
}
