use super::*;

// Drags a selection over shell output, lets new output push it up, scrolls the view
// back, then lets the text leave the screen for history: the highlight stays on the
// same text throughout and copies it, not the rows it left.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_SELECTION_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            let pane = handle.update(cx, |view, _, _| view.visual_shown_pane()).unwrap();
            pane.update(cx, |p, cx| p.visual_type("clear; seq 1 400; sleep 4; seq 401 405; sleep 3; seq 406 460\r", cx));
            // A fresh zsh can take seconds to start; the echoed command line is not output.
            for _ in 0..150 {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if pane.update(cx, |p, _| p.visual_text()).lines().any(|l| l.trim() == "400") { break; }
            }
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let shot = |name: &str, window: &mut gpui::Window, app: &mut App| {
                window.refresh(); window.draw(app).clear(app);
                window.render_to_image().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
            };
            let (from, to) = pane.read_with(cx, |p, _| (p.visual_cell(5, 0), p.visual_cell(7, 1)));
            let (from, to) = (from.expect("pane laid out"), to.expect("pane laid out"));
            cx.update_window(handle.into(), |_, window, app| {
                window.dispatch_event(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position: from, modifiers: Default::default(), click_count: 1, first_mouse: false }.to_platform_input(), app);
                window.dispatch_event(gpui::MouseMoveEvent { position: to, pressed_button: Some(gpui::MouseButton::Left), modifiers: Default::default() }.to_platform_input(), app);
                window.dispatch_event(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position: to, modifiers: Default::default(), click_count: 1 }.to_platform_input(), app);
                shot("1-selected", window, app);
            }).unwrap();
            let (rows, text) = pane.read_with(cx, |p, _| p.visual_selection());
            let text = text.expect("drag selected nothing");
            assert_eq!(rows, [5, 6, 7], "selection drawn on the dragged rows");
            assert_eq!(text.lines().count(), 3, "{text:?}");
            // Copy on select put it on the clipboard already.
            let clip = cx.update(|cx| cx.read_from_clipboard().and_then(|c| c.text()));
            assert_eq!(clip.as_deref(), Some(text.as_str()), "copy on select");
            // New output pushes the selected text up, partly into history.
            for _ in 0..40 {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if pane.update(cx, |p, _| p.visual_text()).lines().any(|l| l.trim() == "405") { break; }
            }
            cx.background_executor().timer(Duration::from_millis(300)).await;
            cx.update_window(handle.into(), |_, window, app| shot("2-output-pushed", window, app)).unwrap();
            // The highlight sits on the rows that show the selected text, wherever they are now.
            let on_text = |((rows, copied), drawn): ((Vec<u16>, Option<String>), Vec<String>), step: &str| {
                assert_eq!(copied.as_deref(), Some(text.as_str()), "{step}: copied text changed");
                let lines: Vec<&str> = text.lines().collect();
                assert!(!drawn.is_empty() && drawn.len() <= lines.len(), "{step}: {rows:?} {drawn:?}");
                // Rows scrolled out of view drop from the top; the end stays in view.
                let shown = &lines[lines.len() - drawn.len()..];
                for (i, (row, line)) in drawn.iter().zip(shown).enumerate() {
                    let ok = if i + 1 == drawn.len() { row.starts_with(line) } else { row == line };
                    assert!(ok, "{step}: highlight on {drawn:?}, selected {lines:?}");
                }
                rows
            };
            let pushed = on_text(pane.read_with(cx, |p, _| p.visual_selection_drawn()), "output");
            assert!(*pushed.last().unwrap() < 7, "output moved the text but not the selection: {pushed:?}");
            // The wheel scrolls the view back; the highlight comes down with its text.
            cx.update_window(handle.into(), |_, window, app| {
                window.dispatch_event(gpui::ScrollWheelEvent { position: from, delta: gpui::ScrollDelta::Lines(gpui::point(0., 3.)), modifiers: Default::default(), touch_phase: gpui::TouchPhase::Moved }.to_platform_input(), app);
                shot("3-scrolled", window, app);
            }).unwrap();
            let scrolled = on_text(pane.read_with(cx, |p, _| p.visual_selection_drawn()), "wheel");
            assert!(scrolled.last() > pushed.last(), "the wheel moved the text but not the selection: {pushed:?} -> {scrolled:?}");
            pane.update(cx, |p, cx| p.copy(cx));
            let copied = cx.update(|cx| cx.read_from_clipboard().and_then(|c| c.text()));
            assert_eq!(copied.as_deref(), Some(text.as_str()), "copy after scrolling");
            // Back to the bottom; more output carries the text off screen into history.
            cx.update_window(handle.into(), |_, window, app| {
                window.dispatch_event(gpui::ScrollWheelEvent { position: from, delta: gpui::ScrollDelta::Lines(gpui::point(0., -200.)), modifiers: Default::default(), touch_phase: gpui::TouchPhase::Moved }.to_platform_input(), app);
            }).unwrap();
            for _ in 0..40 {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if pane.update(cx, |p, _| p.visual_text()).lines().any(|l| l.trim() == "460") { break; }
            }
            cx.background_executor().timer(Duration::from_millis(300)).await;
            cx.update_window(handle.into(), |_, window, app| shot("4-in-history", window, app)).unwrap();
            let ((gone, kept), _) = pane.read_with(cx, |p, _| p.visual_selection_drawn());
            assert!(gone.is_empty(), "the selected text left the screen, its highlight too: {gone:?}");
            assert_eq!(kept.as_deref(), Some(text.as_str()), "selection kept in history");
            cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
            pane.update(cx, |p, cx| p.copy(cx));
            let copied = cx.update(|cx| cx.read_from_clipboard().and_then(|c| c.text()));
            assert_eq!(copied.as_deref(), Some(text.as_str()), "copy from history");
            eprintln!("selection native PASS: {text:?} stays highlighted on its text after output and wheel scrolling, and copies the same from history");
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
