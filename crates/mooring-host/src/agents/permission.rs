//! How each CLI's log says it asks for permission, in the words the phone shows. Read from
//! the log as last written: a mode changed since then shows once the CLI next writes it.
use serde_json::Value;

/// Claude Code's permission mode; a new mode keeps its own name.
pub(super) fn claude(mode: &str) -> String {
    match mode {
        "default" => "逐个确认",
        "acceptEdits" => "自动接受编辑",
        "plan" => "只做计划",
        "auto" => "自动判断",
        "dontAsk" => "不问就拒绝",
        "bypassPermissions" => "全部放行",
        other => other,
    }
    .to_owned()
}

/// A Codex turn's approval policy and sandbox.
pub(super) fn codex(turn: &Value) -> Option<String> {
    let sandbox = turn["sandbox_policy"]["type"].as_str();
    Some(match (turn["approval_policy"].as_str()?, sandbox) {
        ("never", Some("danger-full-access")) => "全部放行".into(),
        ("never", Some("read-only")) => "只读".into(),
        ("never", _) => "不问 · 沙箱内".into(),
        ("on-request" | "on-failure" | "untrusted", _) => "逐个确认".into(),
        (other, _) => other.to_owned(),
    })
}
