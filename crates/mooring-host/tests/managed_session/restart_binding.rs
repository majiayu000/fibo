use super::*;
use std::os::unix::fs::PermissionsExt;

/// A plain Claude (no id in argv, log closed) bound by pid, as it is after its first turn.
struct Plain {
    h: Host,
    id: String,
    pid: u32,
}
fn plain_claude() -> Plain {
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let path = logs.join(format!("{id}.jsonl"));
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let seed = h.root.join("seed");
    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u1","cwd":"/project","entrypoint":"cli","userType":"external","message":{"role":"user","content":"hi"},"timestamp":"2026-01-01T00:00:00.000Z"});
    fs::write(&seed, format!("{row}\n")).unwrap();
    let pid_file = h.root.join("claude-pid");
    list_as(&h, &id, &pid_file);
    let script = format!(
        "echo $$ > {}; cat {} > {}; read -r line",
        shell_quote(&pid_file.to_string_lossy()),
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&path.to_string_lossy())
    );
    h.call(h.launch(
        "plain",
        &format!(
            "exec {} -c {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    wait_for(&h, "plain claude named", |w| {
        w.current_agent("plain")
            .is_some_and(|s| s.cli_session_id == id)
    });
    let pid = wait_file(&pid_file).trim().parse().unwrap();
    Plain { h, id, pid }
}
/// `claude agents --json` lists the pane's process under this conversation.
fn list_as(h: &Host, id: &str, pid_file: &std::path::Path) {
    let agents = h.root.join("claude-agents");
    fs::write(
        &agents,
        format!(
            "#!/bin/sh\nprintf '[{{\"pid\":%s,\"kind\":\"interactive\",\"sessionId\":\"{id}\",\"status\":\"idle\"}}]' \"$(cat {})\"\n",
            shell_quote(&pid_file.to_string_lossy())
        ),
    )
    .unwrap();
    fs::set_permissions(&agents, fs::Permissions::from_mode(0o755)).unwrap();
}
/// A newer host takes the terminals over; only the store and the processes carry across.
fn take_over(h: &mut Host) -> tied::Tied {
    let next = fixture_command(&h.root)
        .env("MOORING_FIXTURE_TAKEOVER", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn_tied()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while h.child.try_wait().unwrap().is_none() || h.call(Request::Workspace).is_err() {
        assert!(Instant::now() < until, "the takeover never finished");
        std::thread::sleep(Duration::from_millis(50));
    }
    next
}
/// The host knows the process when it can stop it; it refuses when it only has the session.
fn stop(h: &Host) -> io::Result<Reply> {
    h.call(Request::StopAgent { id: "plain".into() })
}

/// The Agent is waiting on a background shell, so no hook comes to tell the new host which
/// process it is. Without the pid its shells are never looked at: a running job falls to
/// 等后台 once the restart has settled. Undoing the naming of bound panes fails this.
#[test]
fn a_new_host_finds_the_process_of_a_quiet_plain_claude() {
    let mut p = plain_claude();
    let _next = take_over(&mut p.h);
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if stop(&p.h).is_ok() {
            break;
        }
        assert!(Instant::now() < until, "the new host never found {}", p.pid);
        std::thread::sleep(Duration::from_millis(100));
    }
    // The stop just ended the process, so the session may no longer be current by now.
    let w = p.h.workspace();
    let bound: Vec<_> = w
        .agent_sessions
        .iter()
        .filter(|s| s.terminal.as_deref() == Some("plain"))
        .map(|s| s.cli_session_id.as_str())
        .collect();
    assert_eq!(bound, [p.id.as_str()]);
}

/// What the list says only finds the process of the pane's conversation. A pane whose
/// process is listed under another conversation (a nested `claude -p`, a log it handed on)
/// stays bound as it was, and its process stays unknown.
#[test]
fn a_listed_process_under_another_conversation_does_not_move_a_bound_pane() {
    let mut p = plain_claude();
    let other = uuid::Uuid::new_v4().to_string();
    list_as(&p.h, &other, &p.h.root.join("claude-pid"));
    let _next = take_over(&mut p.h);
    for _ in 0..3 {
        p.h.call(Request::RefreshAgents).unwrap();
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        stop(&p.h).is_err(),
        "must not stop a process it did not confirm"
    );
    assert_eq!(
        p.h.workspace()
            .current_agent("plain")
            .unwrap()
            .cli_session_id,
        p.id
    );
    assert_eq!(unsafe { libc::kill(p.pid as i32, 0) }, 0);
}
