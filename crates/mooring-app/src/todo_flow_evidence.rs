//! MOORING_TODO_FLOW_EVIDENCE: a todo from the window to a real Claude Code and back, with
//! real mouse events on the real buttons. `MOORING_TODO_FLOW_CWD` is a directory Claude
//! trusts, and `claude` on the terminal's PATH is the real one, so this spends model quota.
//!
//! The value names where to click, `start=x,y pick=x,y from=x,y done=x,y`. Layout is read
//! off the pictures, so a run stops at the first point it has no value for, having saved
//! the picture to read it from. Each step is checked against the host's workspace:
//! 1. 开始, then the Agent picked: Claude starts and the text waits in its input, unsent;
//! 2. Enter: it does the work and writes progress back on its own;
//! 3. the progress panel's 来自 opens the conversation that wrote the note;
//! 4. 标记完成 closes the todo, with its time, and its conversation with it.
use super::pane_drag_evidence::{down, send, up};
use super::*;
use gpui::{InputEvent, Pixels, Point, WindowHandle, point};
use mooring_core::status::{self, Status};
use std::time::{Duration, Instant};
use workbench::Workbench;

const TODO: &str = "flow-todo";

fn spot(spec: &str, name: &str) -> Option<Point<Pixels>> {
    let (x, y) = spec
        .split_whitespace()
        .find_map(|part| part.strip_prefix(name)?.strip_prefix('='))?
        .split_once(',')?;
    Some(point(px(x.parse().ok()?), px(y.parse().ok()?)))
}

pub(super) fn start(
    handle: Option<WindowHandle<Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    let Ok(spec) = std::env::var("MOORING_TODO_FLOW_EVIDENCE") else {
        return false;
    };
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
        wait(1500).await;
        std::fs::create_dir_all(&directory).unwrap();
        let socket = handle.update(cx, |v, _, _| v.visual_socket()).unwrap();
        let call = |r: Request| rpc(&socket, &r).unwrap();
        let workspace = || match call(Request::Workspace) {
            Reply::Workspace(w) => w,
            x => panic!("{x:?}"),
        };
        let edit = |edit: Edit| {
            let reply = call(Request::Edit {
                request: None,
                edit,
            });
            assert!(
                matches!(reply, Reply::Ok | Reply::Workspace(_)),
                "{reply:?}"
            );
        };
        let shot = |name: &str, cx: &mut gpui::AsyncApp| {
            let path = directory.join(name);
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh();
                w.draw(a).clear(a);
                w.render_to_image().unwrap().save(path).unwrap();
            })
            .unwrap();
        };
        let click = |at: Point<Pixels>, cx: &mut gpui::AsyncApp| {
            send(handle, cx, down(at));
            send(handle, cx, up(at));
        };
        edit(Edit::AddSpace(Space {
            id: "flow".into(),
            name: "Flow".into(),
            cwd: std::env::var("MOORING_TODO_FLOW_CWD").expect("a directory Claude trusts"),
        }));
        edit(Edit::AddTask(Task {
            id: TODO.into(),
            space: "flow".into(),
            title: "数一下 crates 和 plugins 各有几项".into(),
            description: "分两段做，一段一条命令。crates 上次已经数过了，不用再数。".into(),
            acceptance: vec![],
            agent: Agent::Claude,
        }));
        edit(Edit::AddTaskNote {
            task: TODO.into(),
            session: None,
            text: "上次查到 crates 下有 6 项，还剩 plugins 没数。".into(),
        });
        wait(800).await;
        let open = |cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, _, cx| v.visual_open_task(TODO, cx))
                .unwrap();
        };
        open(cx);
        wait(800).await;
        shot("1-detail.png", cx);
        let done = |cx: &mut gpui::AsyncApp| cx.update(|cx| cx.quit());
        let Some(at) = spot(&spec, "start") else {
            return done(cx);
        };
        click(at, cx);
        wait(500).await;
        shot("2-picker.png", cx);
        let Some(at) = spot(&spec, "pick") else {
            return done(cx);
        };
        click(at, cx);

        // Claude starts; the text reaches its input and stays there.
        let until = Instant::now() + Duration::from_secs(120);
        let mut last = String::new();
        let terminal = loop {
            if Instant::now() >= until {
                shot("3-never-placed.png", cx);
                panic!("the text never reached the Agent's input:\n{last}");
            }
            let w = workspace();
            let tab = w
                .tabs
                .iter()
                .find(|t| t.default_task.as_deref() == Some(TODO));
            let pane = tab
                .and_then(|t| t.layout.as_ref())
                .and_then(|l| l.ids().first().cloned());
            if let Some(pane) = pane {
                let screen = match call(Request::ReadScreen {
                    id: pane.clone(),
                    rows: 40,
                }) {
                    Reply::Text(t) => t,
                    _ => String::new(),
                };
                last = screen.clone();
                if screen.contains("Pasted") || screen.contains("分两段做") {
                    eprintln!("placed:\n{screen}");
                    break pane;
                }
            }
            wait(500).await;
        };
        wait(1500).await;
        shot("3-placed.png", cx);
        let w = workspace();
        assert_eq!(w.task_logs[TODO].notes.len(), 1, "placed, not sent");
        // Claude's hook binds it at startup; Codex has none and shows up with its first ask.
        match w.current_agent(&terminal) {
            Some(session) => {
                assert_eq!(session.task.as_deref(), Some(TODO), "works for the todo");
            }
            None => eprintln!("not bound before the first ask"),
        }

        // The Enter the user presses.
        let key = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        send(handle, cx, key.to_platform_input());
        let until = Instant::now() + Duration::from_secs(300);
        loop {
            let w = workspace();
            let notes = w.task_logs[TODO].notes.len();
            let idle = w
                .current_agent(&terminal)
                .map(|s| status::of(&w, s, mooring_core::time::now()));
            if notes >= 2 && idle == Some(Status::Review) {
                break;
            }
            assert!(
                Instant::now() < until,
                "no progress written back: {notes} notes, {idle:?}"
            );
            wait(1000).await;
        }
        let w = workspace();
        for note in &w.task_logs[TODO].notes {
            eprintln!("note: {} ({:?})", note.text, note.session);
        }
        assert!(
            !w.task_outcomes.contains_key(TODO),
            "the Agent did not close it"
        );
        open(cx);
        wait(800).await;
        shot("4-progress.png", cx);

        let Some(at) = spot(&spec, "from") else {
            return done(cx);
        };
        click(at, cx);
        wait(800).await;
        shot("5-from-clicked.png", cx);
        let left = handle.update(cx, |v, _, _| v.visual_task_detail()).unwrap();
        let focused = handle.update(cx, |v, _, _| v.visual_nav()).unwrap().0;
        assert_eq!(left, None, "来自 leaves the detail page");
        assert_eq!(
            focused.as_deref(),
            Some(terminal.as_str()),
            "and shows that conversation"
        );

        open(cx);
        wait(800).await;
        // A press lands on what was last drawn, so the page has to be drawn first.
        shot("5b-before-done.png", cx);
        let Some(at) = spot(&spec, "done") else {
            return done(cx);
        };
        click(at, cx);
        wait(1000).await;
        shot("6-done.png", cx);
        let w = workspace();
        assert_eq!(w.task_outcomes.get(TODO), Some(&Outcome::Completed));
        assert!(w.task_logs[TODO].closed.is_some(), "closed at a time");
        let s = w.current_agent(&terminal).unwrap();
        assert_eq!(status::of(&w, s, mooring_core::time::now()), Status::Closed);
        eprintln!("TODO-FLOW PASS");
        done(cx);
    })
    .detach();
    true
}
