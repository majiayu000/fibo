//! `program-exits.jsonl` in the runtime: each time the command a terminal was opened to run
//! (an Agent, mostly) exited and its shell took over, with the exit status and the bottom of
//! the screen. Without it a pane that turned into a bare shell says nothing about why: the
//! Agent's own log may have nothing, and its last words scroll away (2026-11-19, a Codex
//! resumed on 11-16 was found as a shell two days later, with no trace of who ended it).
//! Like `screen-waits.jsonl`: this machine only, readable only by its user.
use fibo_core::model::AgentSession;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Enough for an error and the lines before it, or a CLI's goodbye.
const LINES: usize = 20;
const LINE_CHARS: usize = 200;
const COMMAND_CHARS: usize = 300;

static FILE: OnceLock<PathBuf> = OnceLock::new();

pub(super) fn start(root: &Path) {
    let _ = FILE.set(root.join("program-exits.jsonl"));
}

pub(super) fn exited(
    terminal: &str,
    command: Option<&str>,
    session: Option<&AgentSession>,
    exit: Option<i32>,
    tail: &str,
) {
    if let Some(path) = FILE.get() {
        crate::devices::log(path, entry(terminal, command, session, exit, tail));
    }
}

impl super::Host {
    /// Records that the command `terminal` was opened to run exited. Called holding the
    /// host, so the terminal's lock comes second.
    pub(super) fn launch_exited(&self, terminal: &str, exit: Option<i32>) {
        let placed = self.workspace.terminals.iter().find(|t| t.id == terminal);
        // The line the shell ran, without the shell around it or the text handed to it.
        let command = placed.and_then(|t| t.argv.get(4)).map(|line| {
            line.rsplit_once(r"; printf '\033]133;D;")
                .map_or(line.as_str(), |(command, _)| command)
                .trim_end_matches(" \"$1\"")
        });
        let tail = match self.terminals.get(terminal).map(|t| super::lock(t)) {
            Some(Ok(live)) => live.vt.tail_text(LINES * 3).unwrap_or_default(),
            _ => String::new(),
        };
        let session = self.workspace.current_agent(terminal);
        exited(terminal, command, session, exit, &tail);
    }
}

fn entry(
    terminal: &str,
    command: Option<&str>,
    session: Option<&AgentSession>,
    exit: Option<i32>,
    tail: &str,
) -> Value {
    json!({
        "event": "exited",
        "terminal": terminal,
        "command": command.map(|c| c.chars().take(COMMAND_CHARS).collect::<String>()),
        "agent": session.map(|s| s.runtime.key()),
        "session": session.map(|s| s.cli_session_id.as_str()),
        "exit": exit,
        "signal": exit.and_then(signal),
        "screen": bottom(tail),
    })
}

/// A shell reports a program a signal ended as 128 + the signal's number.
fn signal(exit: i32) -> Option<&'static str> {
    Some(match exit - 128 {
        1 => "HUP",
        2 => "INT",
        3 => "QUIT",
        6 => "ABRT",
        9 => "KILL",
        11 => "SEGV",
        13 => "PIPE",
        15 => "TERM",
        _ => return None,
    })
}

fn bottom(tail: &str) -> Vec<String> {
    let mut lines: Vec<&str> = tail
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(LINES)
        .collect();
    lines.reverse();
    lines
        .iter()
        .map(|l| l.chars().take(LINE_CHARS).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_names_its_signal_and_only_the_bottom_of_the_screen_is_kept() {
        let tail = format!("{}Error: resume failed\n\n", "old\n".repeat(50));
        let e = entry("t", Some("codex resume x"), None, Some(143), &tail);
        assert_eq!(e["exit"], 143);
        assert_eq!(e["signal"], "TERM");
        assert_eq!(e["command"], "codex resume x");
        let screen = e["screen"].as_array().unwrap();
        assert_eq!(screen.len(), LINES);
        assert_eq!(screen.last().unwrap(), "Error: resume failed");
        let clean = entry("t", None, None, Some(0), "");
        assert!(clean["signal"].is_null());
        assert!(entry("t", None, None, Some(1), "")["signal"].is_null());
    }
}
