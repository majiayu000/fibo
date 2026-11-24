use super::store::Store;
use fibo_core::layout::Layout;
use fibo_core::model::{Edit, Tab, Workspace};
use fibo_core::protocol::{CONTROL, MAJOR, Reply, Request, receive, send};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A host that launches anything, refuses the edits `refuse` names (`"pending"`,
/// `"place"`), and shows the terminal placed when `shown` says so.
struct Fake {
    asked: Mutex<Vec<Request>>,
    refuse: &'static [&'static str],
    shown: bool,
}
fn serve(name: &str, refuse: &'static [&'static str], shown: bool) -> (PathBuf, Arc<Fake>) {
    let dir = std::env::temp_dir().join(format!("fibo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let socket = dir.join("workbench.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let fake = Arc::new(Fake {
        asked: Mutex::default(),
        refuse,
        shown,
    });
    let host = Arc::clone(&fake);
    std::thread::spawn(move || {
        // The store's status subscription: held open, never answered.
        let mut watching = Vec::new();
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let _: Request = receive(&mut stream).unwrap();
            let hello = Reply::Hello {
                major: MAJOR,
                instance: "fake".into(),
                build: Default::default(),
            };
            send(&mut stream, CONTROL, &hello).unwrap();
            let request: Request = receive(&mut stream).unwrap();
            if matches!(request, Request::WatchRecognition) {
                watching.push(stream);
                continue;
            }
            let refused = |what| host.refuse.contains(&what);
            let reply = match &request {
                Request::Launch { .. } => Reply::Workspace(Workspace::default()),
                Request::Edit {
                    edit: Edit::SetPendingTask { .. },
                    ..
                } if refused("pending") => Reply::Error("待办不存在".into()),
                Request::Edit {
                    edit: Edit::CreateTab { .. },
                    ..
                } if refused("place") => Reply::Error("画面不存在".into()),
                Request::Edit { .. } => Reply::Workspace(Workspace::default()),
                Request::Workspace => Reply::Workspace(placed(host.shown)),
                Request::Stop { .. } => Reply::Ok,
                other => panic!("not asked by a launch: {other:?}"),
            };
            host.asked.lock().unwrap().push(request);
            send(&mut stream, CONTROL, &reply).unwrap();
        }
    });
    (socket, fake)
}
fn placed(shown: bool) -> Workspace {
    let mut w = Workspace::default();
    if shown {
        w.tabs.push(tab());
    }
    w
}
fn tab() -> Tab {
    Tab {
        id: "tab".into(),
        space: "s".into(),
        default_task: None,
        title: "shell".into(),
        layout: Some(Layout::Leaf("new".into())),
    }
}
fn launch(socket: PathBuf, task: Option<&str>) -> std::io::Result<Reply> {
    let (tx, _rx) = futures::channel::mpsc::unbounded();
    let store = Store::new(socket, tx);
    let request = Request::Launch {
        id: "new".into(),
        space: "s".into(),
        argv: vec!["/bin/sh".into()],
        cwd: "/tmp".into(),
        cols: 100,
        rows: 30,
        scheme: Default::default(),
        colors: None,
        start: None,
    };
    let place = Edit::CreateTab { tab: tab() };
    futures::executor::block_on(store.launch(None, None, request, place, task.map(Into::into)))
}
/// What the host was asked, by kind, in order.
fn asked(fake: &Fake) -> Vec<&'static str> {
    let kinds = fake.asked.lock().unwrap();
    kinds
        .iter()
        .map(|r| match r {
            Request::Launch { .. } => "launch",
            Request::Edit {
                edit: Edit::SetPendingTask { .. },
                ..
            } => "pending",
            Request::Edit { .. } => "place",
            Request::Workspace => "workspace",
            Request::Stop { id } if id == "new" => "stop",
            other => panic!("{other:?}"),
        })
        .collect()
}

/// #523: the terminal started, putting it in a tab failed, and it ran on with no pane;
/// each retry started one more. It is stopped and the failure still reaches the user.
#[test]
fn a_terminal_that_could_not_be_placed_is_stopped() {
    let (socket, fake) = serve("launch-place-refused", &["place"], false);
    let error = launch(socket, None).unwrap_err();
    assert_eq!(error.to_string(), "画面不存在");
    assert_eq!(asked(&fake), ["launch", "place", "workspace", "stop"]);
}

/// A to-do's terminal stops there too when its to-do could not be recorded.
#[test]
fn a_terminal_whose_task_could_not_be_recorded_is_stopped() {
    let (socket, fake) = serve("launch-pending-refused", &["pending"], false);
    assert!(launch(socket, Some("todo")).is_err());
    assert_eq!(asked(&fake), ["launch", "pending", "workspace", "stop"]);
}

/// One the host shows in a tab after all (the edit landed, its reply did not) keeps running.
#[test]
fn a_terminal_the_host_shows_placed_is_not_stopped() {
    let (socket, fake) = serve("launch-shown-after-all", &["place"], true);
    assert!(launch(socket, None).is_err());
    assert_eq!(asked(&fake), ["launch", "place", "workspace"]);
}

/// A launch that went through asks nothing more.
#[test]
fn a_placed_terminal_asks_nothing_more() {
    let (socket, fake) = serve("launch-placed", &[], false);
    assert!(matches!(
        launch(socket, Some("todo")),
        Ok(Reply::Workspace(_))
    ));
    assert_eq!(asked(&fake), ["launch", "pending", "place"]);
}
