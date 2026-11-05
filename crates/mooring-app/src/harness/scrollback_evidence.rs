use super::*;

// Scrollback: the thumb shows when scrolled up, a drag held past the top edge scrolls
// and copies across screens, search highlights and steps, the thumb drags the view.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_SCROLLBACK_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            let pane = handle.update(cx, |view, _, _| view.visual_shown_pane()).unwrap();
            pane.update(cx, |p, cx| p.visual_type("clear; seq 1 300\r", cx));
            for _ in 0..50 {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if pane.update(cx, |p, _| p.visual_text()).lines().any(|l| l.trim() == "300") { break; }
            }
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                let path = directory.join(name);
                cx.update_window(handle.into(), |_, window, app| {
                    window.refresh(); window.draw(app).clear(app);
                    window.render_to_image().unwrap().save(path).unwrap();
                }).unwrap();
            };
            let b = pane.read_with(cx, |p, _| p.visual_bounds()).expect("laid out");
            let mv = |window: &mut gpui::Window, app: &mut App, at: gpui::Point<gpui::Pixels>, pressed: bool| {
                window.dispatch_event(gpui::MouseMoveEvent { position: at, pressed_button: pressed.then_some(gpui::MouseButton::Left), modifiers: Default::default() }.to_platform_input(), app);
            };
            // Up the history: the thumb appears.
            cx.update_window(handle.into(), |_, window, app| {
                window.dispatch_event(gpui::ScrollWheelEvent { position: b.center(), delta: gpui::ScrollDelta::Lines(gpui::point(0., 40.)), modifiers: Default::default(), touch_phase: gpui::TouchPhase::Moved }.to_platform_input(), app);
            }).unwrap();
            shot("1-scrolled.png", cx);
            let (top, total, len) = pane.read_with(cx, |p, _| p.visual_viewport());
            eprintln!("scrolled viewport {top}/{total}/{len}");
            assert!(top + len < total, "scrolled into history");
            // Press in the middle, hold the drag past the top edge a while, release.
            let start = gpui::point(b.left() + px(60.), b.center().y);
            let outside = gpui::point(start.x, b.top() - px(30.));
            cx.update_window(handle.into(), |_, window, app| {
                window.dispatch_event(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position: start, modifiers: Default::default(), click_count: 1, first_mouse: false }.to_platform_input(), app);
                mv(window, app, start - gpui::point(px(0.), px(40.)), true);
                mv(window, app, outside, true);
            }).unwrap();
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            cx.update_window(handle.into(), |_, window, app| {
                window.dispatch_event(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position: outside, modifiers: Default::default(), click_count: 1 }.to_platform_input(), app);
            }).unwrap();
            shot("2-dragged-past-top.png", cx);
            let after = pane.read_with(cx, |p, _| p.visual_viewport()).0;
            assert!(after < top, "holding the drag past the top scrolled up: {after} vs {top}");
            pane.update(cx, |p, cx| p.copy(cx));
            let copied = cx.update(|cx| cx.read_from_clipboard().and_then(|c| c.text())).unwrap_or_default();
            let lines: Vec<&str> = copied.lines().collect();
            eprintln!("copied {} lines: {:?} .. {:?}", lines.len(), lines.first(), lines.last());
            assert!(lines.len() > len, "copy reaches past one screen: {} lines, screen {len}", lines.len());
            // Consecutive numbers: nothing skipped or doubled across the screens it crossed.
            let nums: Vec<u32> = lines.iter().filter_map(|l| l.trim().parse().ok()).collect();
            assert!(nums.windows(2).all(|w| w[1] == w[0] + 1), "copied text runs on: {nums:?}");
            // Search: newest match first, then step back.
            handle.update(cx, |_, window, cx| pane.update(cx, |p, cx| p.visual_search("25", window, cx))).unwrap();
            cx.background_executor().timer(Duration::from_millis(500)).await;
            shot("3-search.png", cx);
            let (current, rows) = pane.read_with(cx, |p, _| p.visual_matches());
            eprintln!("search: current {current:?} of {} matches", rows.len());
            assert!(rows.len() >= 10 && current == Some(rows.len() - 1));
            pane.update(cx, |p, cx| p.visual_step_match(-1, cx));
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let (current2, _) = pane.read_with(cx, |p, _| p.visual_matches());
            assert_eq!(current2, Some(rows.len() - 2));
            shot("4-previous-match.png", cx);
            let (vtop, _, vlen) = pane.read_with(cx, |p, _| p.visual_viewport());
            assert!((vtop..vtop + vlen).contains(&rows[rows.len() - 2]), "the current match is in view");
            // Step forward wraps past the newest back to the oldest.
            pane.update(cx, |p, cx| { p.visual_step_match(1, cx); p.visual_step_match(1, cx); });
            assert_eq!(pane.read_with(cx, |p, _| p.visual_matches()).0, Some(0), "wraps to the first");
            // Thumb drag to the very top.
            let (_, total, _) = pane.read_with(cx, |p, _| p.visual_viewport());
            let grab = gpui::point(b.right() - px(4.), b.bottom() - px(10.));
            cx.update_window(handle.into(), |_, window, app| {
                window.dispatch_event(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position: grab, modifiers: Default::default(), click_count: 1, first_mouse: false }.to_platform_input(), app);
                mv(window, app, gpui::point(grab.x, b.top() + px(1.)), true);
                window.dispatch_event(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position: gpui::point(grab.x, b.top() + px(1.)), modifiers: Default::default(), click_count: 1 }.to_platform_input(), app);
            }).unwrap();
            shot("5-thumb-top.png", cx);
            let top_now = pane.read_with(cx, |p, _| p.visual_viewport()).0;
            assert!(top_now < total / 10, "thumb dragged to the top: {top_now} of {total}");
            eprintln!("scrollback native PASS: thumb shows and drags, a drag past the edge scrolls and copies {} lines, search highlights and steps", lines.len());
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
