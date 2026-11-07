use super::*;
use mooring_core::status::{self, Status};

/// A Codex whose turn ended, its footer saying a command was left running, and that command
/// as its child with stdin closed. Its log names the command, as a real rollout does.
fn codex_left_running(h: &Host, pane: &str, command: &str) {
    h.setup();
    let logs = h.root.join("logs/codex");
    fs::create_dir_all(&logs).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let log = logs.join(format!("{id}.jsonl"));
    let rows = [
        serde_json::json!({"type":"session_meta","payload":{"id":id,"cwd":"/tmp"}}),
        serde_json::json!({"type":"event_msg","timestamp":"2026-11-06T10:00:00Z","payload":{"type":"task_started","turn_id":"t1"}}),
        serde_json::json!({"type":"event_msg","timestamp":"2026-11-06T10:00:01Z","payload":{"type":"user_message","message":"start it"}}),
        serde_json::json!({"type":"response_item","timestamp":"2026-11-06T10:00:02Z","payload":{"type":"custom_tool_call","call_id":"c1","name":"exec","input":format!("text(await tools.exec_command({{cmd:{}}}));",serde_json::json!(command))}}),
        serde_json::json!({"type":"event_msg","timestamp":"2026-11-06T10:00:03Z","payload":{"type":"task_complete","last_agent_message":"started"}}),
    ];
    fs::write(&log, rows.map(|r| format!("{r}\n")).concat()).unwrap();
    let cli = h.root.join("codex");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let pid_file = h.root.join("codex-pid");
    // The footer comes after the binding, as on a real screen a turn is well under way.
    let bound = h.root.join("codex-bound");
    let script = format!(
        "echo $$ > {}; /bin/sh -c {} </dev/null & while [ ! -e {} ]; do /bin/sleep 0.1; done; printf '  1 background terminal running \\302\\267 /ps to view \\302\\267 /stop to close\\n'; read -r line",
        shell_quote(&pid_file.to_string_lossy()),
        shell_quote(command),
        shell_quote(&bound.to_string_lossy()),
    );
    h.call(h.launch(
        pane,
        &format!(
            "exec {} -c {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script)
        ),
    ))
    .unwrap();
    let pid: u32 = wait_file(&pid_file).trim().parse().unwrap();
    h.call(Request::Hook {
        runtime: Agent::Codex,
        pid,
        cli_session_id: id,
        transcript: log.to_string_lossy().into_owned(),
        ended: false,
        event: None,
        facts: Default::default(),
        terminal: Some(pane.into()),
        cwd: "/tmp".into(),
    })
    .unwrap();
    fs::write(&bound, "").unwrap();
}
fn shows(h: &Host, pane: &str) -> Option<Status> {
    let w = h.workspace();
    let s = w.current_agent(pane)?;
    Some(status::of(&w, s, mooring_core::content::now()))
}
fn wait_shows(h: &Host, pane: &str, want: Status) {
    let until = Instant::now() + Duration::from_secs(40);
    while shows(h, pane) != Some(want) {
        if Instant::now() >= until {
            let w = h.workspace();
            let c = w.current_agent(pane).and_then(|s| w.content.get(&s.id));
            panic!(
                "still {:?}, wanted {want:?}: {:?} {:?} {:?} {}",
                shows(h, pane),
                c.map(|c| c.turn),
                c.and_then(|c| c.background),
                c.map(|c| c.shells.clone()),
                c.map_or(String::new(), |c| c.signal_detail.clone()),
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
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

/// Nothing tells a new host about a command Codex left running: its report is gone with the
/// old host, and a quiet screen stays quiet. Undoing `read_screens` fails the last wait.
#[test]
fn a_codex_command_left_running_stays_running_across_a_new_host() {
    let mut h = Host::start();
    codex_left_running(&h, "job", "sleep 300; echo codex-job-7c1e");
    wait_shows(&h, "job", Status::Running);
    let _next = take_over(&mut h);
    wait_shows(&h, "job", Status::Running);
    let after = Instant::now() + Duration::from_secs(8);
    while Instant::now() < after {
        assert_eq!(shows(&h, "job"), Some(Status::Running));
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// A server Codex left running is not its turn going on. Undoing `keep_ended_turns` or `found_tasks` leaves it
/// 在跑 for as long as it stays up.
#[test]
fn a_server_codex_left_running_is_waiting_on_the_background() {
    let h = Host::start();
    // Below where any system starts handing out ports.
    let port = 22000 + (std::process::id() % 900) as u16;
    let server = format!("python3 -m http.server {port} --bind 127.0.0.1");
    codex_left_running(&h, "srv", &server);
    wait_shows(&h, "srv", Status::Background);
}
