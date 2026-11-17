//! What the phone's 新任务, a todo's 开始 and 接着做 tell the host they start: the Agent with
//! its first message exactly as the command line hands it over, or the conversation it picks
//! back up. The host keeps it until the terminal runs a conversation
//! (`Workspace::agent_starts`): a task the TUI redraws (OpenCode's `┃`, Codex's rendered
//! backticks) is tied by it, and a CLI resumed with nothing to say is tied before it speaks.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use fibo_core::{agent::Agent, model::*, protocol::*};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;

/// A CLI that only says it is up: nothing binds it, so what the host keeps for it stays.
fn fake(bin: &Path, name: &str) {
    let path = bin.join(name);
    fs::write(&path, "#!/bin/sh\necho STARTED\nexec sleep 60\n").unwrap();
    Command::new("chmod").arg("+x").arg(&path).status().unwrap();
}

fn started(h: &Host, terminal: &str) -> AgentStart {
    h.workspace()
        .agent_starts
        .get(terminal)
        .cloned()
        .unwrap_or_else(|| panic!("the host kept nothing for {terminal}"))
}

#[test]
fn each_way_the_phone_starts_an_agent_tells_the_host_what_with() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let bin = scratch(&h, "bin");
    fake(&bin, "claude");
    fake(&bin, "codex");
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
    let cwd = work.to_string_lossy();

    // A new task: a leading dash, quotes, backticks and a second line, as typed.
    let task = "--keep \"double\" 'single' and `sh job.sh`\nLine two: reply with only OK";
    let opened = r
        .ok(json!({ "op": "open", "cwd": cwd, "agent": "codex", "prompt": task, "request": "n1" }));
    let terminal = opened["terminal"].as_str().unwrap().to_owned();
    h.wait_screen(&terminal, "STARTED");
    let start = started(&h, &terminal);
    assert_eq!(
        (
            start.agent,
            start.prompt.as_deref(),
            start.resume.as_deref()
        ),
        (Agent::Codex, Some(task), None)
    );
    assert!(start.at > 0, "the host stamps when it started it");

    // Pictures: Claude Code's first message is the words with each picture after them.
    let pic = r.ok(json!({ "op": "image", "data": "iVBORw0KGgo=" }))["path"]
        .as_str()
        .unwrap()
        .to_owned();
    let opened = r.ok(
        json!({ "op": "open", "cwd": cwd, "agent": "claude", "prompt": "看图",
        "images": [pic], "request": "c1" }),
    );
    let terminal = opened["terminal"].as_str().unwrap().to_owned();
    h.wait_screen(&terminal, "STARTED");
    let want = format!("看图\n\n@{pic}");
    assert_eq!(
        started(&h, &terminal).prompt.as_deref(),
        Some(want.as_str())
    );

    // A todo's 开始: its handoff, several lines, is the first message.
    h.call(Request::Edit {
        request: None,
        edit: Edit::AddTask(Task {
            id: "t1".into(),
            space: "s".into(),
            title: "修键盘".into(),
            description: "按键重复，先查驱动".into(),
            acceptance: vec![],
            agent: Agent::Codex,
        }),
    })
    .unwrap();
    let opened = r.ok(json!({ "op": "open", "task": "t1", "cwd": cwd, "request": "t1" }));
    let terminal = opened["terminal"].as_str().unwrap().to_owned();
    h.wait_screen(&terminal, "STARTED");
    let w = h.workspace();
    let handoff = w.task_handoff(
        w.tasks.iter().find(|t| t.id == "t1").unwrap(),
        "fibo todo note",
    );
    assert!(handoff.contains('\n'));
    assert_eq!(
        started(&h, &terminal).prompt.as_deref(),
        Some(handoff.as_str())
    );

    // 接着做: the conversation it resumes, in its CLI's own id.
    h.codex("old", &work, &[], "sleep 60");
    let old = h.workspace().current_agent("old").unwrap().clone();
    h.call(Request::Stop { id: "old".into() }).unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.workspace().current_agent("old").is_some() {
        assert!(
            std::time::Instant::now() < until,
            "the old pane never ended"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let opened = r.ok(json!({ "op": "open", "resume": old.id, "request": "r1" }));
    let terminal = opened["terminal"].as_str().unwrap().to_owned();
    h.wait_screen(&terminal, "STARTED");
    let start = started(&h, &terminal);
    assert_eq!(
        (start.agent, start.prompt, start.resume),
        (Agent::Codex, None, Some(old.cli_session_id))
    );

    // A plain shell starts no Agent: nothing kept.
    let opened = r.ok(json!({ "op": "open", "cwd": cwd, "request": "s1" }));
    let terminal = opened["terminal"].as_str().unwrap();
    assert!(!h.workspace().agent_starts.contains_key(terminal));
}
