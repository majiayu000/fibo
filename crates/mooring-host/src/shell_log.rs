//! What a session's Claude Code logs say its background tasks run. A host that restarted
//! has lost the commands its last Stop hooks listed, and that hook will not run again while
//! the shell works. The logs keep them, so nothing new is stored.
use crate::background_shells::contains;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

/// The words Claude Code answers a started background task with, right before its id.
const STARTED: [&str; 2] = ["with ID: ", "Monitor started (task "];

/// `(task id, command)` for each of `ids` that this session started. A subagent's tasks are
/// listed with the session's but logged only in its own file, so those files are read for
/// whatever the session's own log does not have.
pub fn commands_in_session(log: &Path, ids: &[String]) -> io::Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    commands_in_log(log, ids, &mut out)?;
    for sub in subagent_logs(&log.with_extension("").join("subagents")) {
        if out.len() >= ids.len() {
            break;
        }
        // One unreadable subagent must not cost the others' commands.
        if let Err(e) = commands_in_log(&sub, ids, &mut out) {
            eprintln!("background shells: {}: {e}", sub.display());
        }
    }
    Ok(out)
}

/// Subagent logs under `dir`, at any depth: a workflow's are in their own folder.
fn subagent_logs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut logs = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            logs.extend(subagent_logs(&path));
        } else if path.extension().is_some_and(|x| x == "jsonl") {
            logs.push(path);
        }
    }
    logs
}

fn commands_in_log(log: &Path, ids: &[String], out: &mut Vec<(String, String)>) -> io::Result<()> {
    let mut reader = io::BufReader::new(std::fs::File::open(log)?);
    let mut commands: HashMap<String, String> = HashMap::new();
    let mut line = Vec::new();
    while io::BufRead::read_until(&mut reader, b'\n', &mut line)? > 0 {
        // Most lines are neither: don't parse them.
        let used =
            contains(&line, b"\"name\":\"Bash\"") || contains(&line, b"\"name\":\"Monitor\"");
        let started = STARTED.iter().any(|s| contains(&line, s.as_bytes()));
        if used || started {
            let value: serde_json::Value = serde_json::from_slice(&line).unwrap_or_default();
            let blocks = value.pointer("/message/content").and_then(|c| c.as_array());
            for block in blocks.into_iter().flatten() {
                if let (Some(id), Some(command)) = (
                    block["id"].as_str(),
                    block.pointer("/input/command").and_then(|c| c.as_str()),
                ) {
                    commands.insert(id.to_owned(), command.to_owned());
                }
                let Some(used_by) = block["tool_use_id"].as_str() else {
                    continue;
                };
                let text = block["content"].to_string();
                let task = ids.iter().find(|id| {
                    STARTED.iter().any(|s| {
                        text.split(s)
                            .skip(1)
                            .any(|rest| rest.starts_with(id.as_str()))
                    })
                });
                // The first mention is the start; later ones are output quoting the id.
                if let (Some(task), Some(command)) = (task, commands.get(used_by)) {
                    if !out.iter().any(|(id, _)| id == task) {
                        out.push((task.clone(), command.clone()));
                    }
                }
            }
        }
        line.clear();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kl-shells-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }
    fn pairs(names: &[(&str, &str)]) -> Vec<(String, String)> {
        names
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }
    /// One tool call and its answer, as Claude Code 2.1.319 writes them.
    fn call(tool: &str, use_id: &str, command: &str, answer: &str) -> String {
        let used = serde_json::json!({"type":"assistant","message":{"content":[
            {"type":"tool_use","id":use_id,"name":tool,"input":{"command":command}}]}});
        let got = serde_json::json!({"type":"user","message":{"content":[
            {"tool_use_id":use_id,"type":"tool_result","content":answer}]}});
        format!("{used}\n{got}\n")
    }

    #[test]
    fn a_log_gives_back_the_command_of_each_background_task() {
        // Shaped as Claude Code 2.1.318 writes them, including a shell moved to the
        // background by hand and a task another session started.
        let rows = [
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"sleep 1; echo 'one'","run_in_background":true}}]}}"#,
            r#"{"type":"user","message":{"content":[{"tool_use_id":"t1","type":"tool_result","content":"Command running in background with ID: aaa. Output is being written to: /x"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"cargo test"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"tool_use_id":"t2","type":"tool_result","content":[{"type":"text","text":"Command was manually backgrounded with ID: bbb"}]}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"not asked for"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"tool_use_id":"t3","type":"tool_result","content":"Command running in background with ID: ccc."}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"grep aaa log"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"tool_use_id":"t4","type":"tool_result","content":"log: running in background with ID: aaa."}]}}"#,
            "not json with ID: aaa",
        ];
        let dir = temp();
        let log = dir.join("s.jsonl");
        fs::write(&log, rows.join("\n")).unwrap();
        let got = commands_in_session(&log, &ids(&["aaa", "bbb"])).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            got,
            pairs(&[("aaa", "sleep 1; echo 'one'"), ("bbb", "cargo test")])
        );
    }

    /// Undoing the walk of `subagents/` fails this, and so does stopping at its first level
    /// (a workflow's agents) or forgetting that a Monitor answers in other words.
    #[test]
    fn a_task_a_subagent_started_is_found_in_its_own_log() {
        let dir = temp();
        let log = dir.join("s.jsonl");
        fs::write(
            &log,
            call(
                "Bash",
                "m1",
                "main job",
                "Command running in background with ID: main.",
            ),
        )
        .unwrap();
        let subs = dir.join("s/subagents");
        let nested = subs.join("workflows/wf_1");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            subs.join("agent-a.jsonl"),
            call(
                "Bash",
                "a1",
                "queue job",
                "Command running in background with ID: sub. Output",
            ) + &call(
                "Monitor",
                "a2",
                "until done; do sleep 15; done",
                "Monitor started (task mon, expires in 30m",
            ),
        )
        .unwrap();
        fs::write(
            nested.join("agent-b.jsonl"),
            call(
                "Bash",
                "b1",
                "workflow job",
                "Command running in background with ID: wf.",
            ),
        )
        .unwrap();
        // Not a log, even with the words in it.
        fs::write(subs.join("agent-a.meta.json"), "with ID: other").unwrap();
        let mut got =
            commands_in_session(&log, &ids(&["main", "sub", "mon", "wf", "other"])).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        got.sort();
        assert_eq!(
            got,
            pairs(&[
                ("main", "main job"),
                ("mon", "until done; do sleep 15; done"),
                ("sub", "queue job"),
                ("wf", "workflow job"),
            ])
        );
    }

    /// The session's own log is what a restart cannot do without; its subagents are extra.
    #[test]
    fn a_session_without_a_log_is_an_error_and_without_subagents_is_not() {
        let dir = temp();
        let log = dir.join("s.jsonl");
        assert!(commands_in_session(&log, &ids(&["a"])).is_err());
        fs::write(&log, "").unwrap();
        assert!(commands_in_session(&log, &ids(&["a"])).unwrap().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
