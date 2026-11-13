//! What a PermissionRequest hook asks the person when it is not to run something.
use super::{is_question_tool, question_choices, question_summary, short, Parser, Source, Turn};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

/// What a PermissionRequest asks when it is not an approval to run something: a question
/// tool's question and options, or a plan to start on. Its menu's words change between
/// Claude versions and modes, so a plan gets no buttons: a phone shows the terminal and keys.
static PLAN_STEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(\d+)[.)]\s+").unwrap());
/// A plan is markdown, and a phone shows the card as plain words: `# 计划` and `**要点**`
/// read as their marks (2026-11-10 walk), so the heading marks and bold go; and the
/// backticks round inline code (`` `epsilon` ``, 2026-11-12 walk). A fence line stays.
static PLAN_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)#{1,6}\s+").unwrap());
static PLAN_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`\n]+)`").unwrap());
pub(super) fn hook_question(v: &Value) -> Option<(String, Vec<String>)> {
    let name = v["tool_name"].as_str()?;
    let input = &v["tool_input"];
    if name == "ExitPlanMode" {
        let plan = input["plan"].as_str()?.trim();
        if plan.is_empty() {
            return None;
        }
        // Its steps are no options: `1.` would be offered as a button pressing 1 on the menu.
        let plan = plan
            .lines()
            .map(|l| PLAN_HEADING.replace(l, "${1}").replace("**", ""))
            .map(|l| PLAN_CODE.replace_all(&l, "${1}").into_owned())
            .map(|l| PLAN_STEP.replace(&l, "${1}${2}、").into_owned())
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

/// The card's words while a plan waits: its text, or a pointer to the terminal until it is read.
pub(super) fn plan_waiting(args: &Value) -> String {
    hook_question(&serde_json::json!({"tool_name": "ExitPlanMode", "tool_input": args}))
        .map(|q| q.0)
        .unwrap_or_else(|| "【计划】计划正文暂未读到，请点「看终端」核对".into())
}

pub(super) const PLAN_PATH: &str = "_mooring_plan_path";

/// Claude 2.1.322 can log ExitPlanMode {}. Its own slug names the plan beside the
/// transcript's projects directory, including a custom CLAUDE_CONFIG_DIR. Never use a
/// path supplied in arbitrary tool arguments, or a slug containing path components.
pub(crate) fn claude_plan_args(record: &Value, input: &Value, source: &Source) -> Value {
    use std::path::{Component, Path};
    let mut input = input.clone();
    let Some(args) = input.as_object_mut() else {
        return input;
    };
    args.remove(PLAN_PATH);
    let Some(slug) = record["slug"].as_str() else {
        return input;
    };
    let mut parts = Path::new(slug).components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return input;
    }
    if let Some(config) = Path::new(&source.path)
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "projects"))
        .and_then(Path::parent)
    {
        args.insert(
            PLAN_PATH.into(),
            Value::String(
                config
                    .join("plans")
                    .join(format!("{slug}.md"))
                    .to_string_lossy()
                    .into_owned(),
            ),
        );
    }
    input
}

impl Parser {
    /// Read only while a plan approval is pending. A Write result or its permission
    /// notification can arrive after ExitPlanMode was logged, when the file now exists.
    pub(super) fn refresh_plan(&mut self) {
        use std::io::Read;
        if matches!(
            self.card.turn,
            Turn::Interrupted | Turn::Completed | Turn::Error
        ) {
            return;
        }
        let path = self
            .calls
            .values()
            .filter(|c| c.name == "ExitPlanMode" && c.at >= self.card.turn_started)
            .max_by_key(|c| c.source.offset)
            .and_then(|c| c.input[PLAN_PATH].as_str());
        let Some(mut file) = path.and_then(|p| std::fs::File::open(p).ok()) else {
            return;
        };
        let mut bytes = Vec::new();
        if file.by_ref().take(8192).read_to_end(&mut bytes).is_err() {
            return;
        }
        let plan = String::from_utf8_lossy(&bytes);
        if let Some((question, choices)) = hook_question(
            &serde_json::json!({"tool_name":"ExitPlanMode","tool_input":{"plan":plan}}),
        ) {
            self.card.turn = Turn::Waiting;
            self.card.waiting = question;
            self.card.choices = choices;
        }
    }
}
