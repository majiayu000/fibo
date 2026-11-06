use super::*;

#[test]
fn the_working_line_is_a_running_turn() {
    let screen = |line: &str| {
        format!("• Ran cargo test\n  └ ok\n\n{line}\n\n› Ask Codex to do anything\n\n  GPT-6.1-Sol high · ~/mooring\n")
    };
    assert_eq!(
        Codex.screen_busy(&screen("• Working (10m 14s • esc to interrupt)")),
        Some(true)
    );
    assert_eq!(
        Codex.screen_busy(&screen(
            "• Working (32m 57s • esc to interrupt) · 1 background terminal"
        )),
        Some(true)
    );
    assert_eq!(
        Codex.screen_busy(&screen("─ Worked for 3m 2s ─")),
        Some(false)
    );
}
#[test]
fn a_spinner_in_the_title_is_a_running_turn() {
    assert_eq!(Codex.title_busy("⠋ fixing"), Some(true));
    assert_eq!(Codex.title_busy("mooring"), Some(false));
    assert_eq!(Codex.title_busy("Action Required"), None);
    assert_eq!(Codex.title_busy(""), None);
}
#[test]
fn an_unbound_codex_says_its_state_plainly() {
    let note = |title: &str| Codex.title_status(title).unwrap().note.note;
    assert_eq!(note("⠋ fixing"), "codex · 工作中 · 未关联对话");
    assert_eq!(note("Action Required"), "codex · 需要你处理 · 未关联对话");
    assert_eq!(note(""), "codex · 未关联对话");
    assert_eq!(
        Codex.title_status("⠋").unwrap().text,
        "仅显示状态：工作中；发下一句后自动关联对话"
    );
}

#[test]
fn only_the_innermost_codex_process_waits_for_a_screen() {
    let process = |pid, parent, no_daemon| crate::recognition::Process {
        pid,
        parent,
        runtime: Some(mooring_core::model::Agent::Codex),
        cli_session_id: None,
        no_daemon,
        launch: vec![],
    };
    let wrapper = process(1, 0, false);
    let native = process(2, 1, false);
    let all = [wrapper.clone(), native.clone()];
    assert_eq!(Codex.pending(&wrapper, &all), Pending::Wait);
    assert_eq!(
        Codex.pending(&native, &all),
        Pending::Unbound { title_status: true }
    );
    // `--no-daemon`: unbound, but nothing to say until its first prompt.
    assert_eq!(Codex.pending(&process(3, 0, true), &[]), Pending::Quiet);
}
#[test]
fn token_counts_keep_the_newest_account_limits() {
    let record = |limits: Value| json!({"type": "event_msg", "payload": {"type": "token_count", "rate_limits": limits}});
    let mut parser = Parser::default();
    let source = Source::default();
    Codex.add(
        &mut parser,
        &record(json!({
            "primary": {"used_percent": 19.0, "window_minutes": 300, "resets_at": 1793399825},
            "secondary": {"used_percent": 88.6, "window_minutes": 10080, "resets_at": 1793774131},
            "plan_type": "plus"
        })),
        200,
        &source,
    );
    let q = parser.card.quota.clone().unwrap();
    assert_eq!((q.at, q.plan.as_str()), (200, "plus"));
    assert_eq!(
        q.windows
            .iter()
            .map(|w| (w.label(), w.used))
            .collect::<Vec<_>>(),
        [("5h".to_string(), 19), ("周".to_string(), 89)]
    );
    // A weekly-only plan, then an older record replayed: the newer reading stays.
    Codex.add(
        &mut parser,
        &record(json!({
            "primary": {"used_percent": 3.0, "window_minutes": 10080, "resets_at": 1},
            "secondary": null, "plan_type": "pro"
        })),
        300,
        &source,
    );
    Codex.add(
        &mut parser,
        &record(json!({"primary": {"used_percent": 50.0, "window_minutes": 300}})),
        100,
        &source,
    );
    let q = parser.card.quota.clone().unwrap();
    assert_eq!((q.at, q.plan.as_str(), q.windows.len()), (300, "pro", 1));
    // No windows (an API-key sign-in) is no reading, not an empty one.
    Codex.add(
        &mut parser,
        &record(json!({"primary": null, "secondary": null})),
        400,
        &source,
    );
    Codex.add(&mut parser, &record(Value::Null), 500, &source);
    assert_eq!(parser.card.quota.unwrap().at, 300);
    assert_eq!(parser.card.unknown_records, 0);
}
use std::io::Write;

#[test]
fn thread_names_follow_appended_renames_and_wait_for_whole_lines() {
    let home = std::env::temp_dir().join(format!("mooring-codex-names-{}", std::process::id()));
    std::fs::create_dir_all(home.join("sessions")).unwrap();
    let index = home.join("session_index.jsonl");
    let id = "01a18aad-c882-79a2-9d72-3ee5359f0add";
    let rollout = home.join(format!("sessions/rollout-2026-10-30T10-40-10-{id}.jsonl"));
    std::fs::write(
            &index,
            format!("{{\"id\":\"{id}\",\"thread_name\":\"查找网站\"}}\n{{\"id\":\"other\",\"thread_name\":\"别的\"}}\n"),
        )
        .unwrap();
    let root = home.join("sessions");
    assert_eq!(Codex.name(&root, &rollout).as_deref(), Some("查找网站"));
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&index)
        .unwrap();
    // Half a line is not a rename yet.
    write!(f, "{{\"id\":\"{id}\",\"thread_name\":\"排查").unwrap();
    assert_eq!(Codex.name(&root, &rollout).as_deref(), Some("查找网站"));
    writeln!(f, " mirror 停止运行\"}}").unwrap();
    assert_eq!(
        Codex.name(&root, &rollout).as_deref(),
        Some("排查 mirror 停止运行")
    );
    assert_eq!(Codex.name(&root, &root.join("rollout-x.jsonl")), None);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_app_state_names_threads_the_index_missed_and_wins_over_it() {
    let home = std::env::temp_dir().join(format!("mooring-codex-state-{}", uuid::Uuid::new_v4()));
    let root = home.join("sessions");
    std::fs::create_dir_all(&root).unwrap();
    let id = "01a18aad-c882-79a2-9d72-3ee5359f0add";
    let other = "019de0c2-0000-7000-8000-000000000000";
    let rollout = |id: &str| root.join(format!("rollout-2026-10-30T10-40-10-{id}.jsonl"));
    std::fs::write(
            home.join("session_index.jsonl"),
            format!("{{\"id\":\"{id}\",\"thread_name\":\"旧名\"}}\n{{\"id\":\"{other}\",\"thread_name\":\"只在索引\"}}\n"),
        )
        .unwrap();
    // An older schema without names, and the newest one with them.
    Connection::open(home.join("state_4.sqlite"))
        .unwrap()
        .execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY)")
        .unwrap();
    let db = Connection::open(home.join("state_5.sqlite")).unwrap();
    db.execute_batch(&format!(
        "CREATE TABLE threads(id TEXT PRIMARY KEY, name TEXT);
             INSERT INTO threads VALUES('{id}',' 查找 Codex 聊天中的网站 '),('{other}',NULL);"
    ))
    .unwrap();
    assert_eq!(
        Codex.name(&root, &rollout(id)).as_deref(),
        Some("查找 Codex 聊天中的网站")
    );
    assert_eq!(
        Codex.name(&root, &rollout(other)).as_deref(),
        Some("只在索引")
    );
    // A schema that lost the column hands naming back to the index.
    db.execute_batch("ALTER TABLE threads DROP COLUMN name")
        .unwrap();
    drop(db);
    assert_eq!(Codex.name(&root, &rollout(id)).as_deref(), Some("旧名"));
    std::fs::remove_dir_all(home).unwrap();
}
