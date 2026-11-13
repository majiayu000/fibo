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
            ..Default::default()
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
        ..Default::default()
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
            ..Default::default()
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
/// A running Codex turn showing a subagent's dialog.
fn parent_showing_child_dialog(dialog: &str) -> Parser {
    let mut p = Parser::default();
    add(
        &mut p,
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}),
    );
    p.children(
        &Children {
            running: 1,
            waiting: Some(dialog.into()),
            subagents: Vec::new(),
        },
        1,
        Source::default(),
    );
    assert_eq!(p.card.turn, Turn::Waiting);
    p
}
#[test]
fn parent_official_status_cannot_dismiss_a_child_dialog() {
    for status in [Official::Busy, Official::Idle] {
        let mut p = parent_showing_child_dialog("子 Agent：批准运行测试？");
        let waiting = p.card.clone();
        p.official(&status, 2, Source::default());
        assert_eq!(p.card, waiting, "{status:?} describes only the parent");
        let kids = Children {
            running: 1,
            waiting: Some("子 Agent：批准运行测试？".into()),
            subagents: Vec::new(),
        };
        p.children(&kids, 3, Source::default());
        assert_eq!(p.card.waiting_episode, 1, "{status:?}: still one episode");
        p.children(&Children::default(), 4, Source::default());
        assert_eq!(p.card.turn, Turn::Running);
        assert!(p.card.waiting.is_empty());
        assert_eq!(p.child_wait_from, None);
    }
}
#[test]
fn child_dialog_restores_completion_received_while_waiting_after_reload() {
    let mut p = parent_showing_child_dialog("子 Agent：批准运行测试？");
    add(
        &mut p,
        json!({"type":"event_msg","payload":{"type":"task_complete"}}),
    );
    assert_eq!(p.card.turn, Turn::Waiting);
    assert!(p.completed_while_waiting);
    // This state is durable; restarting the parser must not lose the completion.
    let mut p: Parser = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    p.official(&Official::Idle, 2, Source::default());
    assert_eq!(p.card.turn, Turn::Waiting);
    p.children(&Children::default(), 3, Source::default());
    assert_eq!(p.card.turn, Turn::Completed);
    assert!(!p.completed_while_waiting);
    assert_eq!(p.child_wait_from, None);
    assert!(p.card.waiting.is_empty());
    assert!(!p.card.background.unwrap().keeps_working());
}
#[test]
fn ended_child_wait_cannot_dismiss_a_later_parent_dialog() {
    for event in ["error", "turn_aborted"] {
        let mut p = parent_showing_child_dialog("子 Agent：批准运行测试？");
        add(
            &mut p,
            json!({"type":"event_msg","payload":{"type":event,"message":"failed"}}),
        );
        assert_eq!(p.child_wait_from, None, "{event} ends the child projection");
        add(
            &mut p,
            json!({"type":"event_msg","payload":{"type":"approval_request","message":"parent approval"}}),
        );
        let waiting = p.card.waiting.clone();
        p.children(&Children::default(), 2, Source::default());
        assert_eq!(p.card.turn, Turn::Waiting);
        assert_eq!(p.card.waiting, waiting);
        p.official(&Official::Busy, 3, Source::default());
        assert_eq!(p.card.turn, Turn::Running);
        assert!(p.card.waiting.is_empty());
    }
}
#[test]
fn explicit_parent_dialog_takes_ownership_from_a_child_projection() {
    let asked = json!({"questions":[{"question":"parent approval","options":[{"label":"yes"},{"label":"no"}]}]});
    for input in ["event", "tool", "result"] {
        let mut p = parent_showing_child_dialog("child approval");
        let mut choices: Vec<String> = vec!["yes".into(), "no".into()];
        match input {
            "event" => {
                let mut event = asked.clone();
                event["type"] = json!("request_user_input");
                add(&mut p, json!({"type":"event_msg","payload":event}));
            }
            "tool" => call(&mut p, "parent", "AskUserQuestion", asked.clone()),
            _ => {
                choices.clear();
                call(&mut p, "parent", "exec_command", json!({"cmd":"test"}));
                result(
                    &mut p,
                    "parent",
                    "Process exited with code 1: requires approval",
                );
            }
        }
        assert_eq!(p.child_wait_from, None, "{input} owns the new dialog");
        let waiting = p.card.waiting.clone();
        assert!(!waiting.is_empty());
        assert_ne!(waiting, "child approval");
        assert_eq!(p.card.choices, choices, "{input}");
        p.children(&Children::default(), 2, Source::default());
        assert_eq!(p.card.turn, Turn::Waiting, "{input} is not the child's");
        assert_eq!(p.card.waiting, waiting);
        assert_eq!(p.card.choices, choices, "{input}");
        p.official(&Official::Busy, 3, Source::default());
        assert_eq!(p.card.turn, Turn::Running);
        assert!(p.card.waiting.is_empty());
        assert!(p.card.choices.is_empty());
    }
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

/// Claude Code writes an AskUserQuestion call to its log only once it is answered; until
/// then only the PermissionRequest hook knows the question. Without reading its
/// `tool_input` the card said 「CLI 请求工具审批（Hook）」 and the phone had nothing to pick
/// (2026-11-08, a real session stuck on 「下一步」).
#[test]
fn a_question_asked_through_the_permission_hook_keeps_its_question_and_choices() {
    let mut p = Parser::default();
    let s = Source::default();
    let payload = json!({
        "hook_event_name": "PermissionRequest",
        "tool_name": "AskUserQuestion",
        "tool_input": {"questions": [{
            "header": "下一步",
            "question": "接下来怎么继续？",
            "options": [{"label": "新对话继续"}, {"label": "就在这里继续"}, {"label": "先做第 0 幕看效果"}]
        }]}
    });
    // The notification about the same dialog may come first; the question still wins.
    let note = HookFacts {
        notification: Some("permission_prompt".into()),
        ..Default::default()
    };
    p.hook("Notification", &note, 1, s.clone());
    p.hook("PermissionRequest", &hook_facts(&payload), 2, s.clone());
    assert_eq!(p.card.turn, Turn::Waiting);
    assert!(
        p.card.waiting.contains("接下来怎么继续？"),
        "{}",
        p.card.waiting
    );
    assert_eq!(
        p.card.choices,
        ["新对话继续", "就在这里继续", "先做第 0 幕看效果"]
    );
    // Answered: the tool finishes and the question is gone.
    p.hook("PostToolUse", &HookFacts::default(), 3, s.clone());
    assert_eq!(p.card.turn, Turn::Running);
    assert!(p.card.waiting.is_empty() && p.card.choices.is_empty());
    // An approval for any other tool is still an approval.
    let bash = json!({"tool_name": "Bash", "tool_input": {"command": "ls"}});
    assert_eq!(hook_facts(&bash).question, None);
    // A plan to approve says what it is, and the plan; its menu is read off the terminal.
    let plan = json!({"tool_name": "ExitPlanMode", "tool_input": {"plan": "# 建 hello.txt\n## Context\n1. 写入 **hi**\n  2) 读回来"}});
    let (asked, choices) = hook_facts(&plan).question.unwrap();
    assert!(
        asked.starts_with("【计划】") && asked.contains("建 hello.txt"),
        "{asked}"
    );
    // Its steps never read as numbered options a phone would turn into buttons.
    assert!(
        asked.contains("1、写入 hi") && asked.contains("  2、读回来"),
        "{asked}"
    );
    // Markdown marks a phone would show as they are: no `#` or `**`, the words stay.
    assert!(
        asked.contains("\n建 hello.txt\nContext\n")
            && !asked.contains('#')
            && !asked.contains("**"),
        "{asked}"
    );
    assert!(choices.is_empty());
}

#[test]
fn claude_empty_plan_call_reads_the_cli_plan_on_its_first_approval() {
    let root = std::env::temp_dir().join(format!("fibo-plan-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("plans")).unwrap();
    let source = Source {
        path: root
            .join("projects/p/session.jsonl")
            .to_string_lossy()
            .into_owned(),
        ..Default::default()
    };
    let add = |p: &mut Parser, name: &str, args: Value, id: &str| {
        p.add(
            Agent::Claude,
            json!({"type":"assistant","slug":"first-plan",
            "message":{"content":[{"type":"tool_use","name":name,"id":id,"input":args}]}})
            .to_string()
            .as_bytes(),
            source.clone(),
        );
    };
    let plan = root.join("plans/first-plan.md");
    std::fs::write(&plan, "# First plan\n1. Write **hello**\n2. Read it back").unwrap();
    let mut p = Parser::default();
    add(&mut p, "ExitPlanMode", json!({}), "exit");
    assert_eq!(p.card.turn, Turn::Waiting);
    assert!(
        p.card.waiting.contains("1、Write hello"),
        "{}",
        p.card.waiting
    );
    assert!(p.card.choices.is_empty());
    p.hook(
        "Notification",
        &HookFacts {
            notification: Some("permission_prompt".into()),
            ..Default::default()
        },
        1,
        source.clone(),
    );
    assert!(p.card.waiting.contains("Read it back"));
    // Write and ExitPlanMode can be logged together, before the file is written.
    std::fs::remove_file(&plan).unwrap();
    let mut p = Parser::default();
    add(
        &mut p,
        "Write",
        json!({"file_path":plan,"content":"plan"}),
        "write",
    );
    add(&mut p, "ExitPlanMode", json!({}), "exit");
    assert_eq!(p.card.turn, Turn::Waiting);
    // Pending calls already survive a host restart; keep the plan path there as well.
    let mut p: Parser = serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
    std::fs::write(&plan, "# Updated plan\n1. Write hello\n2. Check it").unwrap();
    p.result(
        "write",
        &json!("File created successfully"),
        Some(0),
        true,
        2,
        &source,
    );
    assert_eq!(p.card.turn, Turn::Waiting);
    assert!(
        p.card.waiting.contains("Updated plan"),
        "{}",
        p.card.waiting
    );
    std::fs::write(&plan, "# Latest plan\n1. A\n2. B").unwrap();
    p.hook(
        "PermissionRequest",
        &hook_facts(&json!({"tool_name":"ExitPlanMode", "tool_input":{}})),
        2,
        source.clone(),
    );
    assert!(p.card.waiting.contains("Latest plan"));
    // Claude's actual ExitPlanMode result has no exit code.
    p.result("exit", &json!("User approved"), None, true, 3, &source);
    assert_eq!(p.card.turn, Turn::Running);
    assert!(p.card.waiting.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn plan_file_identity_is_from_the_cli_record_not_tool_arguments() {
    let source = Source {
        path: "/config/projects/project/session.jsonl".into(),
        ..Default::default()
    };
    let args = json!({"planFilePath":"/private/secret",PLAN_PATH:"/private/secret"});
    let safe = claude_plan_args(&json!({"slug":"my-plan"}), &args, &source);
    assert_eq!(safe[PLAN_PATH], "/config/plans/my-plan.md");
    for slug in ["../secret", "/private/secret", "..", ""] {
        let args = claude_plan_args(&json!({"slug":slug}), &args, &source);
        assert!(args.get(PLAN_PATH).is_none(), "{slug}");
    }
    let args = claude_plan_args(&json!({}), &args, &source);
    assert!(args.get(PLAN_PATH).is_none());
}

#[test]
fn parsed_permission_semantics_follow_new_modes_not_the_display_words() {
    let mut p = Parser::default();
    for (mode, full) in [
        ("bypassPermissions", Some(true)),
        ("plan", Some(false)),
        ("future", None),
    ] {
        p.add(
            Agent::Claude,
            json!({"type":"permission-mode","permissionMode":mode})
                .to_string()
                .as_bytes(),
            Source::default(),
        );
        assert_eq!(p.card.permission_full, full);
    }
    for (sandbox, full) in [
        ("danger-full-access", Some(true)),
        ("read-only", Some(false)),
        ("future", None),
    ] {
        p.add(Agent::Codex, json!({"type":"turn_context","payload":{"approval_policy":"never","sandbox_policy":{"type":sandbox}}}).to_string().as_bytes(), Source::default());
        assert_eq!(p.card.permission_full, full);
    }
    let mut saved = serde_json::to_value(mooring_core::content::Content::default()).unwrap();
    saved["permission"] = json!("全部放行");
    saved.as_object_mut().unwrap().remove("permission_full");
    let old: mooring_core::content::Content = serde_json::from_value(saved).unwrap();
    assert_eq!(old.permission_full, None);
}
