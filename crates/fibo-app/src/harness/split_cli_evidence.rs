use super::*;

// `fibo split` / `close` from outside the app, as another Agent runs them: the
// shown tab gains the pane without the keyboard leaving the user's pane, the command
// runs in it, and closing it takes the pane away again.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_SPLIT_CLI_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |ms| executor.timer(Duration::from_millis(ms));
            wait(1500).await;
            std::fs::create_dir_all(&directory).unwrap();
            let socket = handle.update(cx, |v, _, _| v.visual_socket()).unwrap();
            let root = socket.parent().unwrap().to_owned();
            let call = |r: Request| rpc(&socket, &r).unwrap();
            let workspace = || match call(Request::Workspace) { Reply::Workspace(w) => w, x => panic!("{x:?}") };
            let w = workspace();
            let space = match w.spaces.first() {
                Some(s) => s.id.clone(),
                None => {
                    call(Request::Edit { request: None, edit: Edit::AddSpace(Space { id: "s".into(), name: "Tests".into(), cwd: "/tmp".into() }) });
                    "s".into()
                }
            };
            call(Request::Launch { colors: None, id: "base".into(), space: space.clone(), argv: vec!["/bin/sh".into(), "-c".into(), "printf '~/mooring $ 我在这块打字\\n'; exec /bin/sh -i".into()], cwd: "/tmp".into(), cols: 100, rows: 30, scheme: Default::default() });
            call(Request::Edit { request: None, edit: Edit::CreateTab { tab: Tab { id: "tab-base".into(), space, default_task: None, title: "base".into(), layout: Some(Layout::Leaf("base".into())) } } });
            wait(500).await;
            handle.update(cx, |v, _, a| v.visual_peek_show_tab("base", a)).unwrap();
            wait(1000).await;
            let (order, focused) = handle.update(cx, |v, w, a| v.visual_pane_focus(w, a)).unwrap();
            assert_eq!((order, focused.as_deref()), (vec!["base".to_owned()], Some("base")));
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh(); w.draw(a).clear(a);
                    w.render_to_image().unwrap().save(directory.join(name)).unwrap();
                }).unwrap();
            };
            shot("0-before.png", cx);
            let cli = std::env::current_exe().unwrap().with_file_name("fibo");
            let background = cx.background_executor().clone();
            let run = |args: Vec<String>| {
                let cli = cli.clone();
                let root = root.clone();
                background.spawn(async move {
                    let out = std::process::Command::new(cli).arg(&args[0]).arg("--runtime").arg(root).args(&args[1..]).env("SHELL", "/bin/sh").output().unwrap();
                    (out.status.success(), String::from_utf8_lossy(&out.stdout).trim().to_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
                })
            };
            let ok = |r: (bool, String, String)| { assert!(r.0, "{}", r.2); r.1 };
            let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            let id = ok(run(args(&["split", "base", "--down", "--size", "0.35", "--", "echo split-from-cli-$((6*7))"])).await);
            let mut shown = (vec![], None);
            for _ in 0..50 {
                wait(100).await;
                shown = handle.update(cx, |v, w, a| v.visual_pane_focus(w, a)).unwrap();
                if shown.0.len() == 2 { break; }
            }
            assert_eq!(shown.0, vec!["base".to_owned(), id.clone()], "the split never showed");
            assert_eq!(shown.1.as_deref(), Some("base"), "the split took the keyboard");
            let screen = || match call(Request::ReadScreen { id: id.clone(), rows: 30 }) { Reply::Text(t) => t, x => panic!("{x:?}") };
            let mut ran = false;
            for _ in 0..50 {
                wait(100).await;
                if screen().contains("split-from-cli-42") { ran = true; break; }
            }
            assert!(ran, "the command never ran: {}", screen());
            wait(500).await;
            shot("1-split.png", cx);
            ok(run(args(&["close", &id])).await);
            for _ in 0..50 {
                wait(100).await;
                shown = handle.update(cx, |v, w, a| v.visual_pane_focus(w, a)).unwrap();
                if shown.0.len() == 1 { break; }
            }
            assert_eq!(shown, (vec!["base".to_owned()], Some("base".to_owned())), "close left the pane");
            wait(300).await;
            shot("2-closed.png", cx);
            // Splits of panes that are splits already, in both directions and on both sides.
            let shown_after = async |cx: &mut gpui::AsyncApp, count: usize| {
                let mut shown = (vec![], None);
                for _ in 0..50 {
                    wait(100).await;
                    shown = handle.update(cx, |v, w, a| v.visual_pane_focus(w, a)).unwrap();
                    if shown.0.len() == count { break; }
                }
                // Let the panes report their new sizes before the next split is judged.
                wait(400).await;
                shown
            };
            let a = ok(run(args(&["split", "base", "--", "echo pane-a"])).await);
            shown_after(cx, 2).await;
            let b = ok(run(args(&["split", &a, "--down", "--", "echo pane-b"])).await);
            shown_after(cx, 3).await;
            let c = ok(run(args(&["split", "base", "--down", "--", "echo pane-c"])).await);
            let shown = shown_after(cx, 4).await;
            assert_eq!(shown.0, vec!["base".to_owned(), c.clone(), a.clone(), b.clone()], "four panes out of order");
            assert_eq!(shown.1.as_deref(), Some("base"), "a split took the keyboard");
            shot("3-four-panes.png", cx);
            // A fifth and sixth go to the side tab: the shown tab, its four panes and the
            // keyboard stay as they are, and the user's pane keeps the top-left corner.
            let (done, fifth, why) = run(args(&["split", &b, "--", "echo pane-side-1"])).await;
            assert!(done && why.contains("已经有 4 块") && why.contains("终端的旁屏"), "{why}");
            let (done, sixth, _) = run(args(&["split", "base", "--", "echo pane-side-2"])).await;
            assert!(done);
            wait(800).await;
            let still = handle.update(cx, |v, w, a| v.visual_pane_focus(w, a)).unwrap();
            assert_eq!(still.0.len(), 4, "the shown tab changed");
            assert_eq!(still.0[0], "base", "the user's pane left the top left");
            assert_eq!(still.1.as_deref(), Some("base"), "a side pane took the keyboard");
            let titles = handle.update(cx, |v, _, _| v.visual_tab_titles()).unwrap();
            assert!(titles.iter().any(|t| t == "终端的旁屏"), "no side tab: {titles:?}");
            shot("4-side-tab-made.png", cx);
            handle.update(cx, |v, _, a| v.visual_peek_show_tab("终端的旁屏", a)).unwrap();
            let side = shown_after(cx, 2).await;
            assert_eq!(side.0, vec![fifth.clone(), sixth.clone()], "the side tab's panes");
            shot("4b-side-tab.png", cx);
            handle.update(cx, |v, _, a| v.visual_peek_show_tab("base", a)).unwrap();
            shown_after(cx, 4).await;
            let full = (vec!["base".to_owned(), c, a, b], ());
            // A tab that is not shown gets its pane without the shown tab changing.
            call(Request::Launch { colors: None, id: "hidden".into(), space: workspace().spaces[0].id.clone(), argv: vec!["/bin/sh".into(), "-i".into()], cwd: "/tmp".into(), cols: 100, rows: 30, scheme: Default::default() });
            call(Request::Edit { request: None, edit: Edit::CreateTab { tab: Tab { id: "tab-other".into(), space: workspace().spaces[0].id.clone(), default_task: None, title: "other".into(), layout: Some(Layout::Leaf("hidden".into())) } } });
            wait(500).await;
            ok(run(args(&["split", "hidden", "--", "echo pane-hidden"])).await);
            wait(800).await;
            let row = handle.update(cx, |v, _, _| v.visual_tab_row()).unwrap();
            assert_eq!((row.1.as_deref(), row.2), (Some("tab-base"), full.0.len()), "the shown tab changed");
            handle.update(cx, |v, _, a| v.visual_peek_show_tab("other", a)).unwrap();
            let other = shown_after(cx, 2).await;
            assert_eq!(other.0.len(), 2, "the hidden tab's split is missing");
            shot("5-other-tab.png", cx);
            eprintln!("split cli native PASS: an outside split shows in the open tab without taking focus, runs its command, and closes");
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
