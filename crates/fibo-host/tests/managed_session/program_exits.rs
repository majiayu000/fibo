use super::*;
use fibo_platform::shell;

/// The command a terminal was opened to run, an Agent mostly, exits and its shell takes over:
/// how it ended stays on disk with the bottom of its screen, whether it finished, failed or
/// was killed, and none of it counts as one of the shell's commands (the pane's last command).
/// (2026-11-19: a resumed Codex turned into a bare shell and nothing said why.)
#[test]
fn how_the_launched_command_ended_is_kept_on_disk() {
    let h = Host::start();
    h.setup();
    let pid_file = h.root.join("killed.pid");
    let cases = [
        // A full-screen CLI's modes, still on when it is gone: mouse moves would be typed text.
        (
            "clean",
            r"(printf '\033[?1003h\033[?1006h\033[>1u'; echo 'bye for now'; exit 0)".to_owned(),
        ),
        (
            "failed",
            "printf 'starting\\n'; (echo 'Error: resume failed'; exit 3)".to_owned(),
        ),
        (
            "killed",
            format!(
                "sh -c 'echo waiting; echo $$ > {}; exec sleep 60'",
                pid_file.display()
            ),
        ),
    ];
    for (id, command) in &cases {
        let argv = shell::run_then_stay("/bin/sh", command, None);
        let mut launch = h.launch(id, "");
        if let Request::Launch { argv: a, .. } = &mut launch {
            *a = argv;
        }
        let reply = h.call(launch).unwrap();
        assert!(!matches!(reply, Reply::Error(_)), "{reply:?}");
    }
    let pid = wait_file(&pid_file);
    let killed = Command::new("kill")
        .args(["-TERM", pid.trim()])
        .status()
        .unwrap();
    assert!(killed.success());
    let log = h.root.join("program-exits.jsonl");
    let until = Instant::now() + Duration::from_secs(20);
    let entries = loop {
        let entries: Vec<serde_json::Value> = fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        if entries.len() >= cases.len() {
            break entries;
        }
        assert!(Instant::now() < until, "only {entries:?}");
        std::thread::sleep(Duration::from_millis(50));
    };
    let of = |id: &str| {
        let found: Vec<_> = entries.iter().filter(|e| e["terminal"] == id).collect();
        assert_eq!(found.len(), 1, "{id}: {entries:?}");
        found[0].clone()
    };
    let clean = of("clean");
    assert_eq!(clean["exit"], 0);
    assert!(clean["signal"].is_null());
    assert!(
        clean["screen"].to_string().contains("bye for now"),
        "{clean}"
    );
    assert!(clean["command"]
        .as_str()
        .unwrap()
        .ends_with("echo 'bye for now'; exit 0)"));
    let (_, vt, _) = h.attach("clean", false);
    assert!(
        !vt.terminal().is_mouse_tracking().unwrap(),
        "mouse reports left on"
    );
    assert!(
        vt.terminal().kitty_keyboard_flags().unwrap().is_empty(),
        "kitty keys left on"
    );
    let failed = of("failed");
    assert_eq!(failed["exit"], 3);
    assert_eq!(
        failed["command"],
        "printf 'starting\\n'; (echo 'Error: resume failed'; exit 3)"
    );
    assert!(
        failed["screen"]
            .to_string()
            .contains("Error: resume failed"),
        "{failed}"
    );
    let killed = of("killed");
    assert_eq!(killed["exit"], 143);
    assert_eq!(killed["signal"], "TERM");
    assert!(
        killed["at"].is_u64() || killed["at"].is_string(),
        "{killed}"
    );
    let commands = recognition(&h).terminal_commands;
    for (id, _) in &cases {
        assert!(!commands.contains_key(*id), "{id}: {commands:?}");
    }
    // The shells stayed, ready for someone to type in.
    let w = h.workspace();
    for (id, _) in &cases {
        let t = w.terminals.iter().find(|t| t.id == *id).unwrap();
        assert_eq!(t.state, ProcessState::Running, "{id}");
    }
}
