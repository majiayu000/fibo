use super::*;

// Runs the top bar's 「重启后台」 for real, as confirming its dialog does.
pub(super) fn start(handle: Option<WindowHandle<workbench::Workbench>>, cx: &mut App) {
    if let Some(handle) = &handle
        && std::env::var_os("MOORING_RESTART_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            handle
                .update(cx, |view, _, cx| view.visual_restart_host(cx))
                .unwrap();
        })
        .detach();
    }
}
