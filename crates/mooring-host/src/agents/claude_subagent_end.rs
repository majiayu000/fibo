//! A subagent whose meta never says it ended. Stopped mid-turn (a rejected tool, Esc, a
//! `TaskStop`) or cut off by an API error, Claude Code can leave `finished` and
//! `stoppedByUser` unwritten. The parent's task notification says so, but a long parent
//! log is read only at its ends, and that line can sit in the middle. The subagent's own
//! log ends on the line that stopped it, and nothing follows until someone continues it.
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Enough for the short line that ends a stopped subagent.
const TAIL: u64 = 16 * 1024;

/// The subagent's own log ends on an interruption or an API error.
pub(super) fn stopped_in_log(log: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(log) else {
        return false;
    };
    let Ok(len) = f.metadata().map(|m| m.len()) else {
        return false;
    };
    if f.seek(SeekFrom::Start(len.saturating_sub(TAIL))).is_err() {
        return false;
    }
    let mut tail = Vec::new();
    if f.read_to_end(&mut tail).is_err() {
        return false;
    }
    // A last line longer than the tail does not parse: unknown, so still running.
    let Some(v) = tail
        .split(|b| *b == b'\n')
        .rev()
        .find(|l| !l.iter().all(u8::is_ascii_whitespace))
        .and_then(|l| serde_json::from_slice::<Value>(l).ok())
    else {
        return false;
    };
    if v["isApiErrorMessage"] == true {
        return true;
    }
    v["type"] == "user"
        && crate::content::text(&v["message"]["content"])
            .trim_start()
            .starts_with("[Request interrupted by user")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_ending_on_an_interruption_or_an_api_error_has_stopped() {
        let dir = std::env::temp_dir().join(format!("mooring-subagent-end-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("agent-a1.jsonl");
        let ends = |last: &str| {
            let call = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}]}}"#;
            std::fs::write(&log, format!("{call}\n{last}\n")).unwrap();
            stopped_in_log(&log)
        };
        // The shape a rejected tool leaves (11-09, a fork stopped at 19:17 with no meta mark).
        assert!(ends(
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}}"#
        ));
        assert!(ends(
            r#"{"type":"assistant","isApiErrorMessage":true,"message":{"content":[{"type":"text","text":"API Error: Connection closed"}]}}"#
        ));
        // Mid-call, or a prompt merely quoting the words: still running.
        assert!(!ends(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}}"#
        ));
        assert!(!ends(
            r#"{"type":"user","message":{"content":"为什么会出现 [Request interrupted by user]"}}"#
        ));
        assert!(!stopped_in_log(&dir.join("missing.jsonl")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
