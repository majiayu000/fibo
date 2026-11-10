use super::*;

// A stranger's first five minutes, run against a fresh HOME and runtime: what the window
// shows on first launch, once the login shell and hooks are checked, after 「启动 Claude Code」
// (which writes its hook unless `MOORING_FIRST_RUN_NO_HOOK` clears the box first), what the
// terminal then says about `mooring` and the hooks, and 设置 › 通用.
// `MOORING_FIRST_RUN_AGENT` names the CLI to start (default claude).
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_FIRST_RUN_EVIDENCE").is_none() {
        return false;
    }
    let agent = std::env::var("MOORING_FIRST_RUN_AGENT").unwrap_or_else(|_| "claude".into());
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
        std::fs::create_dir_all(&directory).unwrap();
        let shot = |name: &str, window: &mut gpui::Window, app: &mut App| {
            window.refresh();
            window.draw(app).clear(app);
            let image = window.render_to_image().unwrap();
            image.save(directory.join(format!("{name}.png"))).unwrap();
        };
        let take = |name: &'static str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, window, app| shot(name, window, app))
                .unwrap();
        };
        wait(800).await;
        handle
            .update(cx, |_, window, cx| {
                window.resize(size(
                    px(theme::Theme::WINDOW_WIDTH),
                    px(theme::Theme::WINDOW_HEIGHT),
                ));
                window.bounds_changed(cx);
            })
            .unwrap();
        wait(400).await;
        take("1-launch", cx);
        let greeted = handle.update(cx, |v, _, _| v.visual_first_run()).unwrap();
        eprintln!("first run shown: {greeted}");
        // The login shell probe allows itself 5 s.
        wait(6000).await;
        take("2-checked", cx);
        if greeted {
            // 「启动时一起装上」 is ticked unless the run asks for it cleared.
            if std::env::var_os("MOORING_FIRST_RUN_NO_HOOK").is_some() {
                handle
                    .update(cx, |v, _, cx| v.visual_first_run_tick(cx))
                    .unwrap();
                wait(400).await;
                take("3-unticked", cx);
            }
            handle
                .update(cx, |v, _, cx| v.visual_first_run_start(&agent, cx))
                .unwrap();
        }
        wait(8000).await;
        take("4-agent", cx);
        // The Agent keeps its terminal; the checks go in a new tab's shell.
        handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
        wait(3000).await;
        let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
        pane.update(cx, |p, cx| {
            p.visual_type(
                "clear; command -v mooring claude codex; fibo hooks status; fibo list\r",
                cx,
            )
        });
        wait(4000).await;
        take("5-terminal", cx);
        // Where the hook is installed and taken out afterwards.
        handle
            .update(cx, |v, window, cx| v.show_settings(window, cx))
            .unwrap();
        wait(2000).await;
        take("6-settings", cx);
        // And how it is found again without knowing where it is.
        handle
            .update(cx, |v, window, cx| {
                v.visual_first_run_palette("hook", window, cx)
            })
            .unwrap();
        wait(1000).await;
        take("7-palette", cx);
        let still = handle.update(cx, |v, _, _| v.visual_first_run()).unwrap();
        eprintln!("first run still shown at the end: {still}");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
