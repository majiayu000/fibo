use super::*;

// A notification clicked before its session loaded: opened once it loads, unless the
// user went to another page first.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_ROUTE_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
            let frame = |cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, window, app| window.draw(app).clear(app))
                    .unwrap();
            };
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, window, app| {
                    window.refresh();
                    window.draw(app).clear(app);
                    window.render_to_image().unwrap().save(directory.join(name)).unwrap();
                })
                .unwrap();
            };
            std::fs::create_dir_all(&directory).unwrap();
            let mut ids = vec![];
            for _ in 0..80 {
                wait(250).await;
                ids = handle.update(cx, |v, _, _| v.visual_session_ids()).unwrap();
                if !ids.is_empty() {
                    break;
                }
            }
            let id = ids.first().cloned().expect("the fixture's session loaded");
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            wait(1500).await;
            let said = handle
                .update(cx, |v, window, cx| v.visual_route_before_load(&id, false, window, cx))
                .unwrap();
            eprintln!("clicked before load: {said:?}");
            assert_eq!(said.as_deref(), Some("会话还没载入，载入后自动打开"));
            shot("waiting.png", cx);
            let mut state = (false, None, None);
            for _ in 0..20 {
                wait(250).await;
                frame(cx);
                state = handle.update(cx, |v, _, _| v.visual_route_state()).unwrap();
                if state.1.as_deref() == Some(id.as_str()) {
                    break;
                }
            }
            eprintln!("once loaded: {state:?}");
            assert_eq!(state, (true, Some(id.clone()), None), "opened once loaded");
            shot("opened.png", cx);
            handle
                .update(cx, |v, window, cx| v.visual_route_before_load(&id, true, window, cx))
                .unwrap();
            for _ in 0..12 {
                wait(250).await;
                frame(cx);
            }
            let state = handle.update(cx, |v, _, _| v.visual_route_state()).unwrap();
            eprintln!("user moved first: {state:?}");
            assert_eq!(state, (true, None, None), "the user's own page wins");
            shot("moved.png", cx);
            eprintln!(
                "route native PASS: a session clicked before it loaded opens once loaded; a page the user picked meanwhile wins"
            );
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
