use super::*;

#[test]
fn timestamps_keep_rfc3339_offsets_and_unknown_values_stay_zero() {
    for text in [
        "2026-08-25T03:21:00Z",
        "2026-08-25T11:21:00+08:00",
        "2026-08-24T23:51:00-03:30",
        "2026-08-25T03:21:00.123Z",
    ] {
        assert_eq!(seconds(&json!(text)), 1_787_628_060, "{text}");
    }
    assert_eq!(seconds(&json!(1_787_628_060_u64)), 1_787_628_060);
    for value in [
        json!("1969-12-31T23:59:59Z"),
        json!("bad"),
        json!("2026-02-30T03:21:00Z"),
        json!("2026-08-25T03:21:00"),
        json!("1787628060"),
        json!(-1),
        Value::Null,
    ] {
        assert_eq!(seconds(&value), 0, "{value}");
    }
}

/// 待办 3636b388: a question answered three hours later. The clock of the turn that goes
/// on counts the time it worked, not the three hours it waited for the answer.
#[test]
fn the_turn_clock_leaves_out_the_time_a_question_waited() {
    let mut p = Parser::default();
    let s = Source::default();
    let start = 1_787_628_000;
    let hours = 3 * 3600;
    p.hook("UserPromptSubmit", &HookFacts::default(), start, s.clone());
    let asked = json!({"tool_name": "AskUserQuestion", "tool_input": {"questions": [{
        "question": "部署吗？", "options": [{"label": "是"}, {"label": "否"}]
    }]}});
    p.hook(
        "PermissionRequest",
        &hook_facts(&asked),
        start + 60,
        s.clone(),
    );
    assert_eq!(p.card.turn, Turn::Waiting);
    p.hook(
        "PostToolUse",
        &HookFacts::default(),
        start + 60 + hours,
        s.clone(),
    );
    assert_eq!(p.card.turn, Turn::Running);
    assert_eq!(p.card.running_since(), Some(start + hours));
    // A second question in the same turn adds its wait too.
    p.hook(
        "PermissionRequest",
        &hook_facts(&asked),
        start + hours + 120,
        s.clone(),
    );
    p.hook(
        "PostToolUse",
        &HookFacts::default(),
        start + hours + 720,
        s.clone(),
    );
    assert_eq!(p.card.running_since(), Some(start + hours + 600));
    // The next turn starts its clock afresh.
    p.hook(
        "Stop",
        &HookFacts::default(),
        start + hours + 800,
        s.clone(),
    );
    p.hook(
        "UserPromptSubmit",
        &HookFacts::default(),
        start + hours + 900,
        s,
    );
    assert_eq!(p.card.running_since(), Some(start + hours + 900));
}
