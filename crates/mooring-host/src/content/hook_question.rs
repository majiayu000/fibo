//! What a PermissionRequest hook asks the person when it is not to run something.
use super::{is_question_tool, question_choices, question_summary, short};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

/// What a PermissionRequest asks when it is not an approval to run something: a question
/// tool's question and options, or a plan to start on. Its menu's words change between
/// Claude versions and modes, so a plan gets no buttons: a phone shows the terminal and keys.
static PLAN_STEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(\d+)[.)]\s+").unwrap());
pub(super) fn hook_question(v: &Value) -> Option<(String, Vec<String>)> {
    let name = v["tool_name"].as_str()?;
    let input = &v["tool_input"];
    if name == "ExitPlanMode" {
        // Its steps are no options: `1.` would be offered as a button pressing 1 on the menu.
        let plan = (input["plan"].as_str().unwrap_or("").trim().lines())
            .map(|l| PLAN_STEP.replace(l, "${1}${2}、").into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        return Some((
            short(&format!("【计划】按这个计划开始做吗？\n{plan}"), 600),
            Vec::new(),
        ));
    }
    let q = Some(question_summary(input)).filter(|q| is_question_tool(name) && !q.is_empty())?;
    Some((q, question_choices(input)))
}
