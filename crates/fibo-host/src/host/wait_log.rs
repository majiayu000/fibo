//! `screen-waits.jsonl` in the runtime: each time a screen was read as waiting on the user
//! and when that ended, with the rule and the lines it saw. A card asking for an approval
//! nothing asked for leaves no other trace: the screen is gone by the time anyone looks.
//! Screens can hold anything a terminal shows, so it stays on this machine, readable only
//! by its user (`devices::log`), and only the bottom of the screen goes in.
use crate::screen_rules::Explained;
use fibo_core::model::AgentSession;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Enough to hold a prompt and what is above it, or the output mistaken for one.
const LINES: usize = 16;
const LINE_CHARS: usize = 200;

static FILE: OnceLock<PathBuf> = OnceLock::new();

pub(super) fn start(root: &Path) {
    let _ = FILE.set(root.join("screen-waits.jsonl"));
}

fn write(entry: Value) {
    if let Some(path) = FILE.get() {
        crate::devices::log(path, entry);
    }
}

/// The lines worth keeping, each cut to a length: a screen is not a place for megabytes.
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

/// The entry for a screen now read as waiting, if it asks something new: the same question
/// read again on the next quiet screen says nothing more.
fn waiting_entry(
    before: Option<&str>,
    terminal: &str,
    session: Option<&AgentSession>,
    seen: &Explained,
    tail: &str,
) -> Option<Value> {
    let question = seen.question().filter(|q| before != Some(*q))?;
    Some(json!({
        "event": "waiting",
        "terminal": terminal,
        "agent": session.map(|s| s.runtime.key()),
        "session": session.map(|s| s.cli_session_id.as_str()),
        "rule": seen.rule().map(|r| r.id.as_str()),
        "rules": seen.rules.origin(),
        "question": question,
        "screen": bottom(tail),
    }))
}

pub(super) fn waiting(
    before: Option<&str>,
    terminal: &str,
    session: Option<&AgentSession>,
    seen: &Explained,
    tail: &str,
) {
    if let Some(entry) = waiting_entry(before, terminal, session, seen, tail) {
        write(entry);
    }
}

pub(super) fn ended(terminal: &str, question: &str) {
    write(json!({"event": "ended", "terminal": terminal, "question": question}));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_rules::{default_rules, explain};

    const ASKING: &str =
        "Bash command\n  git push origin main\nDo you want to proceed?\n> 1. Yes\n  2. No\n";

    #[test]
    fn a_new_question_is_recorded_with_its_rule_and_screen_and_a_repeat_is_not() {
        let seen = explain(default_rules(), ASKING);
        let question = seen.question().expect("the fixture asks").to_owned();
        let entry = waiting_entry(None, "t", None, &seen, ASKING).unwrap();
        assert_eq!(entry["rule"], "approval");
        assert_eq!(entry["question"], question.as_str());
        assert!(entry["screen"][0]
            .as_str()
            .unwrap()
            .contains("Bash command"));
        assert!(waiting_entry(Some(&question), "t", None, &seen, ASKING).is_none());
    }

    #[test]
    fn a_screen_nothing_waits_on_is_not_recorded() {
        let seen = explain(default_rules(), "all done\n");
        assert!(waiting_entry(None, "t", None, &seen, "all done\n").is_none());
    }

    #[test]
    fn only_the_bottom_of_the_screen_is_kept_and_long_lines_are_cut() {
        let tail = format!("{}{}\n", "old\n".repeat(50), "x".repeat(5000));
        let kept = bottom(&tail);
        assert_eq!(kept.len(), LINES);
        assert_eq!(kept.last().unwrap().chars().count(), LINE_CHARS);
    }
}
