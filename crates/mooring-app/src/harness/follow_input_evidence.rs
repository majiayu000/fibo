use super::*;

// Scrolls a full shell back with the wheel, then types: the view returns to the
// input line for a real key event and for committed text alike.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_FOLLOW_INPUT_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            let pane = handle.update(cx, |view, _, _| view.visual_shown_pane()).unwrap();
            pane.update(cx, |p, cx| p.visual_type("clear; seq 1 400\r", cx));
            for _ in 0..30 {
                cx.background_executor().timer(Duration::from_millis(300)).await;
                if pane.update(cx, |p, _| p.visual_text()).contains("400") { break; }
            }
            let center = pane.read_with(cx, |p, _| p.visual_mouse_app().1).expect("pane laid out");
            let scroll_up = |window: &mut gpui::Window, app: &mut App| {
                window.dispatch_event(gpui::ScrollWheelEvent {
                    position: center,
                    delta: gpui::ScrollDelta::Lines(gpui::point(0., 60.)),
                    modifiers: Default::default(),
                    touch_phase: gpui::TouchPhase::Moved,
                }.to_platform_input(), app);
                window.refresh(); window.draw(app).clear(app);
            };
            for (name, key) in [("key", Some("left")), ("text", None)] {
                cx.update_window(handle.into(), |_, window, app| {
                    scroll_up(window, app);
                    window.render_to_image().unwrap().save(directory.join(format!("{name}-scrolled.png"))).unwrap();
                }).unwrap();
                assert!(!pane.read_with(cx, |p, _| p.visual_at_bottom()), "{name}: wheel did not leave the bottom");
                match key {
                    Some(key) => cx.update_window(handle.into(), |_, window, app| {
                        window.dispatch_event(gpui::KeyDownEvent { keystroke: gpui::Keystroke::parse(key).unwrap(), is_held: false, prefer_character_input: false }.to_platform_input(), app);
                    }).unwrap(),
                    None => pane.update(cx, |p, cx| p.visual_type("x", cx)),
                }
                cx.background_executor().timer(Duration::from_millis(300)).await;
                assert!(pane.read_with(cx, |p, _| p.visual_at_bottom()), "{name}: input did not return to the bottom");
                cx.update_window(handle.into(), |_, window, app| {
                    window.refresh(); window.draw(app).clear(app);
                    window.render_to_image().unwrap().save(directory.join(format!("{name}-typed.png"))).unwrap();
                }).unwrap();
            }
            pane.update(cx, |p, cx| p.visual_type("\x15", cx));
            eprintln!("follow input native PASS: wheel leaves the bottom; a key event and typed text both return to the input line");
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
