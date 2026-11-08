//! What a paired device may do. The machine enforces it; a client reads it from `hello`
//! to show only what will work.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Sees terminals, screens and events.
    View,
    /// Also answers approvals.
    Answer,
    /// Also types and presses keys.
    Full,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "view" => Some(Self::View),
            "answer" => Some(Self::Answer),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
    /// As people read it.
    pub fn label(self) -> &'static str {
        match self {
            Self::View => "只看",
            Self::Answer => "能答审批",
            Self::Full => "完全控制",
        }
    }
    pub fn allows(self, op: &str) -> bool {
        match op {
            // `dirs` shows the machine's folders, so it is no read for a lesser role.
            "send" | "keys" | "input" | "resize" | "open" | "end" | "outcome" | "dirs"
            | "mkdir" | "stop" | "lastsession" | "addspace" | "trust" | "modes" | "setmode"
            | "todoadd" | "todooutcome" => self == Self::Full,
            "answer" => self != Self::View,
            _ => true,
        }
    }
}

/// Ops that change something on the machine; the machine logs each one.
pub fn acts(op: &str) -> bool {
    matches!(
        op,
        "send"
            | "keys"
            | "input"
            | "answer"
            | "open"
            | "end"
            | "outcome"
            | "mkdir"
            | "stop"
            | "addspace"
            | "trust"
            | "setmode"
            | "todoadd"
            | "todooutcome"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_allow_only_what_they_name() {
        for op in [
            "hello",
            "list",
            "cards",
            "read",
            "chat",
            "watch",
            "events",
            "close",
            "diff",
            "quota",
            "usage",
            "toolcalls",
            "toolcall",
            "backfill",
            "suggestion",
            "ask",
        ] {
            assert!(Role::View.allows(op));
        }
        for op in [
            "dirs",
            "mkdir",
            "stop",
            "open",
            "lastsession",
            "addspace",
            "trust",
            "todoadd",
            "todooutcome",
        ] {
            assert!(!Role::Answer.allows(op) && Role::Full.allows(op), "{op}");
        }
        assert!(!Role::View.allows("answer") && !Role::View.allows("send"));
        assert!(Role::Answer.allows("answer") && !Role::Answer.allows("keys"));
        assert!(Role::Full.allows("send") && Role::Full.allows("keys"));
        assert_eq!(Role::parse("admin"), None);
        assert_eq!(serde_json::to_string(&Role::Answer).unwrap(), r#""answer""#);
        assert!(acts("keys") && acts("mkdir") && acts("addspace") && !acts("watch"));
        assert!(acts("todoadd") && acts("todooutcome"));
        assert!(!acts("diff") && !acts("dirs") && !acts("quota") && !acts("usage"));
        assert!(!acts("toolcalls") && !acts("toolcall") && !acts("suggestion"));
    }
}
