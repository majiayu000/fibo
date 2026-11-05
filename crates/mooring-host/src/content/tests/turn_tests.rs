//! How hooks, official status, subagents and interrupts move the turn.
use super::*;
#[test]
fn the_signal_keeps_the_record_or_event_that_changed_the_turn() {
    let mut p = Parser::default();
    let s = Source::default();
    p.add(
        Agent::Claude,
        r#"{"type":"user","uuid":"u","message":{"content":"跑一下测试"}}"#.as_bytes(),
        s.clone(),
    );
    assert_eq!(p.card.signal_detail, "记录：user「跑一下测试」");
    let notification = HookFacts {
        notification: Some("permission_prompt".into()),
        ..Default::default()
    };
    p.hook("Notification", &notification, 10, s.clone());
    assert_eq!(p.card.signal, "Hook");
    assert_eq!(
        p.card.signal_detail,
        "事件：Notification（permission_prompt）"
    );
    // A record that changes nothing leaves the detail of the one that did.
    p.add(
        Agent::Claude,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"还在想"}]}}"#
            .as_bytes(),
        s.clone(),
    );
    assert_eq!(
        p.card.signal_detail,
        "事件：Notification（permission_prompt）"
    );
    p.hook("Stop", &HookFacts::default(), 11, s.clone());
    p.add(
        Agent::Claude,
        r#"{"type":"user","uuid":"v","message":{"content":"继续"}}"#.as_bytes(),
        s.clone(),
    );
    p.add(
        Agent::Claude,
        r#"{"type":"assistant","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"**测试都过了**，已经提交。"}]}}"#.as_bytes(),
        s,
    );
    assert_eq!(p.card.signal, "日志");
    assert_eq!(
        p.card.signal_detail,
        "记录：assistant「测试都过了，已经提交。」"
    );
}
#[test]
fn a_compaction_after_the_last_reply_leaves_the_turn_finished() {
    let mut p = Parser::default();
    let s = Source::default();
    for v in [
        json!({"type":"user","uuid":"u","message":{"content":"修一下"}}),
        json!({"type":"assistant","message":{"model":"claude","stop_reason":"end_turn","content":[{"type":"text","text":"修好了"}]}}),
        json!({"type":"system","subtype":"compact_boundary","content":"Conversation compacted"}),
        json!({"type":"user","uuid":"c","isCompactSummary":true,"isVisibleInTranscriptOnly":true,"message":{"content":"<artifact-content-authored-by-others/>\nThis session is being continued from a previous conversation"}}),
    ] {
        p.add(Agent::Claude, &serde_json::to_vec(&v).unwrap(), s.clone());
    }
    assert_eq!(p.card.turn, Turn::Completed);
    assert_eq!(p.card.asks.last().map(|q| q.text.as_str()), Some("修一下"));
}
#[test]
fn live_hooks_open_a_turn_the_log_has_not_shown() {
    let none = HookFacts::default();
    let s = Source::default();
    // Only the tail of a long turn was read: no start, no end.
    let mut p = Parser::default();
    p.hook("PreToolUse", &none, 1, s.clone());
    assert_eq!(p.card.turn, Turn::Running);
    // A finished turn is not reopened by a straggling tool event.
    let mut p = Parser::default();
    p.hook("Stop", &none, 1, s.clone());
    p.hook("PostToolUse", &none, 2, s.clone());
    assert_eq!(p.card.turn, Turn::Completed);
    // The next prompt opens the next turn before its log record arrives.
    p.hook("UserPromptSubmit", &none, 3, s.clone());
    assert_eq!((p.card.turn, p.card.turn_started), (Turn::Running, 3));
}
#[test]
fn hook_payload_facts_count_running_tasks_by_kind() {
    let facts = hook_facts(&json!({
        "hook_event_name": "Stop",
        "background_tasks": [
            {"id": "1", "type": "subagent", "status": "running"},
            {"id": "2", "type": "shell", "status": "running", "command": "cargo test"},
            {"id": "3", "type": "monitor"},
            {"id": "4", "type": "subagent", "status": "completed"},
            {"id": "5", "type": "workflow", "status": "pending"},
            {"id": "6", "type": "monitor", "status": "running",
             "description": "live updates for artifact 3NA2bQ87qGL9ARV2D1XkK5 (comments)"}
        ],
        "session_crons": [{"id": "c", "schedule": "*/5 * * * *"}],
        "last_assistant_message": "never forwarded"
    }));
    assert_eq!(
        facts.background,
        Some(mooring_core::content::Background {
            agents: 2,
            shells: 1,
            monitors: 1,
            crons: 1,
            jobs: 0,
        })
    );
    assert!(!facts.subagent);
    assert_eq!(
        facts.shells,
        vec![mooring_core::protocol::HookShell {
            id: "2".into(),
            description: String::new(),
            command: "cargo test".into(),
        }],
        "the command rides to the host to find the shell's process"
    );
    let watched = hook_facts(&json!({
        "hook_event_name": "Stop",
        "background_tasks": [{"id": "w", "type": "monitor", "status": "running",
            "description": "live updates for artifact abc (comments)"}],
        "session_crons": []
    }));
    assert_eq!(
        watched.background,
        Some(mooring_core::content::Background::default()),
        "watching a published Artifact is not background work: 等你验收, not 等后台"
    );
    let empty = hook_facts(&json!({"hook_event_name": "Stop"}));
    assert_eq!(
        empty.background, None,
        "missing arrays are unknown, not idle"
    );
    let sub = hook_facts(&json!({"hook_event_name": "PreToolUse", "agent_id": "a1"}));
    assert!(sub.subagent);
    let n = hook_facts(&json!({"notification_type": "permission_prompt"}));
    assert_eq!(n.notification.as_deref(), Some("permission_prompt"));
}
fn facts(agents: u32) -> HookFacts {
    HookFacts {
        background: Some(mooring_core::content::Background {
            agents,
            ..Default::default()
        }),
        ..Default::default()
    }
}
/// Replays one hook sequence and checks every turn change it caused (status trace).
#[test]
fn hook_sequence_replays_to_the_expected_status_trace() {
    let mut p = Parser::default();
    let s = Source::default();
    let none = HookFacts::default();
    p.add(
        Agent::Claude,
        r#"{"type":"user","uuid":"u1","message":{"content":"跑完测试"}}"#.as_bytes(),
        s.clone(),
    );
    // A stop with a running subagent: the turn is over but its work is not.
    p.hook("Stop", &facts(1), 1, s.clone());
    assert!(p.card.background.unwrap().keeps_working());
    // A subagent's own events never move the parent turn.
    let sub = HookFacts {
        subagent: true,
        ..Default::default()
    };
    p.hook("PermissionRequest", &sub, 2, s.clone());
    assert_eq!(p.card.turn, Turn::Completed);
    // The subagent woke the parent: a tool runs without a prompt.
    p.hook("PreToolUse", &none, 3, s.clone());
    assert_eq!(p.card.turn, Turn::Running);
    assert_eq!(p.card.background, None, "a new turn forgets the old tasks");
    // One dialog, reported by two hooks, is one waiting episode.
    p.hook("PermissionRequest", &none, 4, s.clone());
    p.hook(
        "Notification",
        &HookFacts {
            notification: Some("permission_prompt".into()),
            ..Default::default()
        },
        10,
        s.clone(),
    );
    assert_eq!((p.card.turn, p.card.waiting_episode), (Turn::Waiting, 1));
    p.hook("PostToolUse", &none, 11, s.clone());
    assert_eq!(p.card.turn, Turn::Running, "the approved tool finished");
    p.hook("Stop", &facts(0), 12, s.clone());
    // Stragglers after a stop with nothing running change nothing.
    p.hook("PostToolUse", &none, 13, s.clone());
    p.hook(
        "Notification",
        &HookFacts {
            notification: Some("idle_prompt".into()),
            ..Default::default()
        },
        14,
        s.clone(),
    );
    assert_eq!(p.card.turn, Turn::Completed);
    p.hook("SubagentStop", &facts(2), 15, s.clone());
    assert_eq!(
        p.card.turn,
        Turn::Completed,
        "SubagentStop only updates the count"
    );
    assert_eq!(p.card.background.unwrap().agents, 2);
    p.hook("StopFailure", &none, 16, s);
    assert_eq!(p.card.error, "API 错误中止了回合（Hook）");
    assert_eq!(
        p.transitions,
        [
            "Unknown→Running 日志",
            "Running→Completed Hook",
            "Completed→Running Hook",
            "Running→Waiting Hook",
            "Waiting→Running Hook",
            "Running→Completed Hook",
            "Completed→Error Hook",
        ]
    );
}
#[test]
fn codex_subagent_header_names_its_parent_and_children_drive_the_parent() {
    let s = Source::default();
    let mut child = Parser::default();
    child.add(
        Agent::Codex,
        br#"{"type":"session_meta","payload":{"id":"kid","cwd":"/p","source":{"subagent":{"thread_spawn":{"parent_thread_id":"dad","depth":1,"agent_path":"/root/fix","agent_nickname":"Hypatia"}}},"agent_nickname":"Hypatia","parent_thread_id":"dad"}}"#,
        s.clone(),
    );
    assert_eq!(
        (child.card.parent.as_str(), child.card.agent_name.as_str()),
        ("dad", "Hypatia")
    );
    assert_eq!(child.card.hidden_reason, "Codex 子代理");
    let mut p = Parser::default();
    p.add(
        Agent::Codex,
        br#"{"timestamp":1,"type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#,
        s.clone(),
    );
    p.add(
        Agent::Codex,
        r#"{"timestamp":2,"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"已派出子任务"}}"#.as_bytes(),
        s.clone(),
    );
    p.children(
        &Children {
            running: 1,
            waiting: None,
        },
        3,
        s.clone(),
    );
    assert_eq!(p.card.turn, Turn::Completed);
    assert!(p.card.background.unwrap().keeps_working());
    // The subagent's approval is the parent's; once answered the parent is as before.
    let ask = Children {
        running: 1,
        waiting: Some("子 Agent Hypatia：是否运行迁移？".into()),
    };
    p.children(&ask, 4, s.clone());
    assert_eq!(
        (p.card.turn, p.card.signal.as_str()),
        (Turn::Waiting, "子 Agent")
    );
    p.children(&ask, 5, s.clone());
    assert_eq!(p.card.waiting_episode, 1, "the same dialog is one episode");
    p.children(
        &Children {
            running: 0,
            waiting: None,
        },
        6,
        s.clone(),
    );
    assert_eq!(p.card.turn, Turn::Completed);
    assert!(!p.card.background.unwrap().keeps_working());
    // A parent without subagents is left alone: unknown stays unknown.
    let mut lone = Parser::default();
    lone.children(&Children::default(), 1, s);
    assert_eq!(lone.card.background, None);
}
#[test]
fn official_status_only_settles_dialogs() {
    let s = Source::default();
    let none = HookFacts::default();
    let mut p = Parser::default();
    p.add(
        Agent::Claude,
        r#"{"type":"user","uuid":"u1","message":{"content":"部署"}}"#.as_bytes(),
        s.clone(),
    );
    // Busy and idle never start or end a turn on their own: they can lag the log.
    p.official(&Official::Idle, 1, s.clone());
    assert_eq!(p.card.turn, Turn::Running);
    p.hook("PermissionRequest", &none, 2, s.clone());
    p.official(&Official::Busy, 3, s.clone());
    assert_eq!(
        (p.card.turn, p.card.signal.as_str()),
        (Turn::Running, "官方状态")
    );
    assert!(p.card.waiting.is_empty());
    // Esc closed the dialog: no hook says so, the official state is idle.
    p.hook("PermissionRequest", &none, 4, s.clone());
    p.official(&Official::Idle, 5, s.clone());
    assert_eq!(p.card.turn, Turn::Interrupted);
    // A turn end arrived while the dialog was shown; once it is gone the turn is done.
    p.add(
        Agent::Claude,
        r#"{"type":"user","uuid":"u2","message":{"content":"再部署"}}"#.as_bytes(),
        s.clone(),
    );
    p.hook("PermissionRequest", &none, 6, s.clone());
    p.hook("Stop", &none, 7, s.clone());
    assert_eq!(p.card.turn, Turn::Waiting);
    p.official(&Official::Idle, 8, s.clone());
    assert_eq!(p.card.turn, Turn::Completed);
    // A dialog no hook reported.
    let episode = p.card.waiting_episode;
    p.official(&Official::Waiting("有对话框打开".into()), 9, s);
    assert_eq!(p.card.turn, Turn::Waiting);
    assert_eq!(p.card.waiting, "有对话框打开（Claude 官方状态）");
    assert_eq!(p.card.waiting_episode, episode + 1);
}
/// orca#9040 shape: Esc in Claude Code writes `[Request interrupted by user]` and, in
/// most real logs (16 of 22 sampled), no turn_duration or stop before the next prompt.
#[test]
fn claude_interrupted_by_user_ends_the_running_turn() {
    let add = |p: &mut Parser, record: Value| {
        p.add(
            Agent::Claude,
            &serde_json::to_vec(&record).unwrap(),
            Source::default(),
        )
    };
    let mut p = Parser::default();
    add(
        &mut p,
        json!({"type":"user","message":{"role":"user","content":"跑一下全部测试"}}),
    );
    add(
        &mut p,
        json!({"type":"assistant","message":{"model":"claude","stop_reason":"tool_use","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"sleep 100"}}]}}),
    );
    assert_eq!(p.card.turn, Turn::Running);
    add(
        &mut p,
        json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"The user doesn't want to proceed with this tool use."}]}}),
    );
    add(
        &mut p,
        json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}}),
    );
    assert_eq!(p.card.turn, Turn::Interrupted);
    // Mid-reply, the plain form; the next prompt starts a new turn as before.
    add(
        &mut p,
        json!({"type":"user","message":{"role":"user","content":"再试一次"}}),
    );
    assert_eq!(p.card.turn, Turn::Running);
    add(
        &mut p,
        json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}),
    );
    assert_eq!(p.card.turn, Turn::Interrupted);
    assert_eq!(p.card.asks.len(), 2, "the interrupt line is not an ask");
}

/// Claude Code 2.1.317 shape: a conversation handed to its background daemon ends its
/// log mid-turn with `continued-in`, and a forked session goes on with its history.
#[test]
fn claude_continued_in_ends_the_turn_here_and_hides_this_copy() {
    let add = |p: &mut Parser, record: Value| {
        p.add(
            Agent::Claude,
            &serde_json::to_vec(&record).unwrap(),
            Source::default(),
        )
    };
    let mut p = Parser::default();
    add(
        &mut p,
        json!({"type":"user","entrypoint":"cli","userType":"external","cwd":"/w","sessionId":"a","timestamp":"2026-11-03T08:15:28Z","message":{"role":"user","content":"你帮我改啊"}}),
    );
    assert_eq!(p.card.turn, Turn::Running);
    assert!(p.card.hidden_reason().is_none());
    add(
        &mut p,
        json!({"type":"continued-in","timestamp":"2026-11-03T08:15:48.805Z","sessionId":"a","continuedInSessionId":"b"}),
    );
    assert_eq!(p.card.turn, Turn::Completed);
    assert_eq!(p.card.continued_in, "b");
    assert!(p.card.hidden_reason().is_some());
    // `claude --resume a` later: this session is in use again.
    add(
        &mut p,
        json!({"type":"user","entrypoint":"cli","userType":"external","cwd":"/w","sessionId":"a","timestamp":"2026-11-03T09:00:00Z","message":{"role":"user","content":"接着来"}}),
    );
    assert_eq!(p.card.turn, Turn::Running);
    assert!(p.card.continued_in.is_empty());
    assert!(p.card.hidden_reason().is_none());
}
