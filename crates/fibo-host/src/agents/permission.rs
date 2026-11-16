//! Permission semantics come from the CLI mode/policy, never from display words.
use serde_json::Value;

pub(super) struct Reading {
    pub words: String,
    pub full: Option<bool>,
}

pub(super) fn claude(mode: &str) -> Reading {
    let (words, full) = match mode {
        "manual" | "default" => (t!("逐个确认"), Some(false)),
        "acceptEdits" => (t!("自动接受编辑"), Some(false)),
        "plan" => (t!("只做计划"), Some(false)),
        "auto" => (t!("自动判断"), Some(false)),
        "dontAsk" => (t!("不问就拒绝"), Some(false)),
        "bypassPermissions" => (t!("全部放行"), Some(true)),
        other => (other, None),
    };
    Reading {
        words: words.to_owned(),
        full,
    }
}

pub(super) fn codex(turn: &Value) -> Option<Reading> {
    let sandbox = turn["sandbox_policy"]["type"].as_str();
    let (words, full) = match (turn["approval_policy"].as_str()?, sandbox) {
        ("never", Some("danger-full-access")) => (t!("全部放行"), Some(true)),
        ("never", Some("read-only")) => (t!("只读"), Some(false)),
        ("never", Some("workspace-write")) => (t!("不问 · 沙箱内"), Some(false)),
        ("never", _) => (t!("不问 · 沙箱内"), None),
        ("on-request" | "on-failure" | "untrusted", _) => (t!("逐个确认"), Some(false)),
        (other, _) => (other, None),
    };
    Some(Reading {
        words: words.to_owned(),
        full,
    })
}

/// A Codex permission menu's label is its protocol key, not our translated words.
pub(super) fn codex_option(key: &str) -> Reading {
    let (words, full) = match key {
        "Ask for approval" => (t!("逐个确认"), Some(false)),
        "Approve for me" => (t!("只问不安全的"), Some(false)),
        "Full Access" => (t!("全部放行"), Some(true)),
        "Read Only" => (t!("只读"), Some(false)),
        other => (other, None),
    };
    Reading {
        words: words.to_owned(),
        full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_access_comes_from_raw_policy_and_unknown_is_not_guessed() {
        assert_eq!(claude("bypassPermissions").full, Some(true));
        assert_eq!(claude("dontAsk").full, Some(false));
        assert_eq!(claude("全部放行").full, None);
        assert_eq!(codex_option("Full Access").full, Some(true));
        assert_eq!(codex_option("Read Only").full, Some(false));
        assert_eq!(codex_option("future").full, None);
        for (sandbox, expected) in [
            ("danger-full-access", Some(true)),
            ("read-only", Some(false)),
            ("workspace-write", Some(false)),
            ("future", None),
        ] {
            assert_eq!(codex(&serde_json::json!({"approval_policy":"never","sandbox_policy":{"type":sandbox}})).unwrap().full, expected);
        }
    }
}
