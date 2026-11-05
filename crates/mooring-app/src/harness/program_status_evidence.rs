use super::*;

// A shell command that reports its own status (OSC 7501): the sidebar row and the
// pane show 在跑 30%, then 等你回答 with the program's question.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_PROGRAM_STATUS_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            wait(2000).await;
            std::fs::create_dir_all(&directory).unwrap();
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            // "部署中" and "要部署到生产吗？" in base64.
            pane.update(cx, |p, cx| {
                p.visual_type(
                    "clear; printf '\\033]7501;state=working:app=deploy:progress=30:msg=6YOo572y5Lit\\033\\\\'; sleep 3; printf '\\033]7501;state=blocked:kind=question:app=deploy:msg=6KaB6YOo572y5Yiw55Sf5Lqn5ZCX77yf\\033\\\\'; sleep 30\r",
                    cx,
                )
            });
            let shot = |name: &str, window: &mut gpui::Window, app: &mut App| {
                window.refresh();
                window.draw(app).clear(app);
                window
                    .render_to_image()
                    .unwrap()
                    .save(directory.join(format!("{name}.png")))
                    .unwrap();
            };
            wait(1500).await;
            cx.update_window(handle.into(), |_, window, app| shot("0-working", window, app))
                .unwrap();
            wait(3000).await;
            cx.update_window(handle.into(), |_, window, app| shot("1-blocked", window, app))
                .unwrap();
            eprintln!("program status native: screenshots in {}", directory.display());
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
