use super::*;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(_)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_TAB_CWD_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            wait(2000).await;
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            wait(1500).await;
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            pane.update(cx, |pane, cx| pane.visual_type("cd /tmp\r", cx));
            wait(800).await;
            let (before, _) = handle.update(cx, |v, _, _| v.visual_tabs()).unwrap();
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            wait(1500).await;
            let (after, _) = handle.update(cx, |v, _, _| v.visual_tabs()).unwrap();
            let cwd = handle.update(cx, |v, _, _| v.visual_shown_cwd()).unwrap();
            eprintln!("tab cwd: tabs {before}->{after}, new tab cwd={cwd}");
            assert_eq!(after, before + 1, "a new tab opened");
            assert_eq!(
                std::fs::canonicalize(&cwd).unwrap(),
                std::fs::canonicalize("/tmp").unwrap(),
                "a new tab follows the focused shell's live directory"
            );
            eprintln!(
                "tab cwd native PASS: new tab opens in the directory the focused shell cd'd into"
            );
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
