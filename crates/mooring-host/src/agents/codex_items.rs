//! What a Codex rollout says twice. One message is logged up to three times: the finished
//! item (`item_completed` AgentMessage), the `response_item` right after it, and for a
//! turn's last message `task_complete`'s `last_agent_message`. In the last 30 real rollouts
//! 294 came as item+response, 120 as all three, 19 as the item alone, 15 as item and
//! task_complete, 15 as the response alone. Whichever comes first is the record.
use crate::content::{claims_done, Parser};
use mooring_core::content::{short, Source};
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
        for f in files {
            let mut p = Parser::default();
            for (i, line) in std::fs::read(f).unwrap().split(|b| *b == b'\n').enumerate() {
                let source = Source {
                    offset: i as u64,
                    ..Default::default()
                };
                p.add(Agent::Codex, line, source);
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
