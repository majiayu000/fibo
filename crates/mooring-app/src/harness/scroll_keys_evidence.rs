use super::*;

// ⌘Home / ⌘End and the 「回到底部」 chip move through scrollback; a program's OSC 52
// copy lands on the clipboard.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_SCROLL_KEYS_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            let pane = handle.update(cx, |view, _, _| view.visual_shown_pane()).unwrap();
            cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into())));
            // "osc52 复制" in base64, written by the program as OSC 52.
            pane.update(cx, |p, cx| p.visual_type("clear; seq 1 300; printf '\\033]52;c;b3NjNTIg5aSN5Yi2\\a'\r", cx));
            for _ in 0..50 {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if pane.update(cx, |p, _| p.visual_text()).lines().any(|l| l.trim() == "300") { break; }
            }
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let shot = |name: &str, window: &mut gpui::Window, app: &mut App| {
                window.refresh(); window.draw(app).clear(app);
                window.render_to_image().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
            };
            cx.update_window(handle.into(), |_, window, app| shot("0-osc52", window, app)).unwrap();
            let clip = cx.update(|cx| cx.read_from_clipboard().and_then(|c| c.text()));
            assert_eq!(clip.as_deref(), Some("osc52 复制"), "OSC 52 reached the clipboard");
            let key = |k: &str, window: &mut gpui::Window, app: &mut App| {
                window.dispatch_event(gpui::KeyDownEvent { keystroke: gpui::Keystroke::parse(k).unwrap(), is_held: false, prefer_character_input: false }.to_platform_input(), app);
            };
            cx.update_window(handle.into(), |_, window, app| { key("cmd-home", window, app); shot("1-cmd-home", window, app); }).unwrap();
            assert!(!pane.read_with(cx, |p, _| p.visual_at_bottom()), "⌘Home left the bottom");
            assert!(pane.read_with(cx, |p, _| p.visual_text_shown()).lines().next().is_some_and(|l| l.trim() == "1"), "⌘Home shows the top");
            cx.update_window(handle.into(), |_, window, app| key("cmd-end", window, app)).unwrap();
            assert!(pane.read_with(cx, |p, _| p.visual_at_bottom()), "⌘End returned to the bottom");
            cx.update_window(handle.into(), |_, window, app| key("cmd-pageup", window, app)).unwrap();
            assert!(!pane.read_with(cx, |p, _| p.visual_at_bottom()), "⌘PageUp left the bottom");
            // The chip floats at the pane's bottom right; clicking it goes back down.
            let chip = pane.read_with(cx, |p, _| p.visual_bounds()).map(|b| b.bottom_right() - gpui::point(px(50.), px(16.))).expect("laid out");
            cx.update_window(handle.into(), |_, window, app| {
                shot("2-chip", window, app);
                window.dispatch_event(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position: chip, modifiers: Default::default(), click_count: 1, first_mouse: false }.to_platform_input(), app);
                window.dispatch_event(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position: chip, modifiers: Default::default(), click_count: 1 }.to_platform_input(), app);
                shot("3-chip-clicked", window, app);
            }).unwrap();
            assert!(pane.read_with(cx, |p, _| p.visual_at_bottom()), "the chip returned to the bottom");
            eprintln!("scroll keys native PASS: OSC 52 copies; ⌘Home/⌘End/⌘PageUp move the view; the chip returns to the bottom");
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
