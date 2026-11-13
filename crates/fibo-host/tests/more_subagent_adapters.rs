//! Parent-owned Grok metadata and Qoder sidechain logs share the existing projection.
use fibo_core::{
    content::{Content, Turn},
    model::Agent,
};
use fibo_host::{agent_adapter, log_index::Index};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const PARENT: &str = "c19a5748-897d-4836-a555-fce8a7797222";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("fibo-more-children-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn append(path: &Path, record: Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap(),
        "{record}"
    )
    .unwrap();
}
fn card(index: &Index, path: &Path) -> Content {
    index.card(path.to_str().unwrap()).unwrap().unwrap()
}
fn stamp(at: u64) -> String {
    chrono::DateTime::from_timestamp(at as i64, 0)
        .unwrap()
        .to_rfc3339()
}
fn qoder_record(at: u64, child: bool, role: &str, content: Value) -> Value {
    json!({"type":role,"sessionId":PARENT,"cwd":"/repo","timestamp":stamp(at),
        "entrypoint":"cli","userType":"external","isSidechain":child,
        "message":{"role":role,"content":content,"stop_reason":if role=="assistant" {json!("end_turn")} else {Value::Null}}})
}
#[test]
fn qoder_children_are_discovered_and_finish_from_their_own_logs() {
    let f = Fixture::new();
    let now = fibo_core::content::now();
    let root = f.0.join(".qoder/projects");
    let parent = root.join(format!("repo/{PARENT}.jsonl"));
    let child = root.join(format!("repo/{PARENT}/subagents/agent-review.jsonl"));
    append(
        &parent,
        qoder_record(now - 100, false, "user", json!("Review project")),
    );
    append(
        &child,
        qoder_record(now - 80, true, "user", json!("Review parser")),
    );
    let meta = child.with_extension("meta.json");
    fs::write(
        &meta,
        json!({"description":"Review parser","agentType":"general-purpose","toolUseId":"call-1"})
            .to_string(),
    )
    .unwrap();
    let mut index = Index::open(&f.0, vec![(Agent::Qoder, root.clone())]).unwrap();
    index.initialize().unwrap();
    index.bootstrap().unwrap();
    let main = card(&index, &parent);
    assert_eq!(
        main.subagents.len(),
        1,
        "initial scan descends into child logs"
    );
    assert_eq!(main.subagents[0].id, "review");
    assert_eq!(main.subagents[0].name, "Review parser");
    assert!(main.subagents[0].running());
    let sub = card(&index, &child);
    assert_eq!(sub.parent, PARENT);
    assert_eq!(sub.turn, Turn::Running);
    assert!(!sub.interactive && !sub.hidden_reason.is_empty());
    let identity = agent_adapter(Agent::Qoder)
        .identity(&child, &qoder_record(now, true, "user", json!("ask")))
        .unwrap();
    assert_eq!(
        identity.0, "review",
        "child identity is not the parent sessionId"
    );
    // Qoder's unchanged meta has no finished flag. Only its log establishes completion.
    append(
        &child,
        qoder_record(
            now - 10,
            true,
            "assistant",
            json!([{"type":"text","text":"Done"}]),
        ),
    );
    let changed = index.update_many([child.clone()], 1).unwrap();
    assert!(changed.iter().any(|t| t.path == parent.to_string_lossy()));
    let main = card(&index, &parent);
    assert_eq!(main.subagents[0].ended, Some(now - 10));
    assert!(!main.subagents[0].stopped);
    assert_eq!(card(&index, &child).turn, Turn::Completed);
    assert!(fs::read_to_string(&meta)
        .unwrap()
        .find("finished")
        .is_none());
    // A resumed child can ask the user; its answer/end updates the same parent.
    append(
        &child,
        qoder_record(now - 8, true, "user", json!("Continue")),
    );
    let mut ask = qoder_record(
        now - 6,
        true,
        "assistant",
        json!([{"type":"tool_use","id":"ask-1","name":"AskUserQuestion","input":{"questions":[{"question":"Proceed?"}]}}]),
    );
    ask["message"]["stop_reason"] = json!("tool_use");
    append(&child, ask);
    index.update_many([child.clone()], 2).unwrap();
    assert_eq!(card(&index, &parent).turn, Turn::Waiting);
    append(
        &child,
        qoder_record(
            now - 4,
            true,
            "user",
            json!([{"type":"tool_result","tool_use_id":"ask-1","content":"Yes"}]),
        ),
    );
    append(
        &child,
        qoder_record(now - 2, true, "assistant", json!("Done")),
    );
    index.update_many([child.clone()], 3).unwrap();
    assert_ne!(card(&index, &parent).turn, Turn::Waiting);
    // Long, unfinished children still recover their start from before the tail window.
    let long = root.join(format!("repo/{PARENT}/subagents/agent-long.jsonl"));
    append(
        &long,
        qoder_record(now - 70, true, "user", json!("Long review")),
    );
    for _ in 0..180 {
        let mut r = qoder_record(now - 60, true, "assistant", json!("x".repeat(2000)));
        r["message"]["stop_reason"] = Value::Null;
        append(&long, r);
    }
    index.update_many([long.clone()], 4).unwrap();
    assert_eq!(card(&index, &long).turn, Turn::Running);
    assert_eq!(card(&index, &long).turn_started, now - 70);
    index.flush().unwrap();
    drop(index);
    let index = Index::open(&f.0, vec![(Agent::Qoder, root.clone())]).unwrap();
    assert_eq!(card(&index, &parent).subagents.len(), 2);
    assert!(!agent_adapter(Agent::Qoder).descend(&root.join("repo/transcript")));
    assert!(!agent_adapter(Agent::Qoder)
        .is_transcript(&child.with_file_name("task-1.session.execution.jsonl")));
}
fn grok_record(at: u64, update: Value) -> Value {
    json!({"timestamp":at,"method":"session/update","params":{"sessionId":PARENT,"update":update}})
}
fn grok_meta(parent: &Path, id: &str, started: u64, ended: Option<u64>, status: &str) -> PathBuf {
    let file = parent
        .parent()
        .unwrap()
        .join(format!("subagents/{id}/meta.json"));
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(
        &file,
        json!({"parent_session_id":PARENT,"subagent_id":id,"child_session_id":id,
        "description":format!("Task {id}"),"subagent_type":"explore","status":status,
        "started_at":stamp(started),"completed_at":ended.map(stamp)})
        .to_string(),
    )
    .unwrap();
    file
}
#[test]
fn grok_metadata_refreshes_only_its_parent_and_preserves_task_outcomes() {
    let f = Fixture::new();
    let now = fibo_core::content::now();
    let root = f.0.join(".grok/sessions");
    let parent = root.join(format!("%2Frepo/{PARENT}/updates.jsonl"));
    append(
        &parent,
        grok_record(
            now - 100,
            json!({"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"Review project"}}),
        ),
    );
    fs::write(parent.with_file_name("summary.json"), "{}").unwrap();
    let running = grok_meta(&parent, "running", now - 90, None, "running");
    grok_meta(&parent, "done", now - 80, Some(now - 40), "completed");
    grok_meta(&parent, "cancelled", now - 70, Some(now - 30), "cancelled");
    grok_meta(&parent, "error", now - 60, Some(now - 20), "failed");
    let wrong = grok_meta(&parent, "wrong-parent", now - 50, None, "running");
    let mut v: Value = serde_json::from_slice(&fs::read(&wrong).unwrap()).unwrap();
    v["parent_session_id"] = json!("elsewhere");
    fs::write(&wrong, v.to_string()).unwrap();
    let mut index = Index::open(&f.0, vec![(Agent::Grok, root.clone())]).unwrap();
    index.initialize().unwrap();
    index.bootstrap().unwrap();
    let main = card(&index, &parent);
    assert_eq!(main.subagents.len(), 4);
    assert!(main.subagents[0].running());
    assert!(!main.subagents[1].stopped && !main.subagents[1].running());
    assert!(main.subagents[2].stopped && main.subagents[3].stopped);
    assert_eq!(main.subagents[0].started, now - 90);
    // No parent append: only meta changes, including after reloading persisted state.
    index.flush().unwrap();
    drop(index);
    let mut index = Index::open(&f.0, vec![(Agent::Grok, root.clone())]).unwrap();
    grok_meta(&parent, "running", now - 90, Some(now - 5), "completed");
    let changed = index.update_many([running.clone()], 1).unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].path, parent.to_string_lossy());
    assert_eq!(card(&index, &parent).subagents[0].ended, Some(now - 5));
    assert!(index.update_many([running], 2).unwrap().is_empty());
    assert!(agent_adapter(Agent::Grok)
        .subagent_parent(&parent.with_file_name("summary.json"))
        .is_none());
    assert!(agent_adapter(Agent::Grok)
        .subagent_parent(&parent.parent().unwrap().join("subagents/notes.json"))
        .is_none());
    // A fork/resume reference names the source of context, not the delegating parent.
    let child = root.join("%2Frepo/child/updates.jsonl");
    append(
        &child,
        grok_record(
            now - 10,
            json!({"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"Inspect"}}),
        ),
    );
    fs::write(
        child.with_file_name("summary.json"),
        json!({"session_kind":"subagent_fork","parent_session_id":"context-source"}).to_string(),
    )
    .unwrap();
    index.update_many([child.clone()], 3).unwrap();
    assert!(!card(&index, &child).hidden_reason.is_empty());
    assert!(card(&index, &child).parent.is_empty());
}

#[test]
#[ignore = "requires MOORING_REPLAY_QODER_CHILD and MOORING_REPLAY_GROK_PARENT"]
fn replay_qoder_child_and_grok_metadata_read_only() {
    use fibo_core::content::Source;
    let child = PathBuf::from(std::env::var_os("MOORING_REPLAY_QODER_CHILD").unwrap());
    let grok = PathBuf::from(std::env::var_os("MOORING_REPLAY_GROK_PARENT").unwrap());
    let mut parser = fibo_host::content::Parser::default();
    agent_adapter(Agent::Qoder).observe_file(&mut parser, &child);
    let mut offset = 0;
    for line in fs::read(&child).unwrap().split_inclusive(|b| *b == b'\n') {
        parser.add(
            Agent::Qoder,
            line,
            Source {
                path: child.to_string_lossy().into_owned(),
                offset,
                ..Default::default()
            },
        );
        offset += line.len() as u64;
    }
    assert_eq!(
        parser.card.parent,
        child
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert!(!parser.card.agent_name.is_empty());
    assert_eq!(parser.card.turn, Turn::Completed);
    assert!(!parser.card.interactive);
    let children = agent_adapter(Agent::Grok).subagents(&grok).unwrap();
    assert!(!children.is_empty());
    for c in &children {
        let raw: Value = serde_json::from_slice(
            &fs::read(
                grok.parent()
                    .unwrap()
                    .join(format!("subagents/{}/meta.json", c.id)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            raw["parent_session_id"],
            grok.parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        );
        assert_eq!(c.running(), raw["completed_at"].is_null());
        assert_eq!(
            c.stopped,
            !raw["completed_at"].is_null() && raw["status"] != "completed"
        );
    }
    eprintln!(
        "read-only replay: Qoder={:?}; Grok children={}, completed={}, stopped={}",
        parser.card.turn,
        children.len(),
        children
            .iter()
            .filter(|c| !c.running() && !c.stopped)
            .count(),
        children.iter().filter(|c| c.stopped).count()
    );
}
