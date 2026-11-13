use super::*;

// A real terminal with a program holding the mouse: Cmd+C with no selection leaves the
// clipboard alone and says so, a plain drag goes to the program and shows the hint, a
// Shift-drag selects in Fibo and copies, with both notes up.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_MOUSE_TAKEN_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            let before = cx.update(|cx| cx.read_from_clipboard());
            let pane = handle.update(cx, |view, _, _| view.visual_shown_pane()).unwrap();
            pane.update(cx, |p, cx| p.visual_type("printf '\\033[?1000;1002;1006h'; clear; seq 1 20; cat >/dev/null\r", cx));
            for _ in 0..150 {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if pane.update(cx, |p, _| p.visual_text()).lines().any(|l| l.trim() == "20") { break; }
            }
            cx.background_executor().timer(Duration::from_millis(300)).await;
            assert!(pane.read_with(cx, |p, _| p.takes_mouse()), "the program took the mouse");
            let shot = |name: &str, window: &mut gpui::Window, app: &mut App| {
                window.refresh(); window.draw(app).clear(app);
                window.render_to_image().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
            };
            let clipboard = |cx: &mut gpui::AsyncApp| cx.update(|cx| cx.read_from_clipboard().and_then(|c| c.text()));
            cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("the program copied this".into())));

            pane.update(cx, |p, cx| p.copy(cx));
            assert_eq!(clipboard(cx).as_deref(), Some("the program copied this"), "Cmd+C with no selection");
            let (note, _) = pane.read_with(cx, |p, _| p.visual_notes());
            assert!(note.as_deref().is_some_and(|n| n.contains("剪贴板没动")), "{note:?}");
            cx.update_window(handle.into(), |_, window, app| shot("1-cmd-c-no-selection", window, app)).unwrap();

            let (from, to) = pane.read_with(cx, |p, _| (p.visual_cell(5, 0), p.visual_cell(7, 1)));
            let (from, to) = (from.expect("pane laid out"), to.expect("pane laid out"));
            let drag = |modifiers: gpui::Modifiers, window: &mut gpui::Window, app: &mut App| {
                window.dispatch_event(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position: from, modifiers, click_count: 1, first_mouse: false }.to_platform_input(), app);
                window.dispatch_event(gpui::MouseMoveEvent { position: to, pressed_button: Some(gpui::MouseButton::Left), modifiers }.to_platform_input(), app);
                window.dispatch_event(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position: to, modifiers, click_count: 1 }.to_platform_input(), app);
            };
            cx.update_window(handle.into(), |_, window, app| {
                drag(Default::default(), window, app);
                shot("2-plain-drag", window, app);
            }).unwrap();
            let (note, hint) = pane.read_with(cx, |p, _| p.visual_notes());
            assert!(hint, "a plain drag shows the mouse hint");
            assert!(note.as_deref().is_some_and(|n| n.contains("剪贴板没动")), "the hint left the note up: {note:?}");
            assert_eq!(clipboard(cx).as_deref(), Some("the program copied this"), "a plain drag is the program's");

            cx.update_window(handle.into(), |_, window, app| {
                drag(gpui::Modifiers { shift: true, ..Default::default() }, window, app);
                shot("3-shift-drag", window, app);
            }).unwrap();
            let (note, hint) = pane.read_with(cx, |p, _| p.visual_notes());
            let copied = clipboard(cx).expect("Shift-drag copies");
            assert_eq!(copied.lines().count(), 3, "{copied:?}");
            assert_eq!(note.as_deref(), Some("已复制 3 行"));
            assert!(hint, "the copied note left the hint up");

            if let Some(before) = before {
                cx.update(|cx| cx.write_to_clipboard(before));
            }
            eprintln!("mouse taken native PASS: Cmd+C kept the clipboard, a plain drag stayed the program's, a Shift-drag copied {copied:?}");
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
