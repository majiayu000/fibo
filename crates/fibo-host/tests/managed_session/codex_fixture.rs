use super::*;

// Shared-daemon Codex stand-in: the TUI paints prompts (and a title) on its own PTY while a
// separate process keeps each thread's log open for appends, as the real daemon does. No argv
// ID and no open log in the Codex process itself.
pub(crate) struct DaemonStep {
    pub(crate) gate: Option<PathBuf>,
    pub(crate) prompt: String,
    pub(crate) thread: String,
    /// When the log says the prompt arrived; now when unset.
    pub(crate) at: Option<u64>,
}
pub(crate) fn daemon_codex(h: &Host, pane: &str, title: &str, steps: &[DaemonStep], extra: &str) {
    let logs = h.root.join("logs/codex");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("codex");
    if !cli.exists() {
        std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    }
    let q = |p: &std::path::Path| shell_quote(&p.to_string_lossy());
    let mut script = format!("printf '\\033]0;%s\\007' {}; ", shell_quote(title));
    let mut threads: Vec<&str> = vec![];
    for step in steps {
        if !threads.contains(&step.thread.as_str()) {
            threads.push(&step.thread);
        }
    }
    // Like the real daemon: each thread's log is created at TUI start with only its header,
    // held open by a separate non-Agent process, and prompts are appended later.
    for (j, thread) in threads.iter().enumerate() {
        let fifo = h.root.join(format!("{pane}-fifo-{j}"));
        assert!(std::process::Command::new("/usr/bin/mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success());
        let meta = h.root.join(format!("{pane}-meta-{j}"));
        fs::write(
            &meta,
            format!(
                "{}\n",
                serde_json::json!({"type":"session_meta","payload":{"id":thread,"cwd":"/tmp"}})
            ),
        )
        .unwrap();
        script.push_str(&format!(
            "/bin/cat < {} >> {} & exec {}> {}; cat {} >&{}; ",
            q(&fifo),
            q(&logs.join(format!("{thread}.jsonl"))),
            3 + j,
            q(&fifo),
            q(&meta),
            3 + j
        ));
    }
    script.push_str("sleep 1.5; ");
    for (i, step) in steps.iter().enumerate() {
        let at = step.at.unwrap_or_else(fibo_core::content::now);
        let seed_path = h.root.join(format!("{pane}-seed-{i}"));
        let prompt_path = h.root.join(format!("{pane}-prompt-{i}"));
        fs::write(
            &seed_path,
            format!(
                "{}\n{}\n",
                serde_json::json!({"timestamp":at,"type":"event_msg","payload":{"type":"task_started","turn_id":format!("{pane}-{i}")}}),
                serde_json::json!({"timestamp":at,"type":"event_msg","payload":{"type":"user_message","message":step.prompt}})
            ),
        )
        .unwrap();
        fs::write(&prompt_path, &step.prompt).unwrap();
        if let Some(gate) = &step.gate {
            script.push_str(&format!(
                "while [ ! -e {} ]; do sleep 0.02; done; ",
                q(gate)
            ));
        }
        let fd = 3 + threads.iter().position(|t| *t == step.thread).unwrap();
        script.push_str(&format!(
            "printf '› '; cat {}; printf '\\n'; cat {} >&{fd}; ",
            q(&prompt_path),
            q(&seed_path)
        ));
    }
    // A typed `title:<text>` line sets the title and `say:<text>` prints a line, as the TUI
    // does when a turn starts or ends; `clear` wipes the screen.
    script.push_str(
        "while read -r line; do case \"$line\" in title:*) printf '\\033]0;%s\\007' \"${line#title:}\";; say:*) printf '%s\\n' \"${line#say:}\";; clear) printf '\\033[2J\\033[H';; esac; done",
    );
    h.call(h.launch(
        pane,
        // After the script, where every shell takes it for an argument: dash refuses an
        // option it does not know.
        &format!("exec {} -c {} {}", q(&cli), shell_quote(&script), extra),
    ))
    .unwrap();
}
pub(crate) fn step(prompt: &str, thread: &str, gate: Option<&PathBuf>) -> DaemonStep {
    DaemonStep {
        gate: gate.cloned(),
        prompt: prompt.into(),
        thread: thread.into(),
        at: None,
    }
}
/// A fake Codex control socket: answers `thread/read` with `status` and records each
/// method it was asked, so the test can prove nothing subscribed.
pub(crate) fn fake_codex_control(
    path: &std::path::Path,
    status: serde_json::Value,
) -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    use std::io::{Read, Write};
    let listener = std::os::unix::net::UnixListener::bind(path).unwrap();
    let methods = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = methods.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { return };
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                let mut b = [0; 1];
                if s.read_exact(&mut b).is_err() {
                    break;
                }
                head.push(b[0]);
            }
            let _ = s.write_all(b"HTTP/1.1 101 Switching Protocols\r\n\r\n");
            loop {
                let mut h = [0; 2];
                if s.read_exact(&mut h).is_err() {
                    break;
                }
                let mut len = (h[1] & 0x7f) as usize;
                if len == 126 {
                    let mut n = [0; 2];
                    s.read_exact(&mut n).unwrap();
                    len = u16::from_be_bytes(n) as usize;
                }
                let mut mask = [0; 4];
                s.read_exact(&mut mask).unwrap();
                let mut d = vec![0; len];
                s.read_exact(&mut d).unwrap();
                d.iter_mut()
                    .enumerate()
                    .for_each(|(i, b)| *b ^= mask[i % 4]);
                if h[0] & 0x0f != 1 {
                    break;
                }
                let m: serde_json::Value = serde_json::from_slice(&d).unwrap();
                seen.lock()
                    .unwrap()
                    .push(m["method"].as_str().unwrap_or("").to_owned());
                let reply = match m["method"].as_str() {
                    Some("initialize") => serde_json::json!({"id": m["id"], "result": {}}),
                    Some("thread/read") => serde_json::json!({"id": m["id"], "result": {"thread": {
                        "id": m["params"]["threadId"], "status": status}}}),
                    _ => continue,
                };
                let body = reply.to_string();
                let mut f = vec![0x81];
                if body.len() < 126 {
                    f.push(body.len() as u8);
                } else {
                    f.push(126);
                    f.extend_from_slice(&(body.len() as u16).to_be_bytes());
                }
                f.extend_from_slice(body.as_bytes());
                if s.write_all(&f).is_err() {
                    break;
                }
            }
        }
    });
    methods
}
