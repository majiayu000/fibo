use super::*;

// The overview sits in the same window as the panes: one toggle in the top bar goes
// in and back out, Esc and the tab strip return, the sidebar's spaces filter it.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_OVERVIEW_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            wait(3000).await;
            std::fs::create_dir_all(&directory).unwrap();
            let shot = |name: &str, window: &mut gpui::Window, app: &mut App| {
                window.refresh();
                window.draw(app).clear(app);
                window
                    .render_to_image()
                    .unwrap()
                    .save(directory.join(format!("{name}.png")))
                    .unwrap();
            };
            // Focus moves when a frame is drawn; idle frames are paused in this harness.
            let state = |cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                })
                .unwrap();
                handle
                    .update(cx, |v, w, a| v.visual_overview_state(w, a))
                    .unwrap()
            };
            let click = |cx: &mut gpui::AsyncApp, at: gpui::Point<gpui::Pixels>| {
                cx.update_window(handle.into(), |_, w, a| {
                    native_click(w, a, at, gpui::MouseButton::Left)
                })
                .unwrap();
            };
            let escape = |cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, w, a| {
                    use gpui::InputEvent;
                    w.dispatch_event(
                        gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("escape").unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        }
                        .to_platform_input(),
                        a,
                    );
                    w.refresh();
                    w.draw(a).clear(a);
                })
                .unwrap();
            };
            if state(cx).0 {
                escape(cx);
                wait(500).await;
            }
            // A live shell to come back to: the copied workspace's own terminals are gone.
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            wait(2000).await;
            let start = state(cx);
            eprintln!("work view: {start:?}");
            assert!(!start.0, "starts on the panes");
            assert_eq!(start.3, "pane", "the new shell has the keyboard");
            cx.update_window(handle.into(), |_, w, a| shot("1-work", w, a))
                .unwrap();
            let toggle = handle
                .update(cx, |v, _, _| v.visual_toggle_point())
                .unwrap();
            let open = |cx: &mut gpui::AsyncApp| {
                click(cx, toggle);
                let s = state(cx);
                assert!(s.0, "the toggle opens the overview");
                assert_eq!(s.3, "overview", "the overview takes the keyboard");
                // Drawn in place: the toggle has not moved, so it can close it again.
                let again = handle
                    .update(cx, |v, _, _| v.visual_toggle_point())
                    .unwrap();
                assert_eq!(again, toggle, "the toggle does not move");
                s
            };
            open(cx);
            wait(1500).await;
            cx.update_window(handle.into(), |_, w, a| shot("2-overview", w, a))
                .unwrap();
            // The same spot closes it.
            click(cx, toggle);
            wait(800).await;
            let back = handle
                .update(cx, |v, _, _| v.visual_toggle_point())
                .unwrap();
            assert_eq!(back, toggle, "the toggle does not move on the way back");
            let s = state(cx);
            assert!(!s.0, "the same toggle returns");
            assert_eq!(s.1, start.1, "back on the tab that was shown");
            cx.update_window(handle.into(), |_, w, a| shot("3-back-by-toggle", w, a))
                .unwrap();
            assert_eq!(state(cx).3, "pane", "the pane has the keyboard again");
            // Esc returns.
            open(cx);
            escape(cx);
            wait(800).await;
            let s = state(cx);
            assert!(!s.0, "Esc returns");
            assert_eq!(s.3, "pane", "Esc hands the keyboard back to the pane");
            // A tab in the strip returns to it.
            open(cx);
            click(
                cx,
                gpui::point(
                    px(theme::Theme::SIDEBAR + 30.),
                    px(theme::Theme::TOP + theme::Theme::TABS / 2.),
                ),
            );
            wait(800).await;
            let s = state(cx);
            assert!(!s.0, "a tab returns");
            assert!(s.1.is_some(), "a tab is shown");
            // A space in the sidebar filters the overview and keeps it open.
            open(cx);
            let spaces = handle.update(cx, |v, _, _| v.visual_spaces()).unwrap();
            let mut filtered = None;
            for y in (0..12).map(|i| theme::Theme::TOP + 30. + i as f32 * 4.) {
                click(cx, gpui::point(px(60.), px(y)));
                let s = state(cx);
                if s.2.is_some() {
                    filtered = Some((y, s));
                    break;
                }
            }
            let (y, s) = filtered.expect("a sidebar space sets the filter");
            eprintln!("sidebar space at y={y}: {s:?}");
            assert!(s.0, "the overview stays open");
            assert_eq!(s.2.as_ref(), spaces.first(), "the first space filters it");
            wait(800).await;
            cx.update_window(handle.into(), |_, w, a| shot("4-space-filter", w, a))
                .unwrap();
            escape(cx);
            wait(500).await;
            assert!(!state(cx).0);
            eprintln!("overview evidence PASS");
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
}
