//! A conversation as chat for a phone (`Request::Chat`): its user and Agent messages, and
//! with `tools` what the Agent did before each of its messages.
use super::{err, Index};
use crate::agents::codex::script_patches;
use crate::content::{edit_lines, patch_lines, patch_text, LINES, PATCHED};
use mooring_core::protocol::{ChatMessage, ChatTools};
use serde_json::Value;
use std::collections::HashSet;
use std::io;

impl Index {
    /// A conversation's user and Agent messages, oldest first, and whether older ones
    /// exist. See `Request::Chat`.
    pub fn chat(
        &self,
        path: &str,
        after: Option<i64>,
        before: Option<i64>,
        limit: usize,
        tools: bool,
    ) -> io::Result<(Vec<ChatMessage>, bool)> {
        let limit = limit.clamp(1, 500) as i64;
        let kinds = "kind IN ('user','assistant')";
        let (sql, cursor) = match (after, before) {
            (Some(a), _) => (format!("SELECT rowid,at,kind,text FROM session_records WHERE path=?1 AND {kinds} AND rowid>?2 ORDER BY rowid LIMIT ?3"), a),
            (None, Some(b)) => (format!("SELECT rowid,at,kind,text FROM session_records WHERE path=?1 AND {kinds} AND rowid<?2 ORDER BY rowid DESC LIMIT ?3"), b),
            (None, None) => (format!("SELECT rowid,at,kind,text FROM session_records WHERE path=?1 AND {kinds} AND rowid<?2 ORDER BY rowid DESC LIMIT ?3"), i64::MAX),
        };
        let mut q = self.db.prepare_cached(&sql).map_err(err)?;
        let mut messages = q
            .query_map(rusqlite::params![path, cursor, limit + 1], |r| {
                Ok(ChatMessage {
                    seq: r.get(0)?,
                    at: u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                    user: r.get::<_, String>(2)? == "user",
                    text: r.get(3)?,
                    tools: None,
                })
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        let more = messages.len() as i64 > limit;
        messages.truncate(limit as usize);
        if after.is_none() {
            messages.reverse();
        }
        if tools {
            self.attach_tools(path, &mut messages)?;
        }
        // Newer ones beyond the limit are fetched again with `after`; `more` speaks of older.
        Ok((messages, more && after.is_none()))
    }

    /// Each Agent message's tool calls: those logged after the message before it, user's or
    /// Agent's, so a turn's calls are counted once, on the message they led to. One read of
    /// the page's whole range: this runs under the host's lock.
    fn attach_tools(&self, path: &str, messages: &mut [ChatMessage]) -> io::Result<()> {
        let (Some(first), Some(last)) = (messages.first(), messages.last()) else {
            return Ok(());
        };
        let (first, last) = (first.seq, last.seq);
        let from: i64 = self
            .db
            .prepare_cached("SELECT COALESCE(MAX(rowid),0) FROM session_records WHERE path=?1 AND kind IN ('user','assistant') AND rowid<?2")
            .map_err(err)?
            .query_row(rusqlite::params![path, first], |r| r.get(0))
            .map_err(err)?;
        // A result comes after its call, possibly past this page.
        let failed = self
            .db
            .prepare_cached("SELECT call_id FROM session_records WHERE path=?1 AND kind='tool_result' AND exit_code<>0 AND rowid>?2")
            .map_err(err)?
            .query_map(rusqlite::params![path, from], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(err)?;
        let calls = self
            .db
            .prepare_cached("SELECT rowid,call_id,text FROM session_records WHERE path=?1 AND kind='tool_call' AND rowid>?2 AND rowid<?3 ORDER BY rowid")
            .map_err(err)?
            .query_map(rusqlite::params![path, from, last], |r| {
                let id: String = r.get(1)?;
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(2)?, failed.contains(&id)))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        let mut calls = calls.into_iter().peekable();
        for m in messages.iter_mut() {
            let mut before = Vec::new();
            while let Some((_, text, failed)) = calls.next_if(|c| c.0 < m.seq) {
                before.push((text, failed));
            }
            if !m.user {
                m.tools = Some(summary(&before));
            }
        }
        Ok(())
    }
}

/// Commands run and files changed by `tool_call` records (`Name {arguments}`), each with
/// whether it failed: a failed edit changed nothing, a failed command still ran.
fn summary(calls: &[(String, bool)]) -> ChatTools {
    let mut out = ChatTools::default();
    for (call, failed) in calls {
        let (name, args) = call.split_once(' ').unwrap_or((call, ""));
        let args: Value = serde_json::from_str(args).unwrap_or(Value::Null);
        let short = name.rsplit(['.', ':']).next().unwrap_or(name);
        if matches!(short, "Bash" | "shell" | "local_shell") || short.ends_with("exec_command") {
            out.commands += 1;
        }
        if *failed {
            continue;
        }
        let mut touched = Vec::new();
        if short == "exec" {
            // A Codex script: whole, or when long its files as counted at record time.
            touched = serde_json::from_value(args[PATCHED].clone()).unwrap_or_default();
            if let Some(script) = args["input"].as_str() {
                touched = script_patches(script);
            }
        } else if matches!(short, "Edit" | "MultiEdit" | "Write" | "NotebookEdit") {
            let (added, removed) =
                serde_json::from_value(args[LINES].clone()).unwrap_or_else(|_| edit_lines(&args));
            for key in ["file_path", "notebook_path", "path"] {
                if let Some(path) = args[key].as_str() {
                    touched.push((path.to_owned(), added, removed));
                    break;
                }
            }
        } else if short.ends_with("apply_patch") {
            touched = serde_json::from_value(args[PATCHED].clone())
                .unwrap_or_else(|_| patch_lines(patch_text(&args)));
        }
        for (path, added, removed) in touched {
            match out.files.iter_mut().find(|f| f.0 == path) {
                Some(f) => {
                    f.1 += added;
                    f.2 += removed;
                }
                None => out.files.push((path, added, removed)),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_summary_counts_commands_and_sums_lines_per_file() {
        let call = |name: &str, args: Value| format!("{name} {args}");
        let patch =
            "*** Begin Patch\n*** Update File: src/b.rs\n@@\n-old\n+new\n+more\n*** End Patch";
        let calls = [
            call("Bash", json!({"command": "cargo test"})),
            call(
                "Edit",
                json!({"file_path": "/r/a.rs", "old_string": "x", "new_string": "y\nz"}),
            ),
            call(
                "Edit",
                json!({"file_path": "/r/a.rs", "old_string": "q", "new_string": ""}),
            ),
            call("Read", json!({"file_path": "/r/c.rs"})),
            call("exec_command", json!({"cmd": "ls"})),
            call("apply_patch", json!({"input": patch})),
            // A long Write as `logged_args` keeps it: its counted lines, not its text.
            call("Write", json!({"file_path": "/r/big.txt", LINES: [300, 0]})),
            "Bash {\"command\":\"cut off".to_owned(),
        ];
        let mut calls: Vec<_> = calls.into_iter().map(|c| (c, false)).collect();
        // A failed edit changed nothing; a failed command still ran.
        calls.push((
            call(
                "Edit",
                json!({"file_path": "/r/failed.rs", "new_string": "y"}),
            ),
            true,
        ));
        calls.push((call("Bash", json!({"command": "false"})), true));
        let s = summary(&calls);
        assert_eq!(s.commands, 4);
        assert_eq!(
            s.files,
            [
                ("/r/a.rs".to_owned(), 2, 2),
                ("src/b.rs".to_owned(), 2, 1),
                ("/r/big.txt".to_owned(), 300, 0),
            ]
        );
    }

    /// Codex's `exec` scripts, as real logs have them, short and past the 8 KiB record. Each
    /// command inside has a `CommandExecution` of its own, so the script itself counts none.
    #[test]
    fn a_codex_script_says_what_it_patched_and_its_commands_count_once() {
        use crate::content::Parser;
        use mooring_core::{content::Source, model::Agent};
        let patch = "*** Begin Patch\\n*** Add File: /r/new.md\\n+tools.exec_command(not a call)\\n+\\\"two\\\"\\n*** Update File: /r/old.rs\\n@@\\n-a\\n+b\\n*** End Patch";
        let script = format!("text(await tools.exec_command({{cmd:\"ls\"}}));\ntext(await tools.apply_patch(\"{patch}\"));\ntext(await tools.exec_command({{cmd:\"pwd\"}}));\n");
        let long = format!("{script}// {}", "x".repeat(9000));
        let dynamic = "const p = make(); await tools.apply_patch(p);";
        let mut p = Parser::default();
        for (id, s) in [("1", script.as_str()), ("2", long.as_str()), ("3", dynamic)] {
            let line = json!({"type":"response_item","timestamp":"2026-11-04T00:00:00Z","payload":{"type":"custom_tool_call","call_id":id,"name":"exec","input":s}});
            p.add(Agent::Codex, line.to_string().as_bytes(), Source::default());
        }
        let done = json!({"type":"event_msg","timestamp":"2026-11-04T00:00:00Z","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"exec-1","command":["/bin/zsh","-lc","ls"],"exit_code":0}}});
        p.add(Agent::Codex, done.to_string().as_bytes(), Source::default());
        let calls: Vec<_> = (p.records.iter())
            .filter(|r| r.kind == "tool_call")
            .map(|r| (r.text.clone(), false))
            .collect();
        assert!(calls[1].0.len() < 1000, "kept short: {}", calls[1].0.len());
        for call in &calls[..2] {
            let s = summary(std::slice::from_ref(call));
            assert_eq!(s.commands, 0, "{}", call.0);
            assert_eq!(
                s.files,
                [
                    ("/r/new.md".to_owned(), 2, 0),
                    ("/r/old.rs".to_owned(), 1, 1)
                ]
            );
        }
        // Built at run time: nothing to read, nothing made up.
        assert_eq!(summary(&calls[2..3]), ChatTools::default());
        // The command the script ran, as Codex logs it once it is done.
        assert_eq!(summary(&calls[3..]).commands, 1);
    }

    /// What the content parser records for real Codex and Claude lines reads back: the
    /// tool names as each CLI logs them, and a Write too long to keep whole.
    #[test]
    fn records_from_both_clis_summarize() {
        use crate::content::Parser;
        use mooring_core::{content::Source, model::Agent};
        let mut p = Parser::default();
        let codex = |p: &mut Parser, v: Value| {
            let line =
                json!({"type":"response_item","timestamp":"2026-11-04T00:00:00Z","payload":v});
            p.add(Agent::Codex, line.to_string().as_bytes(), Source::default());
        };
        let args = json!({"cmd": "cargo test"}).to_string();
        codex(
            &mut p,
            json!({"type":"function_call","call_id":"c1","name":"exec_command","arguments":args}),
        );
        let patch = "*** Begin Patch\n*** Add File: new.rs\n+fn a() {}\n*** End Patch";
        codex(
            &mut p,
            json!({"type":"custom_tool_call","call_id":"c2","name":"apply_patch","input":patch}),
        );
        let body = "line\n".repeat(5000);
        let write = json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"w","name":"Write","input":{"file_path":"/r/long.txt","content":body}}]}});
        p.add(
            Agent::Claude,
            write.to_string().as_bytes(),
            Source::default(),
        );
        let calls: Vec<(String, bool)> = (p.records.iter())
            .filter(|r| r.kind == "tool_call")
            .map(|r| (r.text.clone(), false))
            .collect();
        assert_eq!(calls.len(), 3, "{calls:?}");
        assert!(
            calls[2].0.len() < 1000,
            "a long call is kept short: {}",
            calls[2].0.len()
        );
        let s = summary(&calls);
        assert_eq!(s.commands, 1);
        assert_eq!(
            s.files,
            [
                ("new.rs".to_owned(), 1, 0),
                ("/r/long.txt".to_owned(), 5000, 0)
            ]
        );
    }
}
