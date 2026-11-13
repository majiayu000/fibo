//! MOORING_RESUME_EVIDENCE: a conversation comes back once, against a real host killed for
//! real. Each step saves a picture and is checked against the host's workspace:
//! 1. clicking a conversation whose pane was lost reopens it in that tab, no new tab;
//! 2. showing a lost pane whose conversation already runs elsewhere removes the pane and
//!    moves to where it runs, starting nothing;
//! 3. showing a lost pane whose conversation you marked done removes it, starting nothing;
//! 4. clicking such a conversation's row is you asking: it resumes in the pane's tab.
//!
//! Run with PATH=/usr/bin:/bin and a scratch HOME: the resumes find no `claude` and stay
//! in their shell.
use super::*;
use gpui::WindowHandle;
use std::time::Duration;
use workbench::Workbench;

pub(super) fn start(
    handle: Option<WindowHandle<Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_RESUME_EVIDENCE").is_none() {
        return false;
    }
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        wait(1500).await;
        std::fs::create_dir_all(&directory).unwrap();
        let socket = handle.update(cx, |v, _, _| v.visual_socket()).unwrap();
        let root = socket.parent().unwrap().to_owned();
        let runtime = root.display().to_string();
        let call = |r: Request| rpc(&socket, &r).unwrap();
        let workspace = || match call(Request::Workspace) {
            Reply::Workspace(w) => w,
            x => panic!("{x:?}"),
        };
        let w = workspace();
        let space = match w.spaces.first() {
            Some(s) => s.id.clone(),
            None => {
                call(Request::Edit {
                    request: None,
                    edit: Edit::AddSpace(Space {
                        id: "s".into(),
                        name: "Tests".into(),
                        cwd: "/tmp".into(),
                    }),
                });
                "s".into()
            }
        };
        let logs = PathBuf::from(std::env::var_os("CLAUDE_CONFIG_DIR").expect("scratch config"))
            .join("projects/-tmp");
        std::fs::create_dir_all(&logs).unwrap();
        let claude = root.join("claude");
        let _ = std::os::unix::fs::symlink("/bin/sh", &claude);
        let session = |n: u32| format!("{:08x}-0000-4000-8000-00000000000{n}", std::process::id());
        // A Claude stand-in with a finished turn, or a plain shell without a session.
        let launch = |tid: &str, cli: Option<&str>| {
            let argv = match cli {
                Some(id) => {
                    let at = fibo_core::content::now();
                    let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/tmp","entrypoint":"cli","userType":"external","timestamp":at,"message":{"content":tid}});
                    let log = logs.join(format!("{id}.jsonl"));
                    std::fs::write(&log, format!("{row}\n")).unwrap();
                    let script = format!("printf '%s\\n' '⏺ {tid}'; sleep 600");
                    vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        format!("exec '{}' -c \"$0\" fixture --session-id {id}", claude.display()),
                        script,
                    ]
                }
                None => vec!["/bin/sh".into(), "-c".into(), "sleep 600".into()],
            };
            call(Request::Launch {
                colors: None,
                id: tid.into(),
                space: space.clone(),
                argv,
                cwd: "/tmp".into(),
                cols: 100,
                rows: 30,
                scheme: Default::default(),
            });
            call(Request::Edit {
                request: None,
                edit: Edit::CreateTab {
                    tab: Tab {
                        id: format!("tab-{tid}"),
                        space: space.clone(),
                        default_task: None,
                        title: tid.into(),
                        layout: Some(Layout::Leaf(tid.into())),
                    },
                },
            });
        };
        let bound = |cli: &str, tid: &str| {
            workspace().agent_sessions.iter().any(|s| {
                s.cli_session_id == cli && s.current && s.terminal.as_deref() == Some(tid)
            })
        };
        let shot = |name: &str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh();
                w.draw(a).clear(a);
                w.render_to_image().unwrap().save(directory.join(name)).unwrap();
            })
            .unwrap();
        };
        let (one, two, three, four) = (session(1), session(2), session(3), session(4));
        launch("shell", None);
        launch("click", Some(&one));
        launch("show", Some(&two));
        launch("asked", Some(&four));
        launch("done", Some(&three));
        wait(500).await;
        handle.update(cx, |v, _, a| v.visual_peek_show_tab("shell", a)).unwrap();
        for _ in 0..120 {
            if bound(&one, "click")
                && bound(&two, "show")
                && bound(&three, "done")
                && bound(&four, "asked")
            {
                break;
            }
            wait(500).await;
        }
        assert!(bound(&three, "done") && bound(&four, "asked"), "stand-ins never bound");
        assert!(bound(&one, "click") && bound(&two, "show"), "stand-ins never bound");
        // Marked done while they ran, as the 标记完成 of a pane that then stays behind is.
        for cli in [&three, &four] {
            let w = workspace();
            let s = w.agent_sessions.iter().find(|s| &s.cli_session_id == cli).unwrap();
            call(Request::Edit {
                request: None,
                edit: Edit::SetOutcome {
                    seen: None,
                    id: s.id.clone(),
                    outcome: Outcome::Completed,
                },
            });
        }

        // The host dies for real; the app restarts it and the panes come back lost.
        let killed = std::process::Command::new("/usr/bin/pkill")
            .args(["-9", "-f", &format!("fibo serve --runtime {runtime}")])
            .status()
            .unwrap();
        assert!(killed.success(), "no host to kill");
        for _ in 0..40 {
            wait(500).await;
            if handle.update(cx, |v, _, _| v.visual_cut_off()).unwrap() {
                break;
            }
        }
        handle.update(cx, |v, _, a| v.visual_reconnect(a)).unwrap();
        // The window's own view, not the host's: it acts on what it has received. The shown
        // shell reopens by itself first, and nothing else may start while that runs.
        let mut back = false;
        for n in 0..80 {
            wait(500).await;
            // A killed host can come back at or below the revision this window saw, which
            // the window ignores: after 10 s of that, edits walk the host past it.
            let host = rpc(&socket, &Request::Workspace).ok().and_then(|r| match r {
                Reply::Workspace(w) => Some(w.revision),
                _ => None,
            });
            let seen = handle.update(cx, |v, _, _| v.visual_revision()).unwrap();
            if n % 5 == 0 {
                eprintln!("resume native: try {n}: host revision {host:?}, window {seen}");
            }
            if n >= 20 {
                let _ = rpc(
                    &socket,
                    &Request::Edit {
                        request: None,
                        edit: Edit::RenameTab {
                            id: "tab-click".into(),
                            title: format!("click {n}"),
                        },
                    },
                );
            }
            back = handle
                .update(cx, |v, _, _| v.visual_panes_lost(&["click", "show", "done", "asked"], "tab-shell", "shell"))
                .unwrap();
            if back {
                break;
            }
        }
        assert!(back, "the panes never came back as lost");
        wait(800).await;
        shot("1-before.png", cx);
        let before = workspace().tabs.len();

        // 1. The click: back into the lost pane's own tab.
        handle.update(cx, |v, w, a| v.visual_open_cli_session(&one, w, a)).unwrap();
        let resumes = |w: &Workspace, cli: &str| {
            w.terminals
                .iter()
                .filter(|t| t.state == ProcessState::Running && t.argv.join(" ").contains(cli))
                .filter(|t| t.argv.iter().any(|a| a.contains("--resume")))
                .map(|t| t.id.clone())
                .collect::<Vec<_>>()
        };
        for _ in 0..40 {
            wait(250).await;
            if !resumes(&workspace(), &one).is_empty() {
                break;
            }
        }
        wait(500).await;
        let w = workspace();
        let started = resumes(&w, &one);
        assert_eq!(started.len(), 1, "expected one resume of the clicked session: {started:?}");
        assert_eq!(w.tabs.len(), before, "the click opened another tab");
        assert_eq!(w.tab_terminals("tab-click"), started, "resumed outside its own tab");
        assert_eq!(handle.update(cx, |v, _, _| v.visual_shown_tab()).unwrap().as_deref(), Some("tab-click"));
        shot("2-clicked-resumed-in-place.png", cx);
        eprintln!("resume native: click reopens in the lost pane's tab PASS");

        // 2. Already running elsewhere: showing the lost pane must not start it again.
        launch("again", Some(&two));
        for _ in 0..120 {
            if bound(&two, "again") {
                break;
            }
            wait(500).await;
        }
        assert!(bound(&two, "again"), "the second stand-in never bound");
        wait(500).await;
        handle.update(cx, |v, _, a| v.visual_peek_show_tab("show", a)).unwrap();
        for _ in 0..40 {
            wait(250).await;
            if !workspace().tabs.iter().any(|t| t.id == "tab-show") {
                break;
            }
        }
        wait(1500).await;
        let w = workspace();
        assert!(resumes(&w, &two).is_empty(), "started the running conversation a second time");
        assert!(!w.tabs.iter().any(|t| t.id == "tab-show"), "the dead tab stayed behind");
        assert_eq!(handle.update(cx, |v, _, _| v.visual_shown_tab()).unwrap().as_deref(), Some("tab-again"));
        assert!(bound(&two, "again"), "the running conversation lost its terminal");
        shot("3-dead-tab-gone.png", cx);
        eprintln!("resume native: lost pane of a running session goes away PASS");

        // 3. Marked done: showing its lost pane must not bring it back.
        handle.update(cx, |v, _, a| v.visual_peek_show_tab("done", a)).unwrap();
        for _ in 0..40 {
            wait(250).await;
            if !workspace().tabs.iter().any(|t| t.id == "tab-done") {
                break;
            }
        }
        wait(1500).await;
        let w = workspace();
        assert!(resumes(&w, &three).is_empty(), "started a conversation marked done");
        assert!(!w.tabs.iter().any(|t| t.id == "tab-done"), "the dead tab stayed behind");
        assert_eq!(handle.update(cx, |v, _, _| v.visual_shown_tab()).unwrap().as_deref(), Some("tab-again"));
        shot("4-done-tab-gone.png", cx);
        eprintln!("resume native: lost pane of a done session goes away PASS");

        // 4. Its row clicked: you asked, so it resumes in place.
        let before = workspace().tabs.len();
        handle.update(cx, |v, w, a| v.visual_open_cli_session(&four, w, a)).unwrap();
        for _ in 0..40 {
            wait(250).await;
            if !resumes(&workspace(), &four).is_empty() {
                break;
            }
        }
        wait(500).await;
        let w = workspace();
        let started = resumes(&w, &four);
        assert_eq!(started.len(), 1, "expected one resume of the clicked done session: {started:?}");
        assert_eq!(w.tabs.len(), before, "the click opened another tab");
        assert_eq!(w.tab_terminals("tab-asked"), started, "resumed outside its own tab");
        assert_eq!(handle.update(cx, |v, _, _| v.visual_shown_tab()).unwrap().as_deref(), Some("tab-asked"));
        shot("5-done-clicked-resumed.png", cx);
        eprintln!("resume native: clicking a done session resumes it in place PASS");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
