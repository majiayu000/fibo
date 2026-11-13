use super::*;

// MOORING_OSC7_EVIDENCE names a directory reached through a symlink: only the
// shell's OSC 7 report keeps that spelling, the process directory resolves it.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(_), Some(dir)) = (
        &handle,
        evidence.clone(),
        std::env::var("MOORING_OSC7_EVIDENCE").ok(),
    ) {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            wait(2000).await;
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            wait(4000).await;
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            pane.update(cx, |pane, cx| {
                pane.visual_type(&format!("cd '{dir}'\r"), cx)
            });
            wait(1500).await;
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            wait(2000).await;
            let cwd = handle.update(cx, |v, _, _| v.visual_shown_cwd()).unwrap();
            eprintln!("osc7: new tab cwd={cwd}, typed cd {dir}");
            assert_eq!(cwd, dir, "a new tab opens where the shell says it is");
            eprintln!(
                "osc7 native PASS: new tab took the shell's OSC 7 directory, symlink spelling kept"
            );
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
