//! Real source shapes go through their adapters, one index and one sidebar projection.
use mooring_core::{
    content::{Content, Turn},
    model::Agent,
};
use mooring_host::{log_index::Index, mirror::Mirror};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const PARENT: &str = "c19a5748-897d-4836-a555-fce8a7797222";
const CHILD: &str = "19817791-63b7-441c-ad06-25fe5e5293ba";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("mooring-child-adapters-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn card(index: &Index, path: &Path) -> Content {
    index
        .card(path.to_str().unwrap())
        .unwrap()
        .unwrap_or_else(|| panic!("missing card: {}", path.display()))
}
fn append(path: &Path, record: Value) {
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
fn add_part(db: &Connection, session: &str, id: &str, at: u64, role: &str, data: Value) {
    db.execute(
        "INSERT INTO message VALUES(?1,?2,?3,?4)",
        params![id, session, at as i64, json!({"role":role}).to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO part VALUES(?1,?1,?2,?3,?4)",
        params![id, session, at as i64, data.to_string()],
    )
    .unwrap();
    db.execute(
        "UPDATE session SET time_updated=?2 WHERE id=?1",
        params![session, at as i64],
    )
    .unwrap();
}

#[test]
fn opencode_and_cursor_children_share_the_index_without_cross_cli_links() {
    let f = Fixture::new();
    let now = mooring_core::content::now();
    let db_path = f.0.join("opencode.db");
    let db = Connection::open(&db_path).unwrap();
    db.execute_batch("CREATE TABLE session(id TEXT PRIMARY KEY,parent_id TEXT,directory TEXT,title TEXT,time_updated INTEGER);
        CREATE TABLE message(id TEXT PRIMARY KEY,session_id TEXT,time_updated INTEGER,data TEXT);
        CREATE TABLE part(id TEXT PRIMARY KEY,message_id TEXT,session_id TEXT,time_updated INTEGER,data TEXT);").unwrap();
    for (id, parent, title, at) in [
        (PARENT, None, "Main work", now - 100),
        ("oc-child", Some(PARENT), "Review changes", now - 80),
    ] {
        db.execute(
            "INSERT INTO session VALUES(?1,?2,'/repo',?3,?4)",
            params![id, parent, title, (at * 1000) as i64],
        )
        .unwrap();
        add_part(
            &db,
            id,
            &format!("ask-{id}"),
            at * 1000,
            "user",
            json!({"type":"text","text":title}),
        );
    }
    let mut spec: Value =
        serde_json::from_str(include_str!("../../../plugins/opencode/agent.json")).unwrap();
    spec["source"]["sqlite"] = db_path.to_string_lossy().into_owned().into();
    let agents = f.0.join("agents");
    fs::create_dir(&agents).unwrap();
    fs::write(agents.join("opencode.json"), spec.to_string()).unwrap();
    assert!(mooring_core::agent::install_from(&agents, &f.0));
    let oc = Agent::from_executable("opencode").unwrap();
    let source = oc.spec().unwrap().source.as_ref().unwrap();
    let out = mooring_host::mirror::dir(&f.0, oc);
    let mut mirror = Mirror::open(&out).unwrap();
    let mut reader = None;
    mirror.poll(source, &mut reader).unwrap();
    let parent = out.join(format!("{PARENT}.jsonl"));
    let child = out.join("oc-child.jsonl");
    let first: Value =
        serde_json::from_str(fs::read_to_string(&child).unwrap().lines().next().unwrap()).unwrap();
    assert_eq!(
        first["_parent"], PARENT,
        "parent identity precedes any prompt"
    );
    let cursor_root = f.0.join(".cursor/projects");
    fs::create_dir_all(&cursor_root).unwrap();
    let mut index = Index::open(
        &f.0,
        vec![(oc, out.clone()), (Agent::Cursor, cursor_root.clone())],
    )
    .unwrap();
    index
        .update_many([child.clone(), parent.clone()], 1)
        .unwrap();
    let main = card(&index, &parent);
    assert_eq!(main.subagents.len(), 1);
    assert_eq!(main.subagents[0].name, "Review changes");
    assert_eq!(
        main.subagents[0].started,
        now - 80,
        "SQLite milliseconds become transcript seconds"
    );
    assert!(main.subagents[0].running());
    assert_eq!(card(&index, &child).parent, PARENT);
    assert!(!card(&index, &child).hidden_reason.is_empty());
    assert!(!card(&index, &child).interactive);
    add_part(
        &db,
        PARENT,
        "done-parent",
        (now - 60) * 1000,
        "assistant",
        json!({"type":"step-finish","reason":"stop"}),
    );
    mirror.poll(source, &mut reader).unwrap();
    index.update_many([parent.clone()], 2).unwrap();
    assert_eq!(card(&index, &parent).turn, Turn::Completed);
    assert_eq!(card(&index, &parent).background.unwrap().agents, 1);
    // Only the child changes. Its question and completion must reach the held parent.
    add_part(
        &db,
        "oc-child",
        "question",
        (now - 40) * 1000,
        "assistant",
        json!({"type":"tool","tool":"question","state":{"status":"running","input":{"questions":[{"question":"Continue?"}]}}}),
    );
    mirror.poll(source, &mut reader).unwrap();
    index.update_many([child.clone()], 3).unwrap();
    assert_eq!(card(&index, &parent).turn, Turn::Waiting);
    assert!(card(&index, &parent).waiting.contains("Continue?"));
    add_part(
        &db,
        "oc-child",
        "answer",
        (now - 30) * 1000,
        "assistant",
        json!({"type":"tool","tool":"question","callID":"q","state":{"status":"completed","output":"Continue"}}),
    );
    add_part(
        &db,
        "oc-child",
        "done-child",
        (now - 20) * 1000,
        "assistant",
        json!({"type":"step-finish","reason":"stop"}),
    );
    mirror.poll(source, &mut reader).unwrap();
    index.update_many([child.clone()], 4).unwrap();
    assert_eq!(card(&index, &parent).turn, Turn::Completed);
    assert_eq!(card(&index, &parent).subagents[0].ended, Some(now - 20));
    // Cursor's child has no chat directory or header: its path carries the relationship.
    let dir = cursor_root.join(format!("repo/agent-transcripts/{PARENT}"));
    fs::create_dir_all(dir.join("subagents")).unwrap();
    let chat = f.0.join(format!(".cursor/chats/hash/{PARENT}"));
    fs::create_dir_all(&chat).unwrap();
    fs::write(chat.join("meta.json"), json!({"cwd":"/repo"}).to_string()).unwrap();
    fs::write(chat.join("prompt_history.json"), "[]").unwrap();
    let cursor_parent = dir.join(format!("{PARENT}.jsonl"));
    let cursor_child = dir.join(format!("subagents/{CHILD}.jsonl"));
    append(
        &cursor_parent,
        json!({"timestamp":now-100,"role":"user","message":{"content":[{"type":"text","text":"Parent task"}]}}),
    );
    append(
        &cursor_child,
        json!({"timestamp":now-50,"role":"user","message":{"content":[{"type":"text","text":"Inspect parser"}]}}),
    );
    let adapter = mooring_host::agent_adapter(Agent::Cursor);
    assert!(adapter.descend(&dir.join("subagents")));
    assert!(adapter.is_transcript(&cursor_child));
    index
        .update_many([cursor_parent.clone(), cursor_child.clone()], 5)
        .unwrap();
    let main = card(&index, &cursor_parent);
    assert_eq!(
        main.subagents.len(),
        1,
        "same parent id in OpenCode must not leak into Cursor"
    );
    assert_eq!(main.subagents[0].id, CHILD);
    assert_eq!(main.subagents[0].name, "Inspect parser");
    assert!(main.subagents[0].running());
    let sub = card(&index, &cursor_child);
    assert_eq!(sub.parent, PARENT);
    assert!(!sub.interactive && !sub.hidden_reason.is_empty());
    assert_eq!(
        index.child_logs(&[(oc, PARENT.into())], 32).unwrap(),
        [child.to_string_lossy()]
    );
    assert_eq!(
        index
            .child_logs(&[(Agent::Cursor, PARENT.into())], 32)
            .unwrap(),
        [cursor_child.to_string_lossy()]
    );
    append(
        &cursor_child,
        json!({"timestamp":now-10,"type":"turn_ended","status":"aborted"}),
    );
    index.update_many([cursor_child.clone()], 6).unwrap();
    assert!(card(&index, &cursor_parent).subagents[0].stopped);
    assert_eq!(card(&index, &parent).subagents.len(), 1);
    assert!(!card(&index, &parent).subagents[0].stopped);
    index.flush().unwrap();
    drop(index);
    let mut index = Index::open(&f.0, vec![(oc, out), (Agent::Cursor, cursor_root)]).unwrap();
    assert_eq!(card(&index, &parent).subagents[0].ended, Some(now - 20));
    assert!(card(&index, &cursor_parent).subagents[0].stopped);
    append(
        &cursor_child,
        json!({"timestamp":now-8,"role":"user","message":{"content":[{"type":"text","text":"Continue checking"}]}}),
    );
    append(
        &cursor_child,
        json!({"timestamp":now-5,"role":"assistant","message":{"content":[{"type":"tool_use","name":"AskQuestion","input":{"questions":[{"prompt":"Proceed?"}]}}]}}),
    );
    index.update_many([cursor_child.clone()], 7).unwrap();
    assert_eq!(card(&index, &cursor_parent).turn, Turn::Waiting);
    append(
        &cursor_child,
        json!({"timestamp":now,"type":"turn_ended","status":"success"}),
    );
    index.update_many([cursor_child], 8).unwrap();
    let main = card(&index, &cursor_parent);
    assert_ne!(
        main.turn,
        Turn::Waiting,
        "Cursor's explicit end releases a child question"
    );
    assert!(!main.subagents[0].running() && !main.subagents[0].stopped);
}

/// Read only the selected user's source records; all derived state stays in the fixture.
#[test]
#[ignore = "requires MOORING_REPLAY_OPENCODE_DB and MOORING_REPLAY_CURSOR_CHILD"]
fn replay_local_child_records_through_the_shared_index() {
    let f = Fixture::new();
    let database = PathBuf::from(std::env::var_os("MOORING_REPLAY_OPENCODE_DB").unwrap());
    let original_child = PathBuf::from(std::env::var_os("MOORING_REPLAY_CURSOR_CHILD").unwrap())
        .canonicalize()
        .unwrap();
    let db =
        Connection::open_with_flags(&database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let (id, parent, quoted): (String, String, String) = db.query_row(
        "SELECT id,parent_id,quote(parent_id) FROM session WHERE parent_id IS NOT NULL ORDER BY time_updated DESC LIMIT 1",[],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    let mut spec: Value =
        serde_json::from_str(include_str!("../../../plugins/opencode/agent.json")).unwrap();
    spec["source"]["sqlite"] = database.to_string_lossy().into_owned().into();
    spec["source"]["sessions"] = spec["source"]["sessions"]
        .as_str()
        .unwrap()
        // The available local sample predates the UI's 30-day window. Replay only this
        // chosen family; the shipped query and user's database are unchanged.
        .replace(
            " AND time_updated > (CAST(strftime('%s','now') AS INTEGER) - 30 * 86400) * 1000",
            "",
        )
        .replace(
            "WHERE time_updated > ?1",
            &format!("WHERE (id={quoted} OR parent_id={quoted}) AND time_updated > ?1"),
        )
        .into();
    let agents = f.0.join("agents");
    fs::create_dir(&agents).unwrap();
    fs::write(agents.join("opencode.json"), spec.to_string()).unwrap();
    assert!(mooring_core::agent::install_from(&agents, &f.0));
    let oc = Agent::from_executable("opencode").unwrap();
    let source = oc.spec().unwrap().source.as_ref().unwrap();
    let out = mooring_host::mirror::dir(&f.0, oc);
    Mirror::open(&out).unwrap().poll(source, &mut None).unwrap();
    let original_dir = original_child.parent().unwrap().parent().unwrap();
    let cursor_id = original_dir.file_name().unwrap().to_str().unwrap();
    // Copy unchanged records to a fresh fixture file: old files are intentionally only
    // indexed for identity by the historical scan, not replayed for their turn state.
    let cursor_root = f.0.join(".cursor/projects");
    let project = original_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .file_name()
        .unwrap();
    let cursor_dir = cursor_root
        .join(project)
        .join("agent-transcripts")
        .join(cursor_id);
    fs::create_dir_all(cursor_dir.join("subagents")).unwrap();
    let cursor_child = cursor_dir
        .join("subagents")
        .join(original_child.file_name().unwrap());
    let cursor_parent = cursor_dir.join(format!("{cursor_id}.jsonl"));
    fs::write(&cursor_child, fs::read(&original_child).unwrap()).unwrap();
    fs::write(
        &cursor_parent,
        fs::read(original_dir.join(format!("{cursor_id}.jsonl"))).unwrap(),
    )
    .unwrap();
    let mut index =
        Index::open(&f.0, vec![(oc, out.clone()), (Agent::Cursor, cursor_root)]).unwrap();
    let paths = fs::read_dir(&out)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "jsonl"));
    index.update_many(paths, 1).unwrap();
    index
        .update_many([cursor_child.clone(), cursor_parent.clone()], 2)
        .unwrap();
    let oc_child = card(&index, &out.join(format!("{id}.jsonl")));
    assert_eq!(oc_child.parent, parent);
    assert!(oc_child.turn_started > 0);
    assert!(!oc_child.hidden_reason.is_empty());
    let cu_child = card(&index, &cursor_child);
    assert_eq!(cu_child.parent, cursor_id);
    assert!(!cu_child.agent_name.is_empty());
    assert!(!cu_child.hidden_reason.is_empty());
    assert_eq!(cu_child.turn, Turn::Completed);
    let oc_main = card(&index, &out.join(format!("{parent}.jsonl")));
    let cu_main = card(&index, &cursor_parent);
    assert!(oc_main.subagents.iter().any(|c| c.id == id));
    assert!(
        cu_child.turn_started < cu_main.turn_started,
        "this historical child belongs to the parent's previous turn"
    );
    assert!(cu_main
        .subagents
        .iter()
        .all(|c| c.id != cursor_child.file_stem().unwrap().to_str().unwrap()));
    eprintln!(
        "real replay: OpenCode child={:?}, parent list={}; Cursor child={:?}, parent list={}",
        oc_child.turn,
        oc_main.subagents.len(),
        cu_child.turn,
        cu_main.subagents.len()
    );
}
