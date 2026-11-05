use super::*;

// Reconnect: a host killed outright shows the strip after its grace period, and
// 「重新连接」 starts a host the status link then resubscribes to.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_RECONNECT_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, window, app| { window.refresh(); window.draw(app).clear(app); window.render_to_image().unwrap().save(directory.join(name)).unwrap(); }).unwrap();
            };
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            assert!(!handle.update(cx, |view, _, _| view.visual_cut_off()).unwrap());
            shot("connected.png", cx);
            let socket = handle.update(cx, |view, _, _| view.visual_socket()).unwrap();
            let runtime = socket.parent().unwrap().display().to_string();
            let killed = std::process::Command::new("/usr/bin/pkill").args(["-9", "-f", &format!("mooring serve --runtime {runtime}")]).status().unwrap();
            assert!(killed.success(), "no host to kill");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            assert!(!handle.update(cx, |view, _, _| view.visual_cut_off()).unwrap(), "strip before the grace period");
            cx.background_executor().timer(Duration::from_secs(3)).await;
            assert!(handle.update(cx, |view, _, _| view.visual_cut_off()).unwrap(), "no strip after the host died");
            shot("cut-off.png", cx);
            handle.update(cx, |view, _, cx| view.visual_reconnect(cx)).unwrap();
            cx.background_executor().timer(Duration::from_millis(200)).await;
            shot("reconnecting.png", cx);
            let mut back = false;
            for _ in 0..40 {
                cx.background_executor().timer(Duration::from_millis(500)).await;
                if !handle.update(cx, |view, _, _| view.visual_cut_off()).unwrap() { back = true; break; }
            }
            assert!(back, "strip still shown after 重新连接");
            cx.background_executor().timer(Duration::from_millis(500)).await;
            shot("reconnected.png", cx);
            eprintln!("reconnect native PASS: no strip within grace, strip after host kill, 重新连接 restarts host, strip clears");
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
