//! What a Codex rollout's finished items add to its other lines.
//!
//! Tools: a tool a script ran has no call line of its own, only its finished item, so these
//! become rows here: MCP calls, file changes, images viewed, web searches and generated
//! images. An item whose id is a `call_…` has its `function_call` row already. In the last
//! 30 real rollouts: McpToolCall 537 with a row, 29 without; FileChange 36, ImageView 48,
//! web search 161, image generation 42, none with a row.
//!
//! Messages: what a rollout says twice. One message is logged up to three times: the finished
//! item (`item_completed` AgentMessage), the `response_item` right after it, and for a
//! turn's last message `task_complete`'s `last_agent_message`. In the last 30 real rollouts
//! 294 came as item+response, 120 as all three, 19 as the item alone, 15 as item and
//! task_complete, 15 as the response alone. Whichever comes first is the record.
use super::tool_lines::{add_blocks, ToolCallLine, ToolOutput};
use crate::content::{claims_done, Parser};
use mooring_core::content::{short, Source};
use serde_json::{json, Value};
use std::hash::{DefaultHasher, Hash, Hasher};

fn mark(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:x}", h.finish())
}

/// A message as a finished item (or the `agent_message` event older logs write): recorded,
/// and the response item about to repeat it is expected.
pub(super) fn item_said(parser: &mut Parser, s: String, at: u64, source: &Source, last: bool) {
    parser.reply = mark(&s);
    parser.message(false, s, at, source, last);
}

/// A message as a response item: the one the item just recorded, or a new one.
pub(super) fn response_said(parser: &mut Parser, s: String, at: u64, source: &Source, last: bool) {
    if std::mem::take(&mut parser.reply) == mark(&s) {
        return;
    }
    parser.message(false, s, at, source, last);
}

/// `task_complete` names the turn's last message, normally already recorded. It still says
/// this message is the final answer.
pub(super) fn final_said(parser: &mut Parser, s: String, at: u64, source: &Source) {
    if short(&s, 1400) == parser.card.last_reply {
        parser.card.completed_claim = claims_done(&s);
        parser.card.updated = parser.card.updated.max(at);
        return;
    }
    parser.message(false, s, at, source, true);
}

/// A file change as the patch text everything else reads: the phone's diff, the files and
/// line counts of `apply_patch`. It says what the scripts' own text may not: a patch built
/// at run time is only known here.
fn patch_of(changes: &Value) -> String {
    let mut out = String::new();
    for (path, change) in changes.as_object().into_iter().flatten() {
        let body = change["content"].as_str().unwrap_or("");
        match change["type"].as_str().unwrap_or("") {
            "add" => {
                out += &format!("*** Add File: {path}\n");
                body.lines().for_each(|l| out += &format!("+{l}\n"));
            }
            "delete" => {
                out += &format!("*** Delete File: {path}\n");
                body.lines().for_each(|l| out += &format!("-{l}\n"));
            }
            _ => {
                out += &format!("*** Update File: {path}\n");
                let diff = change["unified_diff"].as_str().unwrap_or("");
                diff.lines()
                    .filter(|l| !l.starts_with("--- ") && !l.starts_with("+++ "))
                    .for_each(|l| out += &format!("{l}\n"));
            }
        }
    }
    out
}

/// The call a finished item stands for, as `describe_tool` reads calls. None for one with
/// its own call line, and for what is no call.
fn call_of(item: &Value) -> Option<ToolCallLine> {
    if item["id"].as_str().is_none_or(|id| id.starts_with("call_")) {
        return None;
    }
    let (name, args) = match item["type"].as_str()? {
        "McpToolCall" => (
            format!(
                "mcp__{}__{}",
                item["server"].as_str().unwrap_or(""),
                item["tool"].as_str().unwrap_or("")
            ),
            item["arguments"].clone(),
        ),
        "FileChange" => (
            "apply_patch".to_owned(),
            json!({ "input": patch_of(&item["changes"]) }),
        ),
        "ImageView" => ("view_image".to_owned(), json!({ "path": item["path"] })),
        "Extension" => match item["kind"].as_str()? {
            "web.search" => ("WebSearch".to_owned(), json!({ "query": item["query"] })),
            "image_gen.generation" => (
                "image_gen".to_owned(),
                json!({ "path": item["savedPath"], "description": item["revisedPrompt"] }),
            ),
            kind => (kind.to_owned(), json!({})),
        },
        _ => return None,
    };
    Some(ToolCallLine { name, args })
}

/// A call's row for a raw line, when asked for by `id`.
pub(super) fn call_line(item: &Value, id: &str) -> Option<ToolCallLine> {
    (item["id"] == id).then(|| call_of(item)).flatten()
}

/// What such an item came back with. An image is named, never carried.
pub(super) fn result_line(item: &Value) -> Option<ToolOutput> {
    call_of(item)?;
    let mut out = ToolOutput::default();
    let failed = matches!(item["status"].as_str(), Some("failed" | "declined"))
        || item["result"]["isError"] == true
        || !item["failure"].is_null();
    out.error = failed;
    out.code = Some(i32::from(failed));
    let secs = item["duration"]["secs"].as_u64();
    let nanos = item["duration"]["nanos"].as_u64().unwrap_or(0);
    out.took_ms = secs.map(|s| s * 1000 + nanos / 1_000_000);
    match item["type"].as_str()? {
        "McpToolCall" => {
            if let Some(blocks) = item["result"]["content"].as_array() {
                add_blocks(&mut out, blocks);
            }
        }
        "FileChange" => {
            let said = [&item["stdout"], &item["stderr"]];
            let said: Vec<&str> = said.iter().filter_map(|v| v.as_str()).collect();
            out.text = said.join("\n").trim().to_owned();
        }
        "ImageView" => out.images = 1,
        "Extension" if item["kind"] == "image_gen.generation" => out.images = 1,
        _ => (),
    }
    Some(out)
}

/// Takes a finished item this file knows; false for any other.
pub(super) fn add_item(parser: &mut Parser, item: &Value, at: u64, source: &Source) -> bool {
    match item["type"].as_str().unwrap_or("") {
        "Reasoning" | "UserMessage" | "ContextCompaction" => return true,
        "McpToolCall" | "FileChange" | "ImageView" | "Extension" => (),
        _ => return false,
    }
    if let (Some(call), Some(out)) = (call_of(item), result_line(item)) {
        let id = item["id"].as_str().unwrap_or("");
        parser.tool(id, &call.name, call.args, at, source);
        parser.result(id, &json!(out.text), out.code, false, at, source);
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::content::Parser;
    use mooring_core::{content::Source, model::Agent};
    use serde_json::{json, Value};

    fn feed(p: &mut Parser, rows: &[Value]) {
        for (i, row) in rows.iter().enumerate() {
            let source = Source {
                offset: i as u64,
                ..Default::default()
            };
            p.add(Agent::Codex, row.to_string().as_bytes(), source);
        }
    }
    fn item(text: &str) -> Value {
        json!({"type":"event_msg","timestamp":"2026-11-05T00:00:00Z","payload":{"type":"item_completed","item":{"type":"AgentMessage","id":"m","phase":"final_answer","content":[{"type":"text","text":text}]}}})
    }
    fn response(text: &str) -> Value {
        json!({"type":"response_item","timestamp":"2026-11-05T00:00:01Z","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}})
    }
    fn done(text: &str) -> Value {
        json!({"type":"event_msg","timestamp":"2026-11-05T00:00:05Z","payload":{"type":"task_complete","turn_id":"t","last_agent_message":text}})
    }
    fn said_in(p: &Parser) -> Vec<&str> {
        (p.records.iter())
            .filter(|r| r.kind == "assistant")
            .map(|r| r.text.as_str())
            .collect()
    }
    use said_in as said;

    /// Every shape the last 30 real rollouts show gives one record per message.
    #[test]
    fn one_message_is_one_record_however_many_times_it_is_logged() {
        let a = "改好了";
        for rows in [
            vec![item(a), response(a)],
            vec![item(a), response(a), done(a)],
            vec![item(a)],
            vec![item(a), done(a)],
            vec![response(a)],
            vec![response(a), done(a)],
        ] {
            let mut p = Parser::default();
            feed(&mut p, &rows);
            assert_eq!(said(&p), [a], "{rows:?}");
            assert_eq!(p.card.last_reply, a);
        }
    }

    /// A sentence said again later is said twice; only the same one logged twice is one.
    #[test]
    fn the_same_words_said_again_are_a_new_message() {
        let mut p = Parser::default();
        let call = json!({"type":"response_item","timestamp":"2026-11-05T00:00:02Z","payload":{"type":"function_call","call_id":"c","name":"exec_command","arguments":"{}"}});
        feed(
            &mut p,
            &[
                item("继续"),
                response("继续"),
                call,
                item("继续"),
                response("继续"),
                done("继续"),
            ],
        );
        assert_eq!(said(&p), ["继续", "继续"]);
    }

    fn finished(item: Value) -> Value {
        json!({"type":"event_msg","timestamp":"2026-11-05T00:00:03Z","payload":{"type":"item_completed","item":item}})
    }
    fn rows(p: &Parser) -> Vec<String> {
        (p.records.iter())
            .filter(|r| r.kind == "tool_call")
            .map(|r| r.text.split(' ').next().unwrap_or("").to_owned())
            .collect()
    }

    /// Shapes from real rollouts: a row each, with what came back. A call with its own
    /// `function_call` row gets no second one.
    #[test]
    fn finished_items_become_tool_rows() {
        let mut p = Parser::default();
        feed(
            &mut p,
            &[
                finished(
                    json!({"type":"McpToolCall","id":"ex1","server":"crawlume","tool":"fetch","arguments":{"url":"https://e.com"},"status":"completed","duration":{"secs":2,"nanos":0},"result":{"content":[{"type":"text","text":"页面"}],"isError":false}}),
                ),
                finished(
                    json!({"type":"McpToolCall","id":"ex2","server":"s","tool":"t","arguments":{},"status":"failed","result":{"content":[{"type":"text","text":"不行"}],"isError":true}}),
                ),
                finished(
                    json!({"type":"McpToolCall","id":"call_abc","server":"s","tool":"t","arguments":{},"status":"completed","result":{"content":[]}}),
                ),
                finished(json!({"type":"ImageView","id":"ex3","path":"/r/a.png"})),
                finished(
                    json!({"type":"Extension","id":"ex4","kind":"web.search","query":"天气","action":{},"results":[]}),
                ),
                finished(
                    json!({"type":"Extension","id":"ex5","kind":"image_gen.generation","status":"completed","savedPath":"/r/b.png","revisedPrompt":"一只猫","result":"QUJD"}),
                ),
                finished(
                    json!({"type":"Extension","id":"call_9","kind":"clock.sleep","durationMs":10}),
                ),
            ],
        );
        assert_eq!(
            rows(&p),
            [
                "mcp__crawlume__fetch",
                "mcp__s__t",
                "view_image",
                "WebSearch",
                "image_gen"
            ]
        );
        let code = |id: &str| {
            let r = p
                .records
                .iter()
                .find(|r| r.kind == "tool_result" && r.call_id == id);
            r.and_then(|r| r.code)
        };
        assert_eq!(
            (code("ex1"), code("ex2"), code("ex3")),
            (Some(0), Some(1), Some(0))
        );
        assert_eq!(p.card.unknown_records, 0);
        let image = json!({"type":"ImageView","id":"ex3","path":"/r/a.png"});
        let out = super::result_line(&image).unwrap();
        assert_eq!(
            (out.images, out.text.as_str()),
            (1, ""),
            "a placeholder only"
        );
        let gen =
            json!({"type":"Extension","id":"ex5","kind":"image_gen.generation","result":"QUJD"});
        assert_eq!(
            super::result_line(&gen).unwrap().text,
            "",
            "never its bytes"
        );
    }

    /// The desktop's 「改了哪些文件」 and the phone's list both come from one row.
    #[test]
    fn a_file_change_says_which_files_and_how_many_lines() {
        let change = finished(
            json!({"type":"FileChange","id":"ex7","status":"completed","stdout":"","stderr":"","changes":{
                "/r/new.md":{"type":"add","content":"a\nb\n"},
                "/r/old.rs":{"type":"update","move_path":null,"unified_diff":"--- a/old.rs\n+++ b/old.rs\n@@ -1 +1,2 @@\n-x\n+y\n+z\n"}
            }}),
        );
        let mut p = Parser::default();
        feed(&mut p, std::slice::from_ref(&change));
        assert_eq!(rows(&p), ["apply_patch"]);
        let touched: Vec<_> = (p.card.files.iter())
            .map(|f| (f.path.as_str(), f.added, f.removed))
            .collect();
        assert_eq!(touched, [("/r/new.md", 2, 0), ("/r/old.rs", 2, 1)]);
        let failed = finished(
            json!({"type":"FileChange","id":"ex8","status":"failed","stderr":"冲突","changes":{"/r/x":{"type":"add","content":"a"}}}),
        );
        let mut q = Parser::default();
        feed(&mut q, &[failed]);
        assert!(q.card.files.is_empty(), "a failed change changed nothing");
        let call = super::call_line(&change["payload"]["item"], "ex7").unwrap();
        let d = crate::agents::describe_tool(&call.name, &call.args);
        assert_eq!((d.kind, d.input_kind), ("edit", "diff"));
        assert_eq!(d.label, "/r/new.md、/r/old.rs");
    }

    /// Counts only, over the newest 30 real rollouts:
    /// `cargo test -p mooring-host --lib real_rollout_counts -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads real logs; prints numbers only"]
    fn real_rollout_counts() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "jsonl") {
                    out.push(p);
                }
            }
        }
        let root = std::path::Path::new(&std::env::var("HOME").unwrap()).join(".codex/sessions");
        let mut files = vec![];
        walk(&root, &mut files);
        let at = |p: &std::path::PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        files.sort_by_key(at);
        let files = &files[files.len().saturating_sub(30)..];
        let (mut said, mut distinct) = (0, 0);
        let mut calls = std::collections::BTreeMap::<String, usize>::new();
        let mut logged = std::collections::BTreeMap::<String, (usize, usize)>::new();
        for f in files {
            let mut p = Parser::default();
            for (i, line) in std::fs::read(f).unwrap().split(|b| *b == b'\n').enumerate() {
                let source = Source {
                    offset: i as u64,
                    ..Default::default()
                };
                p.add(Agent::Codex, line, source);
                if let Ok(v) = serde_json::from_slice::<Value>(line) {
                    let item = &v["payload"]["item"];
                    let kind = item["kind"].as_str().or(item["type"].as_str());
                    if v["payload"]["type"] == "item_completed" {
                        let e = logged.entry(kind.unwrap_or("").to_owned()).or_default();
                        e.0 += 1;
                        let id = item["id"].as_str().unwrap_or("");
                        e.1 += usize::from(
                            p.records
                                .iter()
                                .any(|r| r.kind == "tool_call" && r.call_id == id),
                        );
                    }
                }
            }
            let words = said_in(&p);
            said += words.len();
            distinct += words.iter().collect::<std::collections::HashSet<_>>().len();
            for r in p.records.iter().filter(|r| r.kind == "tool_call") {
                let name = r.text.split(' ').next().unwrap_or("");
                let name = if name.starts_with("mcp__") {
                    "mcp__*"
                } else {
                    name
                };
                *calls.entry(name.to_owned()).or_default() += 1;
            }
        }
        println!(
            "{} rollouts: {said} agent messages, {distinct} distinct per rollout",
            files.len()
        );
        println!("tool rows: {calls:?}");
        println!("finished items (logged, with a row): {logged:?}");
    }

    /// `task_complete` with no message to repeat still counts, and its claim still reads.
    #[test]
    fn the_final_message_still_counts_when_only_task_complete_has_it() {
        let mut p = Parser::default();
        feed(&mut p, &[response("开始了"), done("全部完成，测试通过")]);
        assert_eq!(said(&p), ["开始了", "全部完成，测试通过"]);
        assert_eq!(p.card.last_reply, "全部完成，测试通过");
        let mut q = Parser::default();
        feed(
            &mut q,
            &[item("全部完成，测试通过"), done("全部完成，测试通过")],
        );
        assert!(q.card.completed_claim);
    }
}
