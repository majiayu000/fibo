use super::*;

// Pane decorations: a shell in a repo with one uncommitted file shows the example
// plugin's 改动 1 in its title bar and the last commit in the hover card; after `cd`
// out of the repo both go; back in, a second file shows as 改动 2 once it is due.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && let Ok(repo) = std::env::var("MOORING_DECORATION_EVIDENCE")
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, window, app| {
                    window.refresh();
                    window.draw(app).clear(app);
                    window.render_to_image().unwrap().save(directory.join(name)).unwrap();
                })
                .unwrap();
            };
            // Each look draws a frame of what changed, as a shown window does on output; a
            // background window with the idle display link paused draws none on its own.
            let lines = |cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, window, app| window.draw(app).clear(app))
                    .unwrap();
                handle.update(cx, |v, _, cx| v.visual_decorations(cx)).unwrap()
            };
            // Asks until the lines settle on `want`: the first look starts the run.
            async fn until<F: std::future::Future>(
                want: impl Fn(&(Vec<String>, Vec<String>)) -> bool,
                lines: impl Fn(&mut gpui::AsyncApp) -> (Vec<String>, Vec<String>),
                wait: impl Fn(u64) -> F,
                secs: u64,
                cx: &mut gpui::AsyncApp,
            ) -> (Vec<String>, Vec<String>) {
                let mut seen = lines(cx);
                for _ in 0..secs * 4 {
                    if want(&seen) {
                        break;
                    }
                    wait(250).await;
                    seen = lines(cx);
                }
                seen
            }
            std::fs::create_dir_all(&directory).unwrap();
            wait(2000).await;
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            wait(1500).await;
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            pane.update(cx, |pane, cx| pane.visual_type(&format!("cd '{repo}'\r"), cx));
            let seen = until(|l| !l.0.is_empty() && l.1.len() == 2, lines, &wait, 5, cx).await;
            eprintln!("in the repo: {seen:?}");
            assert_eq!(seen.0, ["改动 1"], "the cd succeeded: no failure shown");
            assert!(seen.1[0].starts_with("最近提交 ") && seen.1[0].ends_with("：初始提交"));
            assert!(seen.1[1].starts_with("上一条命令用时 ") && seen.1[1].ends_with("退出码 0"));
            shot("decorated.png", cx);
            pane.update(cx, |pane, cx| pane.visual_type("cd /tmp\r", cx));
            let seen = until(|l| l.0.is_empty() && l.1.len() == 1, lines, &wait, 10, cx).await;
            let cwd = pane.update(cx, |p, _| p.cwd().map(|c| c.display().to_string()));
            eprintln!("in /tmp ({cwd:?}): {seen:?}");
            assert!(seen.0.is_empty(), "outside a repo no git line shows");
            assert!(seen.1.len() == 1 && seen.1[0].starts_with("上一条命令用时 "));
            shot("outside.png", cx);
            std::fs::write(std::path::Path::new(&repo).join("second.txt"), "").unwrap();
            pane.update(cx, |pane, cx| pane.visual_type(&format!("cd '{repo}'\r"), cx));
            // The cached 改动 1 shows at once and stays until the run due after 10 s.
            let seen = until(|l| l.0 == ["改动 2"], lines, &wait, 15, cx).await;
            let cwd = pane.update(cx, |p, _| p.cwd().map(|c| c.display().to_string()));
            eprintln!("back, a file later ({cwd:?}): {seen:?}");
            assert_eq!(seen.0, ["改动 2"]);
            shot("refreshed.png", cx);
            // The last command's line is due only every 300 s: a new command runs it at once.
            pane.update(cx, |pane, cx| pane.visual_type("sh -c 'exit 3'\r", cx));
            let seen = until(|l| l.0.len() == 2, lines, &wait, 5, cx).await;
            eprintln!("a failed command: {seen:?}");
            assert_eq!(seen.0, ["✗ 退出 3", "改动 2"]);
            assert!(seen.1[1].ends_with("退出码 3"));
            shot("failed.png", cx);
            pane.update(cx, |pane, cx| pane.visual_type("true\r", cx));
            let seen = until(|l| l.0.len() == 1, lines, &wait, 5, cx).await;
            eprintln!("then one that worked: {seen:?}");
            assert_eq!(seen.0, ["改动 2"]);
            shot("recovered.png", cx);
            // Two cds a second apart: the second lands inside the directory ask's throttle,
            // and the shell then prints nothing more that would ask again.
            pane.update(cx, |pane, cx| pane.visual_type("cd /tmp\r", cx));
            wait(1000).await;
            pane.update(cx, |pane, cx| pane.visual_type(&format!("cd '{repo}'\r"), cx));
            let seen = until(|l| l.0 == ["改动 2"], lines, &wait, 10, cx).await;
            let cwd = pane.update(cx, |p, _| p.cwd().map(|c| c.display().to_string()));
            eprintln!("then a quick hop out and back ({cwd:?}): {seen:?}");
            assert_eq!(seen.0, ["改动 2"], "the pane follows a cd made inside the throttle");
            eprintln!(
                "decoration native PASS: title and hover lines from the plugin, gone outside a repo, refreshed when due, the last command's failure shown and cleared"
            );
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
