//! How a tool result changes the card: evidence, receipts and the turn.
use super::*;

impl Parser {
    pub(crate) fn result(
        &mut self,
        id: &str,
        value: &Value,
        explicit: Option<i32>,
        implicit: bool,
        at: u64,
        source: &Source,
    ) {
        let object = input(value);
        let output = object
            .get("output")
            .map(text)
            .unwrap_or_else(|| text(value));
        let codes: Vec<i32> = EXIT
            .captures_iter(&output)
            .filter_map(|c| c[1].parse().ok())
            .collect();
        let structured = object["exit_code"]
            .as_i64()
            .or_else(|| object["exitCode"].as_i64())
            .or_else(|| object["metadata"]["exit_code"].as_i64())
            .map(|n| n as i32);
        let mut code = explicit.or(structured).or_else(|| {
            codes
                .iter()
                .find(|n| **n != 0)
                .or_else(|| codes.last())
                .copied()
        });
        if object["is_error"] == true {
            code = Some(1);
        }
        self.record("tool_result", short(&output, 8192), id, code, at, source);
        let Some(call) = self.calls.get(id).cloned() else {
            self.card.unknown_records += 1;
            return;
        };
        if call.name.ends_with("write_stdin") {
            let key = call.input["session_id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| call.input["session_id"].to_string());
            if let Some(original) = self.executions.get(&key).cloned() {
                self.result(&original, value, code, implicit, at, source);
            }
            self.calls.remove(id);
            return;
        }
        let pending = object
            .get("session_id")
            .filter(|v| !v.is_null())
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            })
            .or_else(|| SESSION.captures(&output).map(|c| c[1].into()));
        if code.is_none() {
            if let Some(sid) = pending {
                self.executions.insert(sid, id.into());
                return;
            }
        }
        let background = output.contains("running in the background")
            || object.get("backgroundTaskId").is_some();
        if implicit
            && code.is_none()
            && !background
            && matches!(
                call.name.as_str(),
                "Bash" | "Edit" | "MultiEdit" | "Write" | "NotebookEdit"
            )
        {
            code = Some(0);
        }
        let name = call.name.rsplit(['.', '_']).next().unwrap_or(&call.name);
        let patch = call.name.ends_with("apply_patch");
        if patch && code.is_none() && output.contains("Success. Updated the following files:") {
            code = Some(0);
        }
        if code.is_some_and(|n| n != 0)
            && (output.contains("requires approval")
                || output.contains("haven't granted")
                || output.contains("waiting for approval"))
        {
            self.child_wait_from = None;
            self.card.waiting = short(&output, 600);
            self.card.turn = Turn::Waiting;
            self.fact("approval", self.card.waiting.clone(), None, at, source);
            self.calls.remove(id);
            self.card.updated = self.card.updated.max(at);
            return;
        }
        // A failed command is evidence, not a failed turn: agents routinely run commands
        // that exit non-zero (no grep match, an interrupted call) and still answer.
        if code.is_some_and(|n| n != 0) {
            let failed = format!("{}: {}", call.name, short(&output, 240));
            self.fact("error", failed, code, at, source);
        }
        if code == Some(0)
            && (patch || matches!(name, "Edit" | "MultiEdit" | "Write" | "NotebookEdit"))
        {
            let mut paths = Vec::new();
            let (added, removed) = serde_json::from_value(call.input[LINES].clone())
                .unwrap_or_else(|_| edit_lines(&call.input));
            for key in ["file_path", "path", "notebook_path"] {
                if let Some(s) = call.input[key].as_str() {
                    paths.push((s.to_owned(), added, removed));
                }
            }
            paths.extend(
                serde_json::from_value(call.input[PATCHED].clone())
                    .unwrap_or_else(|_| patch_lines(patch_text(&call.input))),
            );
            for (path, added, removed) in paths {
                if !self.inherited {
                    self.touch_file(&path, at, added, removed);
                }
                self.fact("file", format!("文件修改：{path}"), code, at, source);
            }
        }
        if call.name == "Bash" || call.name.ends_with("exec_command") {
            if let Some(command) = call.input["cmd"]
                .as_str()
                .or_else(|| call.input["command"].as_str())
            {
                self.fact("command", short(command, 300), code, at, source);
                // Only receipts from literal test/commit commands qualify; cat/echo/git log never do.
                if verification(command) {
                    let lines: Vec<_> = TEST
                        .find_iter(&output)
                        .map(|m| m.as_str())
                        .take(6)
                        .collect();
                    if !lines.is_empty() && code.is_some() {
                        self.fact(
                            "test",
                            format!("{}：{}", short(command, 100), lines.join("；")),
                            if FAILED.is_match(&output) {
                                Some(1)
                            } else {
                                code
                            },
                            at,
                            source,
                        );
                    } else if code.is_some() {
                        self.fact(
                            "test",
                            format!(
                                "{}（exit {}，未捕获计数）",
                                short(command, 200),
                                code.unwrap()
                            ),
                            code,
                            at,
                            source,
                        );
                    }
                }
                if git_commit(command) && code == Some(0) {
                    for m in COMMIT.find_iter(&output) {
                        self.fact("commit", m.as_str().into(), code, at, source);
                    }
                }
            }
        }
        if self.card.turn == Turn::Waiting
            && (code.is_some()
                || call.name == "AskUserQuestion"
                || call.name.contains("request_user_input"))
        {
            self.card.turn = Turn::Running;
            self.card.waiting.clear();
            self.card.choices.clear();
        }
        self.calls.remove(id);
        self.executions.retain(|_, v| v != id);
        self.card.updated = self.card.updated.max(at);
    }
}

impl Parser {
    /// The card lists a limited number of files; this keeps its count of the rest true when one
    /// that fell off is touched again, without listing them all where the window would get them.
    pub(super) fn touch_file(&mut self, path: &str, at: u64, added: u32, removed: u32) {
        let hash = |p: &str| {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut h = DefaultHasher::new();
            p.hash(&mut h);
            h.finish()
        };
        if self.card.evicted.turn != self.card.turn_started {
            self.evicted_files.clear();
        }
        if self.evicted_files.remove(&hash(path)) {
            self.card.evicted.files = self.card.evicted.files.saturating_sub(1);
        }
        if let Some(gone) = self.card.touch(path, at, added, removed) {
            self.evicted_files.insert(hash(&gone));
        }
    }
}
