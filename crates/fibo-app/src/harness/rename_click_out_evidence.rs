use super::*;

// MOORING_RENAME_CLICK_OUT_EVIDENCE: a click anywhere outside a rename field ends the
// rename and keeps the name, and the click still does what it does. Rows and blank
// sidebar take no focus, so the field's blur alone never fired for them.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence)
        && std::env::var_os("MOORING_RENAME_CLICK_OUT_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            wait(2000).await;
            std::fs::create_dir_all(&directory).unwrap();
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                    w.render_to_image()
                        .unwrap()
                        .save(directory.join(format!("{name}.png")))
                        .unwrap();
                })
                .unwrap();
            };
            // Another window taking over blurs the field by itself, which says nothing about
            // clicks: such a run is neither a pass nor a failure. A window that was never
            // active is fine, it just gets no blur at all.
            let was_active = std::rc::Rc::new(std::cell::Cell::new(false));
            let still_active = {
                let was_active = was_active.clone();
                move |cx: &mut gpui::AsyncApp| {
                    let active = cx
                        .update_window(handle.into(), |_, w, _| w.is_window_active())
                        .unwrap();
                    if was_active.replace(active) && !active {
                        eprintln!("rename click out evidence INCONCLUSIVE: window lost activity");
                        std::process::exit(2);
                    }
                }
            };
            // A hand's press and release, with a frame drawn between them: closing the field
            // at the press moves the chips, and the release would then miss the one pressed.
            macro_rules! press {
                ($from:expr, $to:expr) => {{
                    let (from, to): ((f32, f32), (f32, f32)) = ($from, $to);
                    cx.update_window(handle.into(), |_, w, _| w.activate_window())
                        .unwrap();
                    wait(200).await;
                    still_active(cx);
                    cx.update_window(handle.into(), |_, w, a| {
                        use gpui::InputEvent;
                        w.dispatch_event(
                            gpui::MouseDownEvent {
                                button: gpui::MouseButton::Left,
                                position: gpui::point(px(from.0), px(from.1)),
                                modifiers: Default::default(),
                                click_count: 1,
                                first_mouse: false,
                            }
                            .to_platform_input(),
                            a,
                        );
                        w.refresh();
                        w.draw(a).clear(a);
                    })
                    .unwrap();
                    wait(120).await;
                    still_active(cx);
                    cx.update_window(handle.into(), |_, w, a| {
                        use gpui::InputEvent;
                        w.dispatch_event(
                            gpui::MouseMoveEvent {
                                position: gpui::point(px(to.0), px(to.1)),
                                pressed_button: Some(gpui::MouseButton::Left),
                                modifiers: Default::default(),
                            }
                            .to_platform_input(),
                            a,
                        );
                        w.dispatch_event(
                            gpui::MouseUpEvent {
                                button: gpui::MouseButton::Left,
                                position: gpui::point(px(to.0), px(to.1)),
                                modifiers: Default::default(),
                                click_count: 1,
                            }
                            .to_platform_input(),
                            a,
                        );
                    })
                    .unwrap();
                }};
            }
            // Measured on the shots: with a space's field open it fills the first chip row
            // and the other chip wraps to the second; with no field that chip is on the first.
            const FIELD: (f32, f32) = (157., 136.);
            const OTHER_WRAPPED: (f32, f32) = (41., 167.);
            const OTHER: (f32, f32) = (122., 136.);
            const BLANK: (f32, f32) = (120., 600.);
            handle
                .update(cx, |v, _, cx| v.visual_add_space("other", "/tmp", cx))
                .unwrap();
            wait(800).await;
            shot("0-two-spaces", cx);
            let space = |cx: &mut gpui::AsyncApp| {
                handle
                    .update(cx, |v, w, a| v.visual_space_state(w, a))
                    .unwrap()
            };
            let (first, _, _) = handle.update(cx, |v, _, a| v.visual_shown(a)).unwrap();

            // On another space: saved, closed, and the click still switches.
            handle
                .update(cx, |v, w, cx| v.visual_rename_first_space(w, cx))
                .unwrap();
            wait(300).await;
            handle
                .update(cx, |v, _, cx| v.visual_space_rename_type("点别的空间", cx))
                .unwrap();
            wait(100).await;
            shot("1-renaming", cx);
            press!(OTHER_WRAPPED, OTHER_WRAPPED);
            wait(1200).await;
            shot("2-clicked-other-space", cx);
            let (_, renaming, spaces) = space(cx);
            let (shown, ..) = handle.update(cx, |v, _, a| v.visual_shown(a)).unwrap();
            eprintln!("other space: field={renaming:?} names={spaces:?} shown={shown}");
            assert!(
                renaming.is_none(),
                "a click on another space ends the rename"
            );
            assert_eq!(spaces[0].0, "点别的空间", "and keeps the name");
            assert_eq!(shown, "other", "and still switches");

            // On blank sidebar: saved, closed, and the keyboard is back on the terminal.
            handle
                .update(cx, |v, _, cx| v.visual_pick_space(&first, cx))
                .unwrap();
            wait(800).await;
            handle
                .update(cx, |v, w, cx| v.visual_rename_first_space(w, cx))
                .unwrap();
            wait(300).await;
            handle
                .update(cx, |v, _, cx| v.visual_space_rename_type("点空白处", cx))
                .unwrap();
            press!(BLANK, BLANK);
            wait(1200).await;
            shot("3-clicked-blank", cx);
            let (_, renaming, spaces) = space(cx);
            let keyboard = handle
                .update(cx, |v, w, a| v.visual_pane_has_keyboard(w, a))
                .unwrap();
            eprintln!("blank: field={renaming:?} names={spaces:?} keyboard={keyboard}");
            assert!(
                renaming.is_none(),
                "a click on blank sidebar ends the rename"
            );
            assert_eq!(spaces[0].0, "点空白处", "and keeps the name");
            assert!(keyboard, "and the keyboard goes back to the terminal");

            // The tab's field is the same case: a click on another space ends it too.
            let tab = handle
                .update(cx, |v, _, a| v.visual_shown(a).1)
                .unwrap()
                .expect("the first space shows a tab");
            handle
                .update(cx, |v, w, cx| v.visual_rename_tab(&tab, w, cx))
                .unwrap();
            wait(300).await;
            handle
                .update(cx, |v, _, cx| v.visual_rename_type("点别处的标签", cx))
                .unwrap();
            press!(OTHER, OTHER);
            wait(1200).await;
            shot("4-tab-clicked-out", cx);
            let (_, renaming) = handle
                .update(cx, |v, w, a| v.visual_tab_state(w, a))
                .unwrap();
            eprintln!("tab: field={renaming:?}");
            assert!(renaming.is_none(), "a click elsewhere ends a tab's rename");

            // So is a session's, in a row of the list.
            handle
                .update(cx, |v, w, cx| v.visual_rename_probe_session(w, cx))
                .unwrap();
            wait(300).await;
            shot("4b-session-renaming", cx);
            let opened = handle
                .update(cx, |v, w, a| v.visual_session_field(w, a))
                .unwrap();
            eprintln!("session opened: {opened:?}");
            still_active(cx);
            assert!(opened.is_some_and(|f| f.1), "the session's field is open");
            press!(BLANK, BLANK);
            wait(800).await;
            let renaming = handle
                .update(cx, |v, w, a| v.visual_session_field(w, a))
                .unwrap();
            let keyboard = handle
                .update(cx, |v, w, a| v.visual_pane_has_keyboard(w, a))
                .unwrap();
            eprintln!("session: field={renaming:?} keyboard={keyboard}");
            assert!(
                renaming.is_none(),
                "a click elsewhere ends a session's rename"
            );
            assert!(keyboard, "and the keyboard goes back to the terminal");

            // Inside the field a click only moves the caret, and a selection dragged out of
            // the field and released outside is still a selection.
            handle
                .update(cx, |v, w, cx| v.visual_rename_first_space(w, cx))
                .unwrap();
            wait(300).await;
            press!(FIELD, FIELD);
            wait(800).await;
            let (_, renaming, _) = space(cx);
            assert!(renaming.is_some(), "a click inside keeps the field open");
            press!(FIELD, BLANK);
            wait(800).await;
            shot("5-dragged-out-of-field", cx);
            let (_, renaming, _) = space(cx);
            eprintln!("drag out: field={renaming:?}");
            assert!(renaming.is_some(), "a drag out of the field keeps it open");
            eprintln!("rename click out evidence PASS");
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
