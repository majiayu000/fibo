//! Codex child logs feed the same sidebar list as Claude's subagent metadata.
use super::*;
use mooring_core::content::{Subagent, Turn};
use serde_json::json;

fn event(at: u64, kind: &str) -> String {
    format!(
        "{}\n",
        json!({"timestamp": at, "type": "event_msg", "payload": {"type": kind}})
    )
}
fn seed(f: &Fixture, id: &str, parent: Option<&str>, at: u64) -> PathBuf {
    let path = f.0.join(format!("logs/{id}.jsonl"));
    let source = parent.map_or(json!("cli"), |p| {
        json!({"subagent": {"thread_spawn": {
            "parent_thread_id": p, "agent_path": format!("/root/{id}"), "depth": 1
        }}})
    });
    let header = json!({"timestamp": at, "type": "session_meta", "payload": {
        "id": id, "cwd": "/tmp", "source": source, "agent_nickname": id
    }});
    fs::write(&path, format!("{header}\n{}", event(at, "task_started"))).unwrap();
    path
}
fn append(i: &mut Index, path: &Path, at: u64, kind: &str) {
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(event(at, kind).as_bytes())
        .unwrap();
    i.update_many([path.to_owned()], at).unwrap();
}
fn list(i: &Index, path: &Path) -> Vec<Subagent> {
    i.card(path.to_str().unwrap()).unwrap().unwrap().subagents
}

#[test]
fn codex_children_refresh_sidebar_without_parent_output_and_survive_restart() {
    let f = Fixture::new();
    let now = mooring_core::content::now();
    let parent = seed(&f, "parent", None, now - 100);
    let child = seed(&f, "review", Some("parent"), now - 80);
    let other = seed(&f, "unrelated", None, now - 70);
    let mut i = f.index();
    // Child discovered first, including while its parent is not indexed yet.
    i.update_many([child.clone(), other, parent.clone()], 1)
        .unwrap();
    let subs = list(&i, &parent);
    assert_eq!(subs.len(), 1, "same cwd does not establish parenthood");
    assert_eq!(
        (subs[0].id.as_str(), subs[0].name.as_str()),
        ("review", "review")
    );
    assert_eq!(subs[0].started, now - 80);
    assert!(subs[0].running());
    append(&mut i, &parent, now - 60, "task_complete");
    assert!(
        list(&i, &parent)[0].running(),
        "parent stopping does not finish its child"
    );
    append(&mut i, &child, now - 40, "turn_aborted");
    let sub = &list(&i, &parent)[0];
    assert!(sub.stopped);
    assert_eq!(sub.ended, Some(now - 40));
    append(&mut i, &child, now - 30, "task_started");
    let sub = &list(&i, &parent)[0];
    assert!(sub.running() && !sub.stopped, "resumed child runs again");
    assert_eq!(sub.started, now - 30);
    append(&mut i, &child, now - 10, "task_complete");
    assert_eq!(list(&i, &parent)[0].ended, Some(now - 10));
    assert!(!list(&i, &parent)[0].stopped);
    assert_eq!(
        i.card(parent.to_str().unwrap()).unwrap().unwrap().turn,
        Turn::Completed
    );
    i.flush().unwrap();
    drop(i);
    let mut i = f.index();
    assert_eq!(list(&i, &parent)[0].ended, Some(now - 10));
    append(&mut i, &parent, now, "task_started");
    assert!(
        list(&i, &parent).is_empty(),
        "previous turn's finished children leave the list"
    );
}

#[test]
fn codex_children_keep_this_turns_old_completions_and_mark_stale_work_stopped() {
    let f = Fixture::new();
    let now = mooring_core::content::now();
    let parent = seed(&f, "parent", None, now - 3600);
    let done = seed(&f, "done", Some("parent"), now - 3500);
    let stale = seed(&f, "stale", Some("parent"), now - 3400);
    let mut i = f.index();
    i.update_many([parent.clone(), done.clone(), stale], 1)
        .unwrap();
    append(&mut i, &done, now - 3300, "task_complete");
    let subs = list(&i, &parent);
    assert_eq!(subs.len(), 2);
    assert_eq!(subs[0].id, "done");
    assert_eq!(subs[0].ended, Some(now - 3300));
    assert!(
        !subs[0].stopped,
        "completion remains visible beyond the freshness window"
    );
    assert_eq!(subs[1].id, "stale");
    assert!(
        subs[1].stopped && !subs[1].running(),
        "silent work is not reported as successful"
    );
}

#[test]
fn codex_children_count_each_waiting_child_and_keep_the_parent_question() {
    let f = Fixture::new();
    let now = mooring_core::content::now();
    let parent = seed(&f, "parent", None, now - 100);
    let a = seed(&f, "a", Some("parent"), now - 80);
    let b = seed(&f, "b", Some("parent"), now - 70);
    let mut i = f.index();
    i.update_many([parent.clone(), a.clone(), b.clone()], 1)
        .unwrap();
    append(&mut i, &parent, now - 60, "task_complete");
    for (n, child) in [a, b].iter().enumerate() {
        let ask = json!({"timestamp": now - 50 + n as u64, "type": "response_item", "payload": {
            "type": "function_call", "name": "request_user_input", "call_id": "q",
            "arguments": json!({"questions": [{"question": "是否继续？"}]}).to_string()
        }});
        fs::OpenOptions::new()
            .append(true)
            .open(child)
            .unwrap()
            .write_all(format!("{ask}\n").as_bytes())
            .unwrap();
        i.update_many([child.clone()], 3 + n as u64).unwrap();
    }
    let card = i.card(parent.to_str().unwrap()).unwrap().unwrap();
    assert_eq!(card.turn, Turn::Waiting);
    assert_eq!(card.background.unwrap().agents, 2);
    assert!(card.waiting.contains("是否继续？"));
    assert_eq!(card.subagents.len(), 2);
    assert!(card.subagents.iter().all(Subagent::running));
}
