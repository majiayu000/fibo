//! `claude agents --json`: Claude Code's documented machine-readable session state.
//! Run once per trigger, never on a timer; the binary is the session's own executable, so
//! no PATH from a login shell is needed and the version always matches the session.
use crate::content::Official;
use serde_json::Value;
use std::{
    io,
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub session_id: String,
    pub pid: Option<u32>,
    pub status: Official,
}

/// The program to ask `agents --json`, from a running Claude's executable. One installed
/// with npm runs as `node` (or bun) with the script as an argument: asking node for its
/// agents fails, so it is asked through `claude` on the PATH instead.
pub fn claude_program(pid: u32) -> io::Result<String> {
    let exe = executable(pid)?;
    Ok(program_for(&exe))
}
fn program_for(exe: &str) -> String {
    let name = std::path::Path::new(exe)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if matches!(name, "node" | "bun" | "deno") || name.starts_with("node") {
        "claude".into()
    } else {
        exe.into()
    }
}
pub use mooring_platform::inspect::executable;

pub fn query(program: &str) -> io::Result<Vec<Entry>> {
    let child = Command::new(program)
        .args(["agents", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = match rx.recv_timeout(TIMEOUT) {
        Ok(output) => output?,
        Err(_) => {
            mooring_platform::process::kill(pid as i32);
            return Err(io::Error::other("claude agents --json timed out"));
        }
    };
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "claude agents --json exited with {}",
            output.status
        )));
    }
    parse(&output.stdout)
}

/// Entries this module can map; unknown shapes are skipped, not guessed.
pub fn parse(bytes: &[u8]) -> io::Result<Vec<Entry>> {
    let rows: Vec<Value> = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let status = match (row["status"].as_str(), row["state"].as_str()) {
                (Some("waiting"), _) | (_, Some("blocked")) => {
                    Official::Waiting(reason(row["waitingFor"].as_str().unwrap_or("")))
                }
                (Some("busy"), _) | (_, Some("working")) => Official::Busy,
                (Some("idle"), _) | (_, Some("done" | "failed" | "stopped")) => Official::Idle,
                _ => return None,
            };
            Some(Entry {
                session_id: row["sessionId"].as_str()?.to_owned(),
                pid: row["pid"].as_u64().and_then(|p| u32::try_from(p).ok()),
                status,
            })
        })
        .collect())
}

fn reason(waiting_for: &str) -> String {
    match waiting_for {
        "permission prompt" => "等待权限审批".into(),
        "input needed" => "等待你的输入".into(),
        "sandbox request" => "等待沙箱授权".into(),
        "worker request" => "后台任务在等你回应".into(),
        "dialog open" => "有对话框打开".into(),
        "" => "等待你处理".into(),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_documented_states_and_skips_unknown_rows() {
        let json = br#"[
            {"pid":1,"cwd":"/a","kind":"interactive","startedAt":1,"sessionId":"a","status":"waiting","waitingFor":"input needed"},
            {"pid":2,"cwd":"/b","kind":"interactive","startedAt":1,"sessionId":"b","status":"busy"},
            {"pid":3,"cwd":"/c","kind":"interactive","startedAt":1,"sessionId":"c","status":"idle"},
            {"id":"d","cwd":"/d","kind":"background","startedAt":1,"sessionId":"d","state":"blocked"},
            {"cwd":"/e","kind":"interactive","startedAt":1,"status":"idle"},
            {"cwd":"/f","kind":"interactive","startedAt":1,"sessionId":"f","status":"sleeping"}
        ]"#;
        let entries = parse(json).unwrap();
        assert_eq!(
            entries,
            vec![
                Entry {
                    session_id: "a".into(),
                    pid: Some(1),
                    status: Official::Waiting("等待你的输入".into())
                },
                Entry {
                    session_id: "b".into(),
                    pid: Some(2),
                    status: Official::Busy
                },
                Entry {
                    session_id: "c".into(),
                    pid: Some(3),
                    status: Official::Idle
                },
                Entry {
                    session_id: "d".into(),
                    pid: None,
                    status: Official::Waiting("等待你处理".into())
                },
            ]
        );
        assert!(parse(b"not json").is_err());
    }
}

#[cfg(test)]
mod program_tests {
    #[test]
    fn a_claude_run_by_node_is_asked_through_claude() {
        assert_eq!(super::program_for("/opt/homebrew/bin/node"), "claude");
        assert_eq!(
            super::program_for("/Users/me/.nvm/versions/node/v22/bin/node"),
            "claude"
        );
        assert_eq!(super::program_for("/usr/local/bin/bun"), "claude");
        let native = "/Users/me/.local/share/claude/versions/2.1.317";
        assert_eq!(super::program_for(native), native);
    }
}
