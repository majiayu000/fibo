use super::*;

// Tab order by dragging and by the tab menu, a pane moved to another tab, and a
// space removed after confirming, all against a real host.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_LAYOUT_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            let executor = cx.background_executor().clone();
            let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
            wait(2000).await;
            for _ in 0..2 {
                handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
                wait(1500).await;
            }
            handle
                .update(cx, |v, _, cx| v.visual_name_tabs(&["alpha", "beta", "gamma"], cx))
                .unwrap();
            wait(800).await;
            handle
                .update(cx, |v, _, cx| v.visual_add_space("other", "/tmp", cx))
                .unwrap();
            wait(800).await;
            std::fs::create_dir_all(&directory).unwrap();
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                let path = directory.join(name);
                cx.update_window(handle.into(), |_, window, app| {
                    window.refresh();
                    window.draw(app).clear(app);
                    window.render_to_image().unwrap().save(path).unwrap();
                })
                .unwrap();
            };
            let titles =
                |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_tab_titles()).unwrap();
            shot("1-before.png", cx);
            assert_eq!(titles(cx), ["alpha", "beta", "gamma"]);
            let y = px(theme::Theme::TOP / 2.); // The tabs share the title row.
            let x = |at: f32| px(theme::Theme::SIDEBAR + at);
            let env = |name: &str, default: f32| {
                std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
            };
            let (from, to) = (env("KL_DRAG_FROM", 30.), env("KL_DRAG_TO", 140.));
            cx.update_window(handle.into(), |_, window, app| {
                let at = gpui::point(x(from), y);
                window.dispatch_event(
                    gpui::MouseDownEvent {
                        button: gpui::MouseButton::Left,
                        position: at,
                        modifiers: Default::default(),
                        click_count: 1,
                        first_mouse: false,
                    }
                    .to_platform_input(),
                    app,
                );
                for i in 1..=12 {
                    let p = gpui::point(x(from + (to - from) * i as f32 / 12.), y);
                    window.dispatch_event(
                        gpui::MouseMoveEvent {
                            position: p,
                            pressed_button: Some(gpui::MouseButton::Left),
                            modifiers: Default::default(),
                        }
                        .to_platform_input(),
                        app,
                    );
                }
                window.refresh();
                window.draw(app).clear(app);
                window
                    .render_to_image()
                    .unwrap()
                    .save(directory.join("2-dragging.png"))
                    .unwrap();
                window.dispatch_event(
                    gpui::MouseUpEvent {
                        button: gpui::MouseButton::Left,
                        position: gpui::point(x(to), y),
                        modifiers: Default::default(),
                        click_count: 1,
                    }
                    .to_platform_input(),
                    app,
                );
            })
            .unwrap();
            wait(600).await;
            shot("3-dragged.png", cx);
            eprintln!("after drag: {:?}", titles(cx));
            assert_eq!(
                titles(cx),
                ["beta", "alpha", "gamma"],
                "dragged onto gamma: the tab lands before the one dropped on"
            );
            // The tab menu: right-click the first tab (beta), 向右移.
            cx.update_window(handle.into(), |_, window, app| {
                native_click(window, app, gpui::point(x(from), y), gpui::MouseButton::Right);
                window.refresh();
                window.draw(app).clear(app);
                window
                    .render_to_image()
                    .unwrap()
                    .save(directory.join("4-tab-menu.png"))
                    .unwrap();
            })
            .unwrap();
            let item = handle.update(cx, |v, _, _| v.visual_tab_menu_point(1)).unwrap();
            cx.update_window(handle.into(), |_, window, app| {
                native_click(window, app, item, gpui::MouseButton::Left)
            })
            .unwrap();
            wait(600).await;
            assert_eq!(titles(cx), ["alpha", "beta", "gamma"], "向右移 swaps with the next tab");
            // Move gamma's pane into alpha: gamma closes with its last pane.
            handle
                .update(cx, |v, window, cx| v.visual_page_tab("gamma", window, cx))
                .unwrap();
            wait(600).await;
            handle
                .update(cx, |v, _, cx| v.visual_move_pane("alpha", cx))
                .unwrap();
            wait(800).await;
            shot("5-moved.png", cx);
            assert_eq!(titles(cx), ["alpha", "beta"]);
            assert_eq!(handle.update(cx, |v, _, _| v.visual_tab_layout("alpha")).unwrap(), 2);
            // Remove the other space from its right-click menu: the first press asks,
            // the second removes it.
            handle
                .update(cx, |v, _, cx| v.visual_open_space_menu("other", cx))
                .unwrap();
            wait(300).await;
            let remove = handle
                .update(cx, |v, _, _| v.visual_space_menu_point(2))
                .unwrap();
            // Clicks hit what was last drawn: paint the menu before pressing it.
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh();
                window.draw(app).clear(app);
                native_click(window, app, remove, gpui::MouseButton::Left)
            })
            .unwrap();
            wait(300).await;
            shot("6-remove-confirm.png", cx);
            cx.update_window(handle.into(), |_, window, app| {
                native_click(window, app, remove, gpui::MouseButton::Left)
            })
            .unwrap();
            wait(800).await;
            shot("7-removed.png", cx);
            let spaces = handle.update(cx, |v, _, _| v.visual_spaces()).unwrap();
            assert!(!spaces.iter().any(|s| s == "other"), "{spaces:?}");
            eprintln!(
                "layout native PASS: drag reorders tabs, 向右移 moves one step, a pane moves to another tab, a space is removed after confirming"
            );
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
