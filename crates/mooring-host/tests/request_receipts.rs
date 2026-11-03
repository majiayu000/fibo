//! A named edit takes effect once, however often it is sent: after the sender lost the
//! connection before its receipt, and after the host restarted (docs/request-envelope).
use mooring_core::{model::*, protocol::*};
use std::{
    fs, io,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Host {
    child: Child,
    root: PathBuf,
}
impl Host {
    fn start(root: &Path) -> Self {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "receipts_daemon", "--nocapture"])
            .env("MOORING_FIXTURE_RUNTIME", root)
            .env("MOORING_REMOTE_EXE", env!("CARGO_BIN_EXE_mooring"))
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let until = Instant::now() + Duration::from_secs(30);
        while rpc(&root.join("workbench.sock"), &Request::Workspace).is_err() {
            assert!(Instant::now() < until, "host never listened");
            std::thread::sleep(Duration::from_millis(20));
        }
        Self {
            child,
            root: root.to_owned(),
        }
    }
    fn stop(self) {}
    fn call(&self, request: Option<RequestId>, edit: Edit) -> io::Result<Reply> {
        rpc(
            &self.root.join("workbench.sock"),
            &Request::Edit { request, edit },
        )
    }
    /// Sends without waiting for the receipt, and hangs up.
    fn send_and_leave(&self, request: RequestId, edit: Edit) {
        let mut s = UnixStream::connect(self.root.join("workbench.sock")).unwrap();
        send(&mut s, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
        receive::<Reply>(&mut s).unwrap();
        let edit = Request::Edit {
            request: Some(request),
            edit,
        };
        send(&mut s, CONTROL, &edit).unwrap();
    }
    fn workspace(&self) -> Workspace {
        match rpc(&self.root.join("workbench.sock"), &Request::Workspace).unwrap() {
            Reply::Workspace(w) => w,
            other => panic!("{other:?}"),
        }
    }
    fn tasks(&self) -> Vec<String> {
        match rpc(&self.root.join("workbench.sock"), &Request::Workspace).unwrap() {
            Reply::Workspace(w) => w.tasks.into_iter().map(|t| t.id).collect(),
            other => panic!("{other:?}"),
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(self.root.join("workbench.sock"));
    }
}

fn id(client: &str, id: &str) -> RequestId {
    RequestId {
        client: client.into(),
        id: id.into(),
    }
}
fn task(id: &str) -> Edit {
    Edit::AddTask(Task {
        id: id.into(),
        space: "s".into(),
        title: id.into(),
        description: String::new(),
        acceptance: Vec::new(),
        agent: Agent::Claude,
    })
}

#[test]
fn a_named_edit_takes_effect_once_across_hangups_and_restarts() {
    let root = std::env::temp_dir().join(format!("kl-rq-{}", uuid::Uuid::new_v4().simple()));
    let h = Host::start(&root);
    h.call(
        None,
        Edit::AddSpace(Space {
            id: "s".into(),
            name: "Tests".into(),
            cwd: "/tmp".into(),
        }),
    )
    .unwrap();

    // The sender hangs up before its receipt and sends again under the same name.
    h.send_and_leave(id("app", "1"), task("t1"));
    std::thread::sleep(Duration::from_millis(200));
    for _ in 0..3 {
        assert!(matches!(
            h.call(Some(id("app", "1")), task("t1")).unwrap(),
            Reply::Workspace(_)
        ));
    }
    assert_eq!(h.tasks(), ["t1"]);
    // Unnamed, the same resend is a second creation and refused.
    let again = h.call(None, task("t1")).unwrap_err();
    assert!(again.to_string().contains("ID"), "{again}");
    // Another sender's request 1 is its own.
    h.call(Some(id("phone", "1")), task("t2")).unwrap();

    // A refusal comes back the same, even after what refused it is gone.
    let done = |task: &str| Edit::SetTaskOutcome {
        id: task.into(),
        outcome: Outcome::Abandoned,
        seen: None,
    };
    let first = h.call(Some(id("app", "2")), done("t3")).unwrap_err();
    assert!(first.to_string().contains("待办不存在"), "{first}");
    h.call(Some(id("app", "3")), task("t3")).unwrap();
    let resent = h.call(Some(id("app", "2")), done("t3")).unwrap_err();
    assert_eq!(resent.to_string(), first.to_string());

    // The host restarts: every outcome is still known.
    h.stop();
    let h = Host::start(&root);
    assert!(matches!(
        h.call(Some(id("app", "1")), task("t1")).unwrap(),
        Reply::Workspace(_)
    ));
    let resent = h.call(Some(id("app", "2")), done("t3")).unwrap_err();
    assert_eq!(resent.to_string(), first.to_string());
    let mut tasks = h.tasks();
    tasks.sort();
    assert_eq!(tasks, ["t1", "t2", "t3"]);
    h.stop();
    let _ = fs::remove_dir_all(&root);
}

/// Step C's acceptance: a write sent while the host is gone waits it out and lands once
/// it is back, instead of vanishing; with no host at all it reports that it was not sent.
#[test]
fn a_write_made_while_the_host_is_away_lands_once_it_is_back() {
    use mooring_core::delivery::{deliver, Settled, PATIENCE};
    let root = std::env::temp_dir().join(format!("kl-rq-{}", uuid::Uuid::new_v4().simple()));
    let h = Host::start(&root);
    h.call(
        None,
        Edit::AddSpace(Space {
            id: "s".into(),
            name: "Tests".into(),
            cwd: "/tmp".into(),
        }),
    )
    .unwrap();
    h.stop();
    let socket = root.join("workbench.sock");
    let back = {
        let root = root.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            Host::start(&root)
        })
    };
    let write = Request::Edit {
        request: Some(RequestId::fresh()),
        edit: task("t1"),
    };
    let started = Instant::now();
    let settled = deliver(&socket, &write, PATIENCE);
    assert!(
        matches!(settled, Settled::Confirmed(Reply::Workspace(_))),
        "{settled:?}"
    );
    assert!(started.elapsed() >= Duration::from_millis(1000));
    let h = back.join().unwrap();
    // Sent again (the window's 重试), it is the same write.
    assert!(matches!(
        deliver(&socket, &write, PATIENCE),
        Settled::Confirmed(_)
    ));
    assert_eq!(h.tasks(), ["t1"]);
    h.stop();
    let gone = deliver(&socket, &write, Duration::from_millis(500));
    assert!(
        matches!(gone, Settled::Unknown { sent: false, .. }),
        "{gone:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Step D's acceptance on a real host: an Agent's split (`mooring split`), the window's
/// rename and divider drag all stay; closing a tab that gained a pane since the user
/// agreed is refused and ends nothing.
#[test]
fn layout_changes_from_two_hands_all_stay() {
    use mooring_core::layout::{Axis, Layout};
    let root = std::env::temp_dir().join(format!("kl-rq-{}", uuid::Uuid::new_v4().simple()));
    let h = Host::start(&root);
    h.call(
        None,
        Edit::AddSpace(Space {
            id: "s".into(),
            name: "Tests".into(),
            cwd: "/tmp".into(),
        }),
    )
    .unwrap();
    let socket = root.join("workbench.sock");
    for id in ["a", "b", "agent"] {
        let launch = Request::Launch {
            id: id.into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), "sleep 30".into()],
            cwd: "/tmp".into(),
            cols: 160,
            rows: 48,
            scheme: Default::default(),
        };
        rpc(&socket, &launch).unwrap();
    }
    let leaf = |id: &str| Box::new(Layout::Leaf(id.into()));
    let tab = Tab {
        id: "t".into(),
        space: "s".into(),
        default_task: None,
        title: String::new(),
        layout: Some(Layout::Split {
            axis: Axis::Right,
            ratio: 5000,
            first: leaf("a"),
            second: leaf("b"),
        }),
    };
    h.call(Some(RequestId::fresh()), Edit::CreateTab { tab })
        .unwrap();
    // What the window saw before the Agent split.
    let seen = h.workspace();
    let agreed = Some(seen.tab_terminals("t"));
    h.call(
        None,
        Edit::SplitPane {
            target: "b".into(),
            terminal: "agent".into(),
            axis: Axis::Down,
            before: false,
            share: 5000,
            forced: false,
        },
    )
    .unwrap();
    let rename = Edit::RenameTab {
        id: "t".into(),
        title: "部署".into(),
    };
    h.call(Some(RequestId::fresh()), rename).unwrap();
    let drag = Edit::ResizeSplit {
        tab: "t".into(),
        path: vec![],
        axis: Axis::Right,
        ratio: 3000,
    };
    h.call(Some(RequestId::fresh()), drag).unwrap();
    let w = h.workspace();
    let t = w.tabs.iter().find(|t| t.id == "t").unwrap();
    assert_eq!(t.title, "部署");
    assert_eq!(w.tab_terminals("t"), ["a", "b", "agent"]);
    assert!(matches!(t.layout, Some(Layout::Split { ratio: 3000, .. })));
    let close = |expect| Edit::CloseTab {
        id: "t".into(),
        expect,
    };
    let refused = h.call(Some(RequestId::fresh()), close(agreed)).unwrap_err();
    assert!(refused.to_string().contains("再确认"), "{refused}");
    let w = h.workspace();
    assert_eq!(w.tab_terminals("t"), ["a", "b", "agent"]);
    assert!(w.terminals.iter().all(|t| t.state == ProcessState::Running));
    h.call(Some(RequestId::fresh()), close(Some(w.tab_terminals("t"))))
        .unwrap();
    assert!(h.workspace().tabs.is_empty());
    drop(h);
    let _ = fs::remove_dir_all(&root);
}

/// Step E's acceptance: a phone or another computer that lost its connection before the
/// reply sends the same line again on a new one, and it is typed once; unnamed, twice.
#[test]
fn a_remote_write_sent_again_after_a_reconnect_is_typed_once() {
    use mooring_remote::client::Client;
    use mooring_remote::protocol::{Hello, Op};
    let root = std::env::temp_dir().join(format!("kl-rq-{}", uuid::Uuid::new_v4().simple()));
    let h = Host::start(&root);
    h.call(
        None,
        Edit::AddSpace(Space {
            id: "s".into(),
            name: "Tests".into(),
            cwd: "/tmp".into(),
        }),
    )
    .unwrap();
    let typed = root.join("typed.txt");
    let launch = Request::Launch {
        id: "far".into(),
        space: "s".into(),
        argv: vec![
            "/bin/sh".into(),
            "-c".into(),
            format!("stty -echo; cat >> '{}'", typed.display()),
        ],
        cwd: "/tmp".into(),
        cols: 80,
        rows: 24,
        scheme: Default::default(),
    };
    rpc(&root.join("workbench.sock"), &launch).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let t = Duration::from_secs(10);
    let connect = || {
        let c = Client::spawn(&[
            env!("CARGO_BIN_EXE_mooring").to_owned(),
            "remote".into(),
            "--stdio".into(),
            "--runtime".into(),
            root.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let hello = Op::Hello {
            version: Some(1),
            client: Some("phone-1".into()),
        };
        let hello: Hello = serde_json::from_value(c.request(hello, t).unwrap()).unwrap();
        assert!(hello.requests);
        c
    };
    let send = |text: &str, request: Option<&str>| Op::Send {
        terminal: "far".into(),
        text: text.into(),
        enter: true,
        request: request.map(str::to_owned),
    };
    // The reply is lost with the connection; the phone reconnects and sends it again.
    let first = connect();
    first.request(send("named", Some("r1")), t).unwrap();
    drop(first);
    let again = connect();
    again.request(send("named", Some("r1")), t).unwrap();
    again.request(send("unnamed", None), t).unwrap();
    again.request(send("unnamed", None), t).unwrap();
    // A resend of a refused write is refused again, not tried a second time.
    let gone = Op::Keys {
        terminal: "gone".into(),
        keys: vec!["enter".into()],
        request: Some("r2".into()),
    };
    let refused = again.request(gone.clone(), t).unwrap_err().to_string();
    assert_eq!(again.request(gone, t).unwrap_err().to_string(), refused);
    std::thread::sleep(Duration::from_millis(500));
    let lines = fs::read_to_string(&typed).unwrap();
    let count = |w: &str| lines.lines().filter(|l| l.trim() == w).count();
    assert_eq!((count("named"), count("unnamed")), (1, 2), "{lines:?}");
    drop(again);
    drop(h);
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[ignore = "subprocess daemon entry for this file's tests"]
fn receipts_daemon() {
    let root = PathBuf::from(std::env::var_os("MOORING_FIXTURE_RUNTIME").unwrap());
    mooring_host::serve_with_roots(&root, vec![(Agent::Codex, root.join("logs/codex"))]).unwrap();
}
