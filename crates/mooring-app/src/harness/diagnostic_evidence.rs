use super::*;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_DIAGNOSTIC_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            let pane = handle.update(cx, |view, _, _| view.visual_shown_pane()).unwrap();
            pane.update(cx, |pane, cx| pane.visual_type("/usr/bin/python3 -c 'b=b\"x\"*(1536<<20);exec(\"while 1: pass\")'\r", cx));
            cx.background_executor().timer(Duration::from_secs(1)).await;
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh(); window.draw(app).clear(app);
                window.render_to_image().unwrap().save(directory.join("entry.png")).unwrap();
                let at = gpui::point(px(theme::Theme::SIDEBAR / 2.), window.viewport_size().height - px(theme::Theme::S8 + theme::Theme::ROW / 2.));
                native_click(window, app, at, gpui::MouseButton::Left);
            }).unwrap();
            cx.background_executor().timer(Duration::from_millis(500)).await;
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh(); window.draw(app).clear(app);
                window.render_to_image().unwrap().save(directory.join("loading.png")).unwrap();
                window.dispatch_event(gpui::KeyDownEvent { keystroke: gpui::Keystroke::parse("escape").unwrap(), is_held: false, prefer_character_input: false }.to_platform_input(), app);
            }).unwrap();
            assert!(handle.update(cx, |view, _, _| view.visual_diagnostic_closed()).unwrap());
            cx.background_executor().timer(Duration::from_millis(300)).await;
            handle.update(cx, |view, window, cx| view.visual_open_diagnostics(window, cx)).unwrap();
            // Mid-wait: the terminal lines, the spinner and the countdown.
            cx.background_executor().timer(Duration::from_secs(3)).await;
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh(); window.draw(app).clear(app);
                window.render_to_image().unwrap().save(directory.join("waiting.png")).unwrap();
            }).unwrap();
            cx.background_executor().timer(Duration::from_secs(15)).await;
            let report = handle.update(cx, |view, _, cx| {
                let report = view.visual_diagnostic_report(cx).expect("missing native diagnostic report");
                assert!(report.complete && report.process_count > 0, "{report:?}");
                assert!(report.memory.is_some_and(|m| m.used_bytes > 0), "memory unread");
                assert!(report.apps.iter().any(|app| app.cpu_percent > 0.));
                assert!(report.apps.iter().any(|app| !app.terminals.is_empty() && app.cpu_percent > 5.));
                assert!(report.memory.is_some_and(|m| m.pressure.is_some() && m.wired_bytes > 0), "pressure or wired unread");
                assert!(report.terminals.iter().any(|t| t.memory_bytes > 1 << 30), "the 1.5 GB terminal is missing: {:?}", report.terminals);
                view.visual_copy_diagnostics(cx);
                report
            }).unwrap();
            std::fs::write(directory.join("report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh(); window.draw(app).clear(app);
                window.render_to_image().unwrap().save(directory.join("complete.png")).unwrap();
                // The bottom of the body: the ranking and the right column should end together.
                let center = gpui::point(window.viewport_size().width / 2., window.viewport_size().height / 2.);
                window.dispatch_event(gpui::ScrollWheelEvent { position: center, delta: gpui::ScrollDelta::Lines(gpui::point(0., -200.)), modifiers: Default::default(), touch_phase: gpui::TouchPhase::Moved }.to_platform_input(), app);
            }).unwrap();
            cx.background_executor().timer(Duration::from_millis(300)).await;
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh(); window.draw(app).clear(app);
                window.render_to_image().unwrap().save(directory.join("complete-bottom.png")).unwrap();
            }).unwrap();
            // Under pressure the panel turns its memory side forward.
            handle.update(cx, |view, _, cx| view.visual_tighten_diagnostic_memory(cx)).unwrap();
            cx.update_window(handle.into(), |_, window, app| {
                let center = gpui::point(window.viewport_size().width / 2., window.viewport_size().height / 2.);
                window.dispatch_event(gpui::ScrollWheelEvent { position: center, delta: gpui::ScrollDelta::Lines(gpui::point(0., 200.)), modifiers: Default::default(), touch_phase: gpui::TouchPhase::Moved }.to_platform_input(), app);
            }).unwrap();
            cx.background_executor().timer(Duration::from_millis(300)).await;
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh(); window.draw(app).clear(app);
                window.render_to_image().unwrap().save(directory.join("pressure-warn.png")).unwrap();
            }).unwrap();
            handle.update(cx, |view, _, cx| view.visual_open_diagnostic_terminal(cx)).unwrap();
            assert!(handle.update(cx, |view, _, _| view.visual_diagnostic_closed()).unwrap());
            pane.update(cx, |pane, cx| pane.visual_type("\x03", cx));
            eprintln!("diagnostic native PASS: loading, Escape cancellation, restart, real complete report, clipboard readback, managed terminal navigation");
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
