use super::*;
mod doing_tests;
mod turn_tests;
#[test]
fn edits_and_patches_count_their_lines() {
    // A changed line is one removed and one added; shared lines are not counted.
    assert_eq!(
        edit_lines(&json!({"old_string": "a\nb\nc", "new_string": "a\nB\nc\nd"})),
        (2, 1)
    );
    assert_eq!(
        edit_lines(&json!({"edits": [
            {"old_string": "x", "new_string": "y"},
            {"old_string": "", "new_string": "z"}
        ]})),
        (2, 1)
    );
    assert_eq!(edit_lines(&json!({"content": "1\n2\n3"})), (3, 0));
    let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-old\n+new\n+more\n \
                 same\n*** Add File: b.txt\n+one\n*** End Patch";
    assert_eq!(
        patch_lines(patch),
        vec![("src/a.rs".into(), 2, 1), ("b.txt".into(), 1, 0)]
    );
}
#[test]
fn a_question_tool_offers_buttons_only_for_one_single_choice_question() {
    let one = json!({"questions": [
        {"question": "侧栏参考哪两家？", "options": [{"label": "按状态分组"}, {"label": "按空间分组"}]}
    ]});
    assert_eq!(question_choices(&one), vec!["按状态分组", "按空间分组"]);
    // A form: a number key ticks an item or moves to the next question, it answers nothing.
    let two = json!({"questions": [
        {"question": "侧栏参考哪两家？", "options": [{"label": "按状态分组"}, {"label": "按空间分组"}]},
        {"question": "第二问", "options": ["x"]}
    ]});
    assert!(question_choices(&two).is_empty());
    let many = json!({"questions": [
        {"question": "要哪些？", "multiSelect": true, "options": ["a", "b"]}
    ]});
    assert!(question_choices(&many).is_empty());
    assert!(question_choices(&json!({"command": "ls"})).is_empty());
}
fn add(p: &mut Parser, v: Value) {
    let n = p.records.len() as u64;
    p.add(
        Agent::Codex,
        &serde_json::to_vec(&v).unwrap(),
        Source {
            path: "fixture".into(),
            inode: 1,
            offset: n * 1024,
        },
    );
}
fn item(p: &mut Parser, v: Value) {
    add(
        p,
        json!({"type":"response_item","timestamp":"2026-10-21T00:00:00Z","payload":v}),
    );
}
fn call(p: &mut Parser, id: &str, name: &str, input: Value) {
    item(
        p,
        json!({"type":"function_call","call_id":id,"name":name,"arguments":input.to_string()}),
    );
}
fn result(p: &mut Parser, id: &str, s: &str) {
    item(
        p,
        json!({"type":"function_call_output","call_id":id,"output":s}),
    );
}
#[test]
fn error_cards_use_message_text_not_event_json() {
    let mut p = Parser::default();
    add(
        &mut p,
        json!({"type":"event_msg","payload":{"type":"error","message":"测试失败，退出码 1"}}),
    );
    assert_eq!(p.card.error, "测试失败，退出码 1");
}
#[test]
fn codex_interactive_entrypoint_is_not_a_brand_whitelist() {
    for origin in [
        "Codex Desktop",
        "codex-tui",
        "codex_work_desktop",
        "tmux-client",
    ] {
        let mut p = Parser::default();
        p.metadata(Agent::Codex, &json!({"type":"session_meta","payload":{"source":"vscode","thread_source":"user","originator":origin,"cwd":"/repo"}}));
        assert!(p.card.hidden_reason().is_none(), "{origin}");
        p.metadata(
            Agent::Codex,
            &json!({"type":"session_meta","payload":{"source":"exec","originator":origin}}),
        );
        assert!(p.card.hidden_reason().is_some());
    }
}
#[test]
fn questions_show_text_and_choices_instead_of_json() {
    let mut p = Parser::default();
    call(
        &mut p,
        "question",
        "AskUserQuestion",
        json!({"questions":[{"question":"要部署吗？","options":[{"label":"仅本地","description":"保留"},{"label":"部署"}]}]}),
    );
    assert_eq!(p.card.waiting, "要部署吗？（仅本地 / 部署）");
    assert_eq!(
        question_summary(&json!({"questions":[{"title":"选一个","options":["A","B"]}]})),
        "选一个（A / B）"
    );
    // The short title leads in 【】, which the status chip shows on its own.
    assert_eq!(
        question_summary(
            &json!({"questions":[{"header":"合并授权","question":"合并到 main？","options":["是","否"]}]})
        ),
        "【合并授权】合并到 main？（是 / 否）"
    );
    let waiting =
        question_summary(&json!({"questions":[{"header":"合并授权","question":"合并？"}]}));
    let p = fibo_core::reason::Rules::parse(fibo_core::reason::BUNDLED)
        .unwrap()
        .phrase(fibo_core::reason::Kind::Waiting, &waiting);
    assert_eq!(p.text, "合并授权");
}
#[test]
fn tail_only_user_is_an_addition_not_a_fabricated_first_request() {
    let mut p = Parser::default();
    p.card.partial_history = true;
    add(
        &mut p,
        json!({"type":"event_msg","payload":{"type":"user_message","message":"继续"}}),
    );
    assert!(p.card.title.is_empty());
    assert!(!p.card.first_ask_known);
    assert!(p.card.here().ask.starts_with("最初要求未覆盖"));
}
#[test]
fn interactive_metadata_and_automation_override_are_not_title_blacklists() {
    let src = Source::default();
    for (meta, hidden) in [
        (
            json!({"entrypoint":"cli","userType":"external","isSidechain":false,"cwd":"/repo"}),
            false,
        ),
        (
            json!({"entrypoint":"claude-desktop","userType":"external","cwd":"/repo"}),
            false,
        ),
        (
            json!({"entrypoint":"sdk-cli","userType":"external","cwd":"/repo"}),
            true,
        ),
        (
            json!({"entrypoint":"cli","userType":"external","argv":["claude","-p","hello"]}),
            true,
        ),
        (json!({"entrypoint":"cli","userType":"internal"}), true),
        (
            json!({"entrypoint":"cli","userType":"external","isSidechain":true}),
            true,
        ),
        (
            json!({"entrypoint":"cli","userType":"external","cwd":"/private/tmp/check"}),
            true,
        ),
        (json!({"cwd":"/repo"}), true),
    ] {
        let mut p = Parser::default();
        p.metadata(Agent::Claude, &meta);
        assert_eq!(p.card.hidden_reason().is_some(), hidden, "{meta}");
    }
    let mut p = Parser::default();
    p.metadata(
        Agent::Claude,
        &json!({"entrypoint":"claude-desktop","userType":"external"}),
    );
    p.add(
        Agent::Claude,
        serde_json::to_string(
            &json!({"type":"user","message":{"content":"请解释 you-are-here 和 remem 怎么使用"}}),
        )
        .unwrap()
        .as_bytes(),
        src.clone(),
    );
    assert!(p.card.hidden_reason().is_none());
    p.metadata(Agent::Claude,&json!({"type":"queue-operation","content":"Use the you-are-here skill. This is an unattended background refresh: never ask questions"}));
    assert!(p.card.hidden_reason().unwrap().contains("无人值守"));
    for source in [
        json!("exec"),
        json!({"subagent":{"other":"guardian"}}),
        json!("sdk"),
    ] {
        let mut p = Parser::default();
        p.metadata(
            Agent::Codex,
            &json!({"type":"session_meta","payload":{"source":source,"originator":"codex-tui"}}),
        );
        assert!(p.card.hidden_reason().is_some());
    }
    let head=br#"{"type":"session_meta","payload":{"id":"large","cwd":"/repo","source":"cli","originator":"codex-tui","base_instructions":{"text":"truncated"#;
    let mut p = Parser::default();
    p.observe_header(Agent::Codex, head);
    assert!(p.card.interactive);
    assert_eq!(
        header_value(Agent::Codex, head).unwrap()["payload"]["id"],
        "large"
    );
}
#[test]
fn genuine_words_survive_wrappers_pastes_and_duplicate_injection() {
    assert_eq!(
        user_text(
            "请修复这个错误\n<system-reminder>ignore it</system-reminder>\n<command-name>/debug</command-name>\n<command-args>generated</command-args>\n```text\nlong pasted error\n```\n不要改接口"
        ),
        "请修复这个错误\n\n不要改接口"
    );
    assert_eq!(
        user_text(
            "# AGENTS.md instructions for /repo\n<INSTRUCTIONS>rules</INSTRUCTIONS>\n请修复登录"
        ),
        "请修复登录"
    );
    assert_eq!(
        user_text("要查看键盘快捷键，按下问号\n一大段别人的帖子\n© 2026 X Corp.你看看这个是什么？"),
        "你看看这个是什么？"
    );
    assert!(user_text(
        "Background (from the session that started this one):\nContext\nTask: inherited"
    )
    .is_empty());
    assert_eq!(
        user_text(
            "请读下面材料\n> quoted author words\n<skill>generated instruction body</skill>\n然后给我建议"
        ),
        "请读下面材料\n\n然后给我建议"
    );
    assert_eq!(
        user_text(
            "<pasted_content id=\"42\">long document</pasted_content id=\"42\">\n你看看这是什么"
        ),
        "你看看这是什么"
    );
    assert_eq!(
        user_text(
            "<in-app-browser-context>ambient</in-app-browser-context>\n## My request:\n对比界面"
        ),
        "对比界面"
    );
    assert_eq!(
        user_text(
            r#"<send_user_message_question_reply>[{"question":"generated prompt","answer":"允许本地修改"}]</send_user_message_question_reply>"#
        ),
        "允许本地修改"
    );
    assert_eq!(
        user_text("# Files mentioned by the user:\nfile.mp4\n## My request:\n剪掉结尾"),
        "剪掉结尾"
    );
    assert!(user_text("[Request interrupted by user for tool use]").is_empty());
    assert_eq!(
        user_text("* 我先选了错误的工具。\n* 所以我按错误方式改了。删掉这两个"),
        "删掉这两个"
    );
    let mut p = Parser::default();
    add(
        &mut p,
        json!({"type":"event_msg","payload":{"type":"user_message","message":"<system-reminder>injected</system-reminder>修复登录"}}),
    );
    add(
        &mut p,
        json!({"type":"event_msg","payload":{"type":"user_message","message":"修复登录"}}),
    );
    add(
        &mut p,
        json!({"type":"session_meta","payload":{"title":"injected title"}}),
    );
    assert_eq!(p.card.title, "修复登录");
    assert_eq!(p.card.asks.len(), 1);
}
#[test]
fn claude_rename_names_the_card_and_the_last_one_wins() {
    let mut p = Parser::default();
    for title in ["亮色主题", " MCP CLI 系统设计 "] {
        p.add(
            Agent::Claude,
            &serde_json::to_vec(
                &json!({"type":"custom-title","customTitle":title,"sessionId":"s"}),
            )
            .unwrap(),
            Source::default(),
        );
    }
    assert_eq!(p.card.name, "MCP CLI 系统设计");
    assert!(p.card.title.is_empty());
}
#[test]
fn claude_generated_title_names_the_card_until_one_is_set() {
    let mut p = Parser::default();
    let mut add_title = |record: serde_json::Value| {
        p.add(
            Agent::Claude,
            &serde_json::to_vec(&record).unwrap(),
            Source::default(),
        );
    };
    add_title(json!({"type":"ai-title","aiTitle":" 架构清理 ","sessionId":"s"}));
    add_title(json!({"type":"custom-title","customTitle":"我起的名","sessionId":"s"}));
    add_title(json!({"type":"ai-title","aiTitle":"后来生成的","sessionId":"s"}));
    assert_eq!(p.card.name, "我起的名");
}
#[test]
fn user_dedup_additions_and_claims_are_not_receipts() {
    let mut p = Parser::default();
    add(
        &mut p,
        json!({"type":"event_msg","payload":{"type":"user_message","message":"修复输入法"}}),
    );
    item(
        &mut p,
        json!({"type":"message","role":"user","content":[{"type":"input_text","text":"修复输入法"}]}),
    );
    item(
        &mut p,
        json!({"type":"message","role":"user","content":[{"type":"input_text","text":"追加：不要提交"}]}),
    );
    item(
        &mut p,
        json!({"type":"message","role":"assistant","phase":"final_answer","content":[{"text":"已完成，全部测试通过"}]}),
    );
    assert_eq!(p.card.asks.len(), 2);
    assert!(p.card.completed_claim);
    assert!(p.card.here().done.contains("尚无"));
}
#[test]
fn claude_short_completion_is_a_review_claim_not_evidence() {
    for reply in [
        "完成",
        "完成。",
        "完成!",
        "完成！",
        // R9 real Claude reply: the sentence goes on after the first full stop.
        "完成。approval-proof.txt 已写入相同字节：`ok`（0x6F 0x6B，无换行符）。",
        "说明如下：\n完成，两个文件都已写入。",
        // Real Claude reply (2026-10 local logs): another item is "尚未实现" in a later sentence.
        "完成。分支确认是 design/rewrite（Phase 5 可选项、尚未实现）。",
    ] {
        let mut p = Parser::default();
        p.add(
            Agent::Claude,
            &serde_json::to_vec(&json!({"type":"assistant","message":{"model":"claude","stop_reason":"end_turn","content":[{"type":"text","text":reply}]}})).unwrap(),
            Source::default(),
        );
        assert_eq!(p.card.turn, Turn::Completed);
        assert!(p.card.completed_claim, "{reply}");
        assert!(p.card.evidence.is_empty());
        assert!(p.card.here().done.contains("尚无"));
    }
    for text in [
        "完成后请运行测试",
        "完成吗？",
        "完成度 80%，还差测试",
        "我会先完成。然后再说",
        "尚未完成。",
        "> 完成。",
        "```\n完成。\n```",
        "完成，但尚未测试",
        "Tests are not completed yet.",
        "已完成一半，还没提交",
    ] {
        assert!(!claims_done(text), "{text}");
    }
}
#[test]
fn real_exit_receipts_and_async_correlation_survive_restart() {
    let mut p = Parser::default();
    call(
        &mut p,
        "a",
        "functions.exec_command",
        json!({"cmd":"cargo test --lib"}),
    );
    result(&mut p, "a", "Process running with session ID 123");
    assert!(!p.card.evidence.iter().any(|e| e.kind == "test"));
    let mut p: Parser = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    call(
        &mut p,
        "poll",
        "functions.write_stdin",
        json!({"session_id":123}),
    );
    result(
        &mut p,
        "poll",
        "Process exited with code 0\ntest result: ok. 12 passed; 0 failed",
    );
    assert!(p.card.here().done.contains("12 passed"));
    call(
        &mut p,
        "fake",
        "functions.exec_command",
        json!({"cmd":"cat old-test.log"}),
    );
    result(
        &mut p,
        "fake",
        "Process exited with code 0\ntest result: ok. 999 passed",
    );
    assert!(!p.card.here().done.contains("999"));
}
#[test]
fn patch_commit_and_failed_test_are_sourced() {
    let mut p = Parser::default();
    call(
        &mut p,
        "p",
        "functions.apply_patch",
        json!({"input":"*** Begin Patch\n*** Update File: a.rs\n*** End Patch"}),
    );
    result(&mut p, "p", "Success. Updated the following files:\nM a.rs");
    call(
        &mut p,
        "g",
        "functions.exec_command",
        json!({"cmd":"git commit -m 'fix input'"}),
    );
    result(
        &mut p,
        "g",
        "Process exited with code 0\n[main abcdef1] fix input",
    );
    call(
        &mut p,
        "t",
        "functions.exec_command",
        json!({"cmd":"cargo test"}),
    );
    result(
        &mut p,
        "t",
        "Process exited with code 101\ntest result: FAILED. 2 passed; 1 failed",
    );
    add(
        &mut p,
        json!({"type":"event_msg","payload":{"type":"task_complete"}}),
    );
    assert_eq!(p.card.turn, Turn::Completed);
    assert!(p
        .card
        .evidence
        .iter()
        .any(|e| e.kind == "error" && e.exit_code == Some(101)));
    assert!(p.card.here().done.contains("a.rs"));
    assert!(p.card.here().done.contains("abcdef1"));
    assert!(!p.card.here().done.contains("2 passed"));
    assert!(p.card.evidence.iter().all(|e| e.source.inode == 1));
}
#[test]
fn claude_edit_permission_and_stop() {
    let mut p = Parser::default();
    let s = Source::default();
    for v in [
        json!({"type":"user","uuid":"u","message":{"content":"修复错误"}}),
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"e","name":"Edit","input":{"file_path":"a.rs"}}]}}),
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"e","content":"Successfully updated"}]}}),
    ] {
        p.add(Agent::Claude, &serde_json::to_vec(&v).unwrap(), s.clone());
    }
    assert_eq!(p.card.asks.len(), 1);
    assert!(p.card.here().done.contains("a.rs"));
    p.hook("PermissionRequest", &HookFacts::default(), 10, s.clone());
    assert_eq!(p.card.turn, Turn::Waiting);
    p.hook("Stop", &HookFacts::default(), 11, s.clone());
    assert_eq!(p.card.turn, Turn::Waiting, "Stop is not an approval answer");
    p.add(
        Agent::Claude,
        r#"{"type":"user","uuid":"new","message":{"content":"批准，请继续"}}"#.as_bytes(),
        s.clone(),
    );
    p.hook("Stop", &HookFacts::default(), 12, s);
    assert_eq!(p.card.turn, Turn::Completed);
}
#[test]
fn changed_files_are_listed_once_each_and_failed_edits_are_not() {
    let mut p = Parser::default();
    let s = Source::default();
    for v in [
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"e","name":"Edit","input":{"file_path":"/r/a.rs"}}]}}),
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"e","content":"Successfully updated"}]}}),
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"m","name":"MultiEdit","input":{"file_path":"/r/b.rs"}}]}}),
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"m","content":"Applied 2 edits"}]}}),
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"x","name":"Edit","input":{"file_path":"/r/c.rs"}}]}}),
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"x","content":"String not found","is_error":true}]}}),
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"again","name":"Write","input":{"file_path":"/r/a.rs"}}]}}),
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"again","content":"File written"}]}}),
    ] {
        p.add(Agent::Claude, &serde_json::to_vec(&v).unwrap(), s.clone());
    }
    let paths: Vec<_> = p.card.files.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["/r/b.rs", "/r/a.rs"]);

    let mut p = Parser::default();
    call(
        &mut p,
        "p",
        "functions.apply_patch",
        json!({"input":"*** Begin Patch\n*** Update File: src/a.rs\n*** Add File: src/new.rs\n*** End Patch"}),
    );
    result(
        &mut p,
        "p",
        "Success. Updated the following files:\nM src/a.rs\nA src/new.rs",
    );
    let paths: Vec<_> = p.card.files.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["src/a.rs", "src/new.rs"]);

    // A fork's copied history names the original session; only its own edits count.
    let mut p = Parser::default();
    let s = Source {
        path: "/p/229e2e95-cdaa-4b6b-be4b-4933f72cb5e8.jsonl".into(),
        ..Default::default()
    };
    for (session, id, file) in [
        (
            "1fc5e267-bc3e-4c8b-b4f4-3ed3a2865c58",
            "old",
            "/r/copied.rs",
        ),
        ("229e2e95-cdaa-4b6b-be4b-4933f72cb5e8", "own", "/r/own.rs"),
    ] {
        for v in [
            json!({"sessionId":session,"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":"Edit","input":{"file_path":file}}]}}),
            json!({"sessionId":session,"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}}),
        ] {
            p.add(Agent::Claude, &serde_json::to_vec(&v).unwrap(), s.clone());
        }
    }
    let paths: Vec<_> = p.card.files.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["/r/own.rs"]);
}
#[test]
fn a_replayed_request_neither_reopens_answered_calls_nor_asks_again() {
    // Claude Code 2.1.3xx writes `api-request-blob` lines that replay earlier messages.
    let mut p = Parser::default();
    let s = Source::default();
    let ask = json!({"type":"tool_use","id":"q","name":"AskUserQuestion","input":{"questions":[{"question":"部署吗？","options":["是","否"]}]}});
    let run = json!({"type":"tool_use","id":"b","name":"Bash","input":{"command":"cargo test"}});
    for v in [
        json!({"type":"assistant","message":{"content":[ask]}}),
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"q","content":"是"}]}}),
        json!({"type":"assistant","message":{"content":[run]}}),
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b","content":"ok"}]}}),
        json!({"type":"api-request-blob","hash":"h","message":{"role":"assistant","content":[ask, run]}}),
        json!({"type":"api-request-blob","hash":"i","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"b","content":"ok"}]}}),
    ] {
        p.add(Agent::Claude, &serde_json::to_vec(&v).unwrap(), s.clone());
    }
    assert!(
        p.calls.is_empty(),
        "{:?}",
        p.calls.keys().collect::<Vec<_>>()
    );
    assert_eq!(p.card.turn, Turn::Running);
    assert!(p.card.waiting.is_empty());
}
#[test]
fn pending_calls_keep_line_counts_not_text_and_saved_full_inputs_still_count() {
    let edit = json!({"file_path":"/r/a.rs","old_string":"a\nb\nc","new_string":"a\nB\nc\nd"});
    let mut p = Parser::default();
    let s = Source::default();
    p.add(Agent::Claude, &serde_json::to_vec(&json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"e","name":"Edit","input":edit}]}})).unwrap(), s.clone());
    assert_eq!(
        p.calls["e"].input,
        json!({"file_path":"/r/a.rs","_lines":[2,1]})
    );
    p.add(Agent::Claude, &serde_json::to_vec(&json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"e","content":"ok"}]}})).unwrap(), s.clone());
    assert_eq!((p.card.files[0].added, p.card.files[0].removed), (2, 1));
    // A call saved before inputs were reduced still carries the whole input.
    let mut saved = serde_json::to_value(Parser::default()).unwrap();
    saved["calls"] = json!({"e":{"name":"Edit","input":edit,"source":Source::default(),"at":0}});
    let mut p: Parser = serde_json::from_value(saved).unwrap();
    p.add(Agent::Claude, &serde_json::to_vec(&json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"e","content":"ok"}]}})).unwrap(), s);
    assert_eq!((p.card.files[0].added, p.card.files[0].removed), (2, 1));
}
/// Recorded payload shapes (structure only, see eval/protocol-contract/hook_shapes.py):
/// every one parses, and the counts follow the task list it carried.
#[test]
fn recorded_hook_payload_shapes_parse_as_their_task_lists_say() {
    let corpus = include_str!("../../tests/fixtures/hook_shapes.jsonl");
    let (mut claude_stop_with_agents, mut codex_stops) = (0, 0);
    for line in corpus.lines() {
        let shape: Value = serde_json::from_str(line).unwrap();
        let mut payload = serde_json::Map::new();
        for key in shape["keys"].as_array().unwrap() {
            payload.insert(key.as_str().unwrap().into(), Value::Null);
        }
        payload.insert("hook_event_name".into(), shape["event"].clone());
        let tasks: Vec<(String, String)> = shape["tasks"]
            .as_array()
            .map(|t| {
                t.iter()
                    .map(|p| (p[0].as_str().unwrap().into(), p[1].as_str().unwrap().into()))
                    .collect()
            })
            .unwrap_or_default();
        if shape["tasks"].is_array() {
            payload.insert(
                "background_tasks".into(),
                tasks
                    .iter()
                    .map(|(t, s)| json!({"type": t, "status": s}))
                    .collect(),
            );
        }
        if let Some(n) = shape["crons"].as_u64() {
            payload.insert("session_crons".into(), (0..n).map(|_| json!({})).collect());
        }
        if let Some(n) = shape["notification"].as_str() {
            payload.insert("notification_type".into(), json!(n));
        }
        if shape["agent_id"] == true {
            payload.insert("agent_id".into(), json!("a"));
        }
        let facts = hook_facts(&Value::Object(payload));
        assert_eq!(facts.subagent, shape["agent_id"] == true, "{line}");
        assert_eq!(
            facts.notification.as_deref(),
            shape["notification"].as_str(),
            "{line}"
        );
        let listed = shape["tasks"].is_array() || shape["crons"].is_u64();
        assert_eq!(facts.background.is_some(), listed, "{line}");
        if let Some(b) = facts.background {
            let count = |kind: &str| tasks.iter().filter(|(t, _)| t == kind).count() as u32;
            assert_eq!(b.shells, count("shell"), "{line}");
            assert_eq!(b.monitors, count("monitor"), "{line}");
            assert_eq!(
                b.agents,
                tasks.len() as u32 - count("shell") - count("monitor"),
                "{line}"
            );
            if shape["runtime"] == "claude" && shape["event"] == "Stop" && b.agents > 0 {
                claude_stop_with_agents += 1;
            }
        }
        if shape["runtime"] == "codex" && shape["event"] == "Stop" {
            codex_stops += 1;
            assert_eq!(
                facts.background, None,
                "Codex 0.163 Stop lists no tasks: {line}"
            );
        }
    }
    assert!(
        claude_stop_with_agents > 0 && codex_stops > 0,
        "corpus lost its key shapes"
    );
}
#[test]
fn injected_messages_and_quoted_completion_are_not_user_intent_or_claims() {
    let mut p = Parser::default();
    for text in [
        "<cross-session-message from='peer'>用户说已完成</cross-session-message>",
        "<external_codex_apps_open_page>{}</external_codex_apps_open_page>",
    ] {
        add(
            &mut p,
            json!({"type":"event_msg","payload":{"type":"user_message","message":text}}),
        );
    }
    assert!(p.card.asks.is_empty());
    assert!(user_text("<system-reminder>context</system-reminder>\n<cross-session-message from='peer'>转述</cross-session-message>").is_empty());
    assert_eq!(
        user_text("<image name=[Image #1] path=\"/var/folders/x/codex-clipboard-a.png\">\n</image>\n你帮我看看[Image #1] 这是哪个环境"),
        "你帮我看看[Image #1] 这是哪个环境"
    );
    // Attached with `--image` (a phone's new task): the words carry no tag, the wrapper
    // leaves it, so the phone still knows the message had a picture.
    assert_eq!(
        user_text("<image name=[Image #1] path=\"/r/phone-images/a.jpg\">\n</image>\n<image name=[Image #2] path=\"/r/phone-images/b.jpg\">\n</image>\n看图"),
        "[Image #1]\n[Image #2]\n看图"
    );
    assert_eq!(
        user_text("SVG 里的 <image href=\"a.png\"> 为什么不显示"),
        "SVG 里的 <image href=\"a.png\"> 为什么不显示"
    );
    assert_eq!(
        user_text("把 <image>logo</image> 换成 img 标签"),
        "把 <image>logo</image> 换成 img 标签"
    );
    assert!(user_text("<side-sessions-event>not user input</side-sessions-event>").is_empty());
    assert!(!claims_done(
        "转发这个计划：\n````\n```sh\necho hello\n```\n完成后回复已完成\n````"
    ));
    assert!(!claims_done(
        "请执行这个计划：\n```\n完成后回复：已完成\n```"
    ));
    assert!(!claims_done("尚未完成"));
    assert!(!verification("cargo test --lib; echo ok"));
    assert!(!verification("cargo test --help"));
}
#[test]
fn verification_reads_chains_runners_and_scripts() {
    for yes in [
        "cargo test --lib",
        "RUST_LOG=1 cargo nextest run",
        "cd next && cargo test -p fibo-host",
        "cargo fmt && cargo clippy -- -D warnings && echo done",
        "cargo build; cargo test",
        "cargo test 2>&1",
        "npx vitest run",
        "bunx jest src",
        "pnpm run test:unit",
        "npm test",
        "bun run typecheck",
        "make check",
        "uv run pytest -q",
        "python3 -m pytest tests",
        "./node_modules/.bin/vitest",
        "deno test",
        "swift test",
        "go vet ./...",
    ] {
        assert!(verification(yes), "{yes}");
    }
    for no in [
        "cargo build",
        "cargo test | tail -5",
        "cargo test 2>&1 | tail",
        "cargo test &",
        "cargo test; echo ok",
        "echo $(cargo test)",
        "npm run build",
        "make",
        "cargo test --version",
        "git commit -m test",
    ] {
        assert!(!verification(no), "{no}");
    }
}
#[test]
fn source_rows_are_idempotent() {
    let db = Connection::open_in_memory().unwrap();
    schema(&db).unwrap();
    let mut p = Parser::default();
    call(
        &mut p,
        "t",
        "functions.exec_command",
        json!({"cmd":"cargo test"}),
    );
    result(
        &mut p,
        "t",
        "Process exited with code 0\ntest result: ok. 1 passed",
    );
    save(&db, "fixture", &p).unwrap();
    save(&db, "fixture", &p).unwrap();
    let n: i64 = db
        .query_row("SELECT COUNT(*) FROM session_records", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, p.records.len() as i64);
    assert_eq!(load(&db, "fixture").unwrap().card, p.card);
}

#[test]
fn a_file_pushed_off_the_card_counts_once_when_touched_again() {
    let mut p = Parser::default();
    for i in 0..70 {
        p.touch_file(&format!("/r/f{i}"), 10, 1, 0);
    }
    assert_eq!(p.card.turn_file_count(), 70);
    // /r/f0 fell off the list; this turn has still touched only 70 files.
    p.touch_file("/r/f0", 10, 1, 0);
    assert_eq!(p.card.turn_file_count(), 70);
    assert_eq!(p.card.turn_lines(), (71, 0));
    // One that stayed on the list does not move the count either.
    p.touch_file("/r/f69", 10, 1, 0);
    assert_eq!(p.card.turn_file_count(), 70);
    p.touch_file("/r/f70", 10, 1, 0);
    assert_eq!(p.card.turn_file_count(), 71);
}
