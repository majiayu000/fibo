//! The phone's 待办 tab through `mooring remote --stdio`: every todo you have not closed with
//! its state, your words and the progress so far; noting one down, closing and reopening it;
//! and starting its Agent in its space.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use mooring_core::{agent::Agent, model::*, protocol::*};
use serde_json::json;

fn task(id: &str, space: &str, title: &str) -> Task {
    Task {
        id: id.into(),
        space: space.into(),
        title: title.into(),
        description: String::new(),
        acceptance: vec![],
        agent: Agent::Codex,
    }
}
fn edit(h: &Host, edit: Edit) {
    h.call(Request::Edit {
        request: None,
        edit,
    })
    .unwrap();
}

/// Not only those nobody started: the tab is every todo you have not closed. An older phone
/// reads the first four fields as before.
#[test]
fn list_carries_every_unfinished_todo_with_its_state_words_and_progress() {
    let h = Host::start();
    let mut first = task("t1", "s", "写周报");
    first.description = "  这周的，发群里  ".into();
    first.acceptance = vec![AcceptanceCriterion {
        id: "c".into(),
        description: "群里有人回".into(),
        evidence: vec![],
    }];
    for t in [first, task("t2", "s", "已经放弃的")] {
        edit(&h, Edit::AddTask(t));
    }
    edit(
        &h,
        Edit::AddTaskNote {
            task: "t1".into(),
            session: None,
            text: "初稿写好了".into(),
        },
    );
    edit(
        &h,
        Edit::SetTaskOutcome {
            id: "t2".into(),
            outcome: Outcome::Abandoned,
            seen: None,
        },
    );
    let mut view = Remote::open(&h, Some("view"), &[]);
    let list = view.ok(json!({ "op": "list" }));
    let todos = list["todos"].as_array().unwrap();
    assert_eq!(todos.len(), 1, "{list}");
    let t = &todos[0];
    assert_eq!(
        (&t["id"], &t["space"], &t["title"], &t["agent"]),
        (&json!("t1"), &json!("s"), &json!("写周报"), &json!("codex"))
    );
    assert_eq!(t["state"], "not_started");
    assert_eq!(
        t["state_label"],
        mooring_core::status::TaskState::NotStarted.label()
    );
    assert_eq!(t["said"], "这周的，发群里");
    assert_eq!(t["acceptance"], json!(["群里有人回"]));
    assert_eq!(t["notes"][0]["text"], "初稿写好了");
    assert!(t["terminal"].is_null());
}

/// Closing is yours, and a phone is your hand: done takes it off the tab, open brings it
/// back (the undo). A phone that only watches may not.
#[test]
fn a_phone_notes_closes_and_reopens_a_todo_and_only_with_full_control() {
    let h = Host::start();
    let dir = scratch(&h, "proj");
    edit(
        &h,
        Edit::AddSpace(Space {
            id: "p".into(),
            name: "proj".into(),
            cwd: dir.to_string_lossy().into_owned(),
        }),
    );
    let mut view = Remote::open(&h, Some("view"), &[]);
    assert!(!view.ops().iter().any(|o| o.starts_with("todo")));
    view.error(json!({ "op": "todoadd", "title": "x", "space": "p" }));

    let mut r = Remote::open(&h, None, &[]);
    assert!(r.ops().contains(&"todoadd".to_owned()) && r.ops().contains(&"todooutcome".to_owned()));
    r.error(json!({ "op": "todoadd", "title": "  ", "space": "p" }));
    r.error(json!({ "op": "todoadd", "title": "x", "space": "nowhere" }));
    let add = json!({ "op": "todoadd", "title": " 修键盘 ", "said": "输入法挡住了", "space": "p", "request": "a1" });
    r.ok(add.clone());
    r.ok(add);
    let w = h.workspace();
    assert_eq!(w.tasks.len(), 1, "a resend adds it once");
    let id = w.tasks[0].id.clone();
    assert_eq!(
        (
            w.tasks[0].title.as_str(),
            w.tasks[0].description.as_str(),
            w.tasks[0].space.as_str()
        ),
        ("修键盘", "输入法挡住了", "p")
    );

    let listed = |r: &mut Remote| {
        r.ok(json!({ "op": "list" }))["todos"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(listed(&mut r), 1);
    r.ok(json!({ "op": "todooutcome", "task": id, "outcome": "done", "request": "d1" }));
    assert_eq!(
        h.workspace().task_outcomes.get(&id),
        Some(&Outcome::Completed)
    );
    assert_eq!(listed(&mut r), 0);
    r.ok(json!({ "op": "todooutcome", "task": id, "outcome": "open", "request": "d2" }));
    assert_eq!(listed(&mut r), 1, "undone");
    r.error(json!({ "op": "todooutcome", "task": id, "outcome": "deferred" }));
    r.error(json!({ "op": "todooutcome", "task": "gone", "outcome": "done" }));
}

/// 接着做 with nothing working on it: its Agent in a new tab of its space, the tab working
/// for the todo so the conversation that starts there counts for it.
#[test]
fn open_with_a_todo_starts_it_in_its_space_for_that_todo() {
    let h = Host::start();
    let first = scratch(&h, "first");
    let dir = scratch(&h, "proj");
    for (id, cwd) in [("a-first", &first), ("p", &dir)] {
        edit(
            &h,
            Edit::AddSpace(Space {
                id: id.into(),
                name: id.into(),
                cwd: cwd.to_string_lossy().into_owned(),
            }),
        );
    }
    edit(&h, Edit::AddTask(task("t1", "p", "修键盘")));
    let mut r = Remote::open(&h, None, &[]);
    r.error(json!({ "op": "open", "task": "gone", "request": "o0" }));
    let opened = r.ok(json!({ "op": "open", "task": "t1", "request": "o1" }));
    let terminal = opened["terminal"].as_str().unwrap();
    let w = h.workspace();
    assert_eq!(w.terminal_home(terminal), Some("p"));
    assert_eq!(
        w.terminal_tab(terminal)
            .and_then(|t| t.default_task.as_deref()),
        Some("t1")
    );
    // A folder from elsewhere does not move it out of its space; a space picked by hand does.
    let elsewhere = r
        .ok(json!({ "op": "open", "task": "t1", "cwd": first.to_string_lossy(), "request": "o2" }));
    let elsewhere = elsewhere["terminal"].as_str().unwrap();
    assert_eq!(h.workspace().terminal_home(elsewhere), Some("p"));
    let picked = r.ok(json!({ "op": "open", "task": "t1", "space": "a-first", "request": "o3" }));
    let picked = picked["terminal"].as_str().unwrap();
    assert_eq!(h.workspace().terminal_home(picked), Some("a-first"));
}
