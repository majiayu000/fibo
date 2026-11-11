//! Click and hover actual tabs: selection must not move their edges or their neighbours.
use super::*;
use gpui::InputEvent;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_TAB_GEOMETRY_EVIDENCE").is_none() {
        return false;
    }
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        wait(2000).await;
        handle.update(cx, |v, w, cx| {
            w.resize(size(px(1420.), px(900.)));
            v.visual_page("Dense-first-standard-light", w, cx);
            v.visual_tab_agent_sample(cx);
            v.visual_tab_scroll_home(cx);
        }).unwrap();
        wait(400).await;
        std::fs::create_dir_all(&directory).unwrap();
        let shot = |name: &str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh();
                w.draw(a).clear(a);
                w.render_to_image().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
            }).unwrap();
        };
        let bounds = |cx: &mut gpui::AsyncApp| {
            handle.update(cx, |v, _, _| v.visual_tab_bounds()).unwrap()
        };
        let hover = |at, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_event(gpui::MouseMoveEvent {
                    position: at, pressed_button: None, modifiers: Default::default(),
                }.to_platform_input(), a);
            }).unwrap();
        };
        let mut failures = Vec::new();
        for (mode, name) in [(crate::ui::appearance::Mode::Light, "light"), (crate::ui::appearance::Mode::Dark, "dark")] {
            cx.update(|cx| crate::ui::appearance::set_mode(mode, cx).unwrap());
            hover(gpui::point(px(800.), px(850.)), cx);
            wait(200).await;
            shot(&format!("{name}-initial"), cx);
            let before = bounds(cx);
            assert!(before.len() >= 2, "at least two real shell tabs");
            for ix in 0..before.len() {
                // Stay over the label, away from the close control at the right edge.
                let at = gpui::point(before[ix].left() + px(14.), before[ix].center().y);
                hover(at, cx);
                wait(100).await;
                shot(&format!("{name}-{ix}-hover"), cx);
                if bounds(cx) != before { failures.push(format!("{name} hover {ix}")); }
                cx.update_window(handle.into(), |_, w, a| native_click(w, a, at, gpui::MouseButton::Left)).unwrap();
                wait(150).await;
                hover(gpui::point(px(800.), px(850.)), cx);
                shot(&format!("{name}-{ix}-selected"), cx);
                let after = bounds(cx);
                let (ids, selected, _) = handle.update(cx, |v, _, _| v.visual_tab_row()).unwrap();
                assert_eq!(selected.as_ref(), Some(&ids[ix]), "the native click selects the intended tab");
                eprintln!("tab geometry {name} click {ix}: before={before:?} after={after:?}");
                if after != before { failures.push(format!("{name} click {ix}")); }
            }
        }
        assert!(failures.is_empty(), "tab edges moved: {failures:?}");
        std::fs::write(directory.join("result.txt"), "PASS: shell, long title and running Agent tab edges stay fixed on hover and selection in light and dark\n").unwrap();
        eprintln!("tab geometry native PASS: shell and running Agent tab edges stable on hover and selection in light and dark");
        cx.update(|cx| cx.quit());
    }).detach();
    true
}
