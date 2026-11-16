//! What the host says on a terminal about an Agent it sees but cannot read (the sidebar row,
//! the pane header, the state menu's hint) is in the host's language, like its other words.
//! One test only: the language belongs to the whole process.
#[path = "support/tied.rs"]
mod tied;
use fibo_core::{i18n, model::Agent};
use fibo_host::{agent_adapter, recognition::Process, unbound_hint, Pending};
use std::process::{Command, Stdio};
use tied::SpawnTied;

fn cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{3000}'..='\u{303f}' | '\u{ff00}'..='\u{ffef}')
    })
}

/// `[hint, note, why]` of every note an adapter builds: unbound Claude, Codex by its title
/// (working, needing you, idle, no title), a declared CLI without transcripts, and a Claude
/// that saves none (`nested` is a process holding the variable that makes it so).
fn notes(fake: Agent, nested: u32) -> Vec<[String; 3]> {
    let process = |pid, runtime| Process {
        pid,
        parent: 0,
        runtime: Some(runtime),
        cli_session_id: None,
        no_daemon: false,
        launch: vec![],
    };
    let unrecorded =
        |agent: Agent, pid| match agent_adapter(agent).pending(&process(pid, agent), &[]) {
            Pending::Unrecorded(hint) => hint,
            other => panic!("{agent:?}: expected a note, got {other:?}"),
        };
    let codex = |title| agent_adapter(Agent::Codex).title_status(title).unwrap();
    [
        unbound_hint(Agent::Claude),
        codex("⠋ fixing"),
        codex("Action Required"),
        codex("fibo"),
        codex(""),
        unrecorded(fake, 1),
        unrecorded(Agent::Claude, nested),
    ]
    .into_iter()
    .map(|h| [h.text, h.note.note, h.note.why])
    .collect()
}

#[test]
#[ignore = "subprocess standing in for a Claude that inherited the nested-session marker"]
fn holds_the_marker() {
    std::thread::sleep(std::time::Duration::from_secs(60));
}

#[test]
fn terminal_notes_are_in_the_hosts_language() {
    let root = std::env::temp_dir().join(format!("fibo-notes-en-{}", std::process::id()));
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::write(
        root.join("agents/fake.json"),
        r#"{"api":1,"id":"fake","name":"Fake CLI","executable":"fake-cli","start":"fake-cli","resume":"fake-cli --resume {id}"}"#,
    )
    .unwrap();
    assert!(fibo_core::agent::install_from(&root.join("agents"), &root));
    let fake = Agent::from_key("fake");
    // macOS hides a platform binary's environment, so the marker is held by this test
    // program itself, waiting in `holds_the_marker`.
    let mut nested = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "holds_the_marker"])
        .env("CLAUDE_CODE_CHILD_SESSION", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn_tied()
        .unwrap();

    // Chinese, as every host was before it had a language: the words tests and old data hold.
    let zh = notes(fake, nested.id());
    assert_eq!(zh[0][0], "未对上对话：claude");
    assert_eq!(zh[0][1], "claude · 发出第一句后对上对话");
    assert_eq!(zh[1][1], "codex · 工作中 · 未关联对话");
    assert_eq!(zh[5][1], "fake-cli · 没有对话记录");
    assert_eq!(zh[6][1], "claude · 没有保存记录");

    i18n::set(i18n::Lang::En);
    let en = notes(fake, nested.id());
    nested.kill_tree();
    let _ = std::fs::remove_dir_all(&root);
    for (words, was) in en.iter().zip(&zh) {
        for (text, zh) in words.iter().zip(was) {
            assert!(!cjk(text), "still Chinese in an English host: {text}");
            assert!(!text.is_empty(), "no English for: {zh}");
        }
    }
    let note = |i: usize| en[i][1].as_str();
    assert_eq!(en[0][0], "Unmatched session: claude");
    assert_eq!(note(0), "claude · links after your first message");
    assert_eq!(note(1), "codex · Working · unlinked");
    assert_eq!(note(2), "codex · Needs you · unlinked");
    assert_eq!(note(3), "codex · Idle · unlinked");
    assert_eq!(note(4), "codex · unlinked");
    assert_eq!(
        en[1][0],
        "Status only: Working; links to its conversation after your next message"
    );
    assert_eq!(note(5), "fake-cli · no transcript");
    assert!(en[5][2].contains("fake.json"), "{}", en[5][2]);
    assert_eq!(note(6), "claude · no transcript saved");
    // The state itself is still read by its Chinese key, whatever the language shown.
    assert_eq!(
        agent_adapter(Agent::Codex).title_busy("⠋ fixing"),
        Some(true)
    );
    assert_eq!(agent_adapter(Agent::Codex).title_busy("fibo"), Some(false));
}
