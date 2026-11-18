use super::*;
use gpui::InputEvent;

// A few tabs that all fit, and the mouse wheel turned over them: the strip has nowhere to
// go, so no edge may claim a tab is out of sight, not even in the frame right after the turn.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_TAB_WHEEL_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            std::fs::create_dir_all(&directory).unwrap();
            wait(2000).await;
            for _ in 0..2 {
                handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
                wait(300).await;
            }
            wait(1500).await;
            let (left, right, marker) = handle
                .update(cx, |v, _, _| v.visual_tab_overflow())
                .unwrap();
            eprintln!("tab wheel: before left={left} right={right}");
            assert!(left == 0 && right == 0, "the tabs fit the strip");
            // `marker` sits just past the strip's right edge; the strip is under the left of it.
            let over_strip = gpui::point(marker.x - px(120.), marker.y);
            let view = cx.update(|cx| handle.entity(cx)).unwrap();
            let mut worst = 0;
            for turn in 0..6 {
                let seen = cx
                    .update_window(handle.into(), |_, w, a| {
                        w.dispatch_event(
                            gpui::ScrollWheelEvent {
                                position: over_strip,
                                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-90.))),
                                modifiers: Default::default(),
                                touch_phase: gpui::TouchPhase::Moved,
                            }
                            .to_platform_input(),
                            a,
                        );
                        // What the next render counts, before layout clamps the offset.
                        let seen = view.read(a).visual_tab_overflow().0;
                        w.draw(a).clear(a);
                        if turn == 0 {
                            w.render_to_image()
                                .unwrap()
                                .save(directory.join("tab-wheel-turned.png"))
                                .unwrap();
                        }
                        seen
                    })
                    .unwrap();
                eprintln!("tab wheel: turn {turn} left={seen}");
                worst = worst.max(seen);
                wait(50).await;
            }
            assert_eq!(
                worst, 0,
                "a wheel turn over tabs that fit hides none of them"
            );
            eprintln!("tab wheel native PASS");
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
