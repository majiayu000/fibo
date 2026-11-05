use super::*;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_TAB_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            wait(2000).await;
            for _ in 0..2 {
                handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
                wait(1500).await;
            }
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
            let state = |cx: &mut gpui::AsyncApp| {
                handle
                    .update(cx, |v, w, a| v.visual_tab_state(w, a))
                    .unwrap()
            };
            // The first tab sits right after the sidebar, in the strip under the top bar.
            let tab = gpui::point(
                px(theme::Theme::SIDEBAR + 30.),
                px(theme::Theme::TOP + theme::Theme::TABS / 2.),
            );
            let hover = |at: gpui::Point<gpui::Pixels>| {
                gpui::MouseMoveEvent {
                    position: at,
                    pressed_button: None,
                    modifiers: Default::default(),
                }
                .to_platform_input()
            };
            // Hovering shows the tab's card; the menu must not sit under it.
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_event(hover(tab), a);
            })
            .unwrap();
            wait(900).await;
            cx.update_window(handle.into(), |_, w, a| shot("0-tab-hover", w, a))
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                native_click(w, a, tab, gpui::MouseButton::Right)
            })
            .unwrap();
            let (menu, renaming) = state(cx);
            assert!(menu && renaming.is_none(), "right click opens the tab menu");
            // Still over the tab with the menu open: the card stays away.
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_event(hover(tab + gpui::point(px(4.), px(0.))), a);
            })
            .unwrap();
            wait(900).await;
            cx.update_window(handle.into(), |_, w, a| shot("1-tab-menu", w, a))
                .unwrap();
            let at = handle
                .update(cx, |v, _, _| v.visual_tab_menu_point(0))
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                native_click(w, a, at, gpui::MouseButton::Left)
            })
            .unwrap();
            wait(300).await;
            let (menu, renaming) = state(cx);
            eprintln!("menu rename: menu={menu} field={renaming:?}");
            assert!(
                !menu && renaming.as_ref().is_some_and(|(_, focused)| *focused),
                "重命名 opens a focused field"
            );
            cx.update_window(handle.into(), |_, w, a| shot("2-rename-selected", w, a))
                .unwrap();
            let key = |key: &str| {
                gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }
                .to_platform_input()
            };
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_event(key("backspace"), a);
            })
            .unwrap();
            wait(300).await;
            eprintln!("after backspace: {:?}", state(cx).1);
            cx.update_window(handle.into(), |_, w, a| shot("3-rename-caret", w, a))
                .unwrap();
            // A name longer than the tab stays on one line and slides to keep the caret.
            let long = "PR 1234 修复标签重命名时长名字换行被裁掉的问题";
            handle
                .update(cx, |v, _, cx| v.visual_rename_type(long, cx))
                .unwrap();
            wait(300).await;
            cx.update_window(handle.into(), |_, w, a| shot("3b-rename-long", w, a))
                .unwrap();
            for k in ["cmd-left", "alt-right", "shift-alt-right"] {
                cx.update_window(handle.into(), |_, w, a| {
                    w.dispatch_event(key(k), a);
                })
                .unwrap();
            }
            wait(300).await;
            cx.update_window(handle.into(), |_, w, a| shot("3c-rename-moved", w, a))
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_event(key("escape"), a);
            })
            .unwrap();
            wait(300).await;
            assert!(state(cx).1.is_none(), "Esc cancels the rename");
            // A real double click: two presses, the second carrying click_count 2.
            cx.update_window(handle.into(), |_, w, a| {
                for count in [1, 2] {
                    w.dispatch_event(
                        gpui::MouseDownEvent {
                            button: gpui::MouseButton::Left,
                            position: tab,
                            modifiers: Default::default(),
                            click_count: count,
                            first_mouse: false,
                        }
                        .to_platform_input(),
                        a,
                    );
                    w.dispatch_event(
                        gpui::MouseUpEvent {
                            button: gpui::MouseButton::Left,
                            position: tab,
                            modifiers: Default::default(),
                            click_count: count,
                        }
                        .to_platform_input(),
                        a,
                    );
                }
            })
            .unwrap();
            wait(300).await;
            let (_, renaming) = state(cx);
            eprintln!("double click: field={renaming:?}");
            assert!(
                renaming.as_ref().is_some_and(|(_, focused)| *focused),
                "double click opens a focused field"
            );
            cx.update_window(handle.into(), |_, w, a| shot("4-double-click", w, a))
                .unwrap();
            // Clicking the terminal keeps the typed name. Focus events only reach an
            // active window, as the user's is when they click in it.
            cx.update_window(handle.into(), |_, w, _| w.activate_window())
                .unwrap();
            wait(500).await;
            handle
                .update(cx, |v, _, cx| v.visual_rename_type("部署 PR", cx))
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                let middle = w.viewport_size();
                native_click(
                    w,
                    a,
                    gpui::point(middle.width / 2., middle.height / 2.),
                    gpui::MouseButton::Left,
                );
                // Focus listeners run once a frame is drawn, as the app does at once.
                w.refresh();
                w.draw(a).clear(a);
            })
            .unwrap();
            wait(300).await;
            let name = handle
                .update(cx, |v, _, cx| v.visual_first_tab_name(cx))
                .unwrap();
            let active = cx
                .update_window(handle.into(), |_, w, _| w.is_window_active())
                .unwrap();
            eprintln!(
                "after click away: active={active} field={:?} name={name}",
                state(cx).1
            );
            // macOS will not hand focus to a window started from a background shell.
            if active {
                assert!(state(cx).1.is_none(), "a click elsewhere ends the rename");
                assert_eq!(name, "部署 PR", "and keeps the typed name");
                cx.update_window(handle.into(), |_, w, a| shot("4b-blur-saved", w, a))
                    .unwrap();
            } else {
                eprintln!("click away not checked: the window is not active");
                cx.update_window(handle.into(), |_, w, a| {
                    w.dispatch_event(key("escape"), a);
                })
                .unwrap();
                wait(300).await;
            }
            // A 出错了 conversation whose resume fails: the second click returns to the
            // resume tab instead of opening another.
            let (before, _) = handle.update(cx, |v, _, _| v.visual_tabs()).unwrap();
            handle
                .update(cx, |v, w, a| v.visual_open_failed_session(w, a))
                .unwrap();
            wait(3000).await;
            let (first, resumed) = handle.update(cx, |v, _, _| v.visual_tabs()).unwrap();
            cx.update_window(handle.into(), |_, w, a| shot("5-failed-resume", w, a))
                .unwrap();
            for _ in 0..3 {
                handle
                    .update(cx, |v, w, a| v.visual_open_failed_session(w, a))
                    .unwrap();
                wait(1500).await;
            }
            let (again, current) = handle.update(cx, |v, _, _| v.visual_tabs()).unwrap();
            eprintln!(
                "failed session tabs: before={before} first={first} after 3 more clicks={again}"
            );
            assert_eq!(first, before + 1, "the first click opens one resume tab");
            assert_eq!(
                (again, &current),
                (first, &resumed),
                "later clicks return to it"
            );
            cx.update_window(handle.into(), |_, w, a| shot("6-failed-reclick", w, a))
                .unwrap();
            // Closing the shown tab hands the view to a neighbour with a live shell; that
            // shell must be drawable at once, before the host confirms the close.
            let neighbour = handle
                .update(cx, |v, _, cx| v.visual_close_tab(cx))
                .unwrap();
            assert!(neighbour.is_some(), "a neighbouring tab takes over");
            cx.update_window(handle.into(), |_, w, a| shot("7-closed-tab", w, a))
                .unwrap();
            // A full-screen Agent such as Codex asks for mouse reports; right-click must
            // still open Mooring's pane menu instead of going to the program.
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            pane.update(cx, |p, cx| {
                p.visual_type("printf '\\033[?1000;1002;1006h'\r", cx)
            });
            wait(1500).await;
            let (tracking, center) = pane.read_with(cx, |p, _| p.visual_mouse_app());
            assert!(tracking, "the shell turned on mouse reports");
            let center = center.expect("painted pane");
            cx.update_window(handle.into(), |_, w, a| {
                native_click(w, a, center, gpui::MouseButton::Right)
            })
            .unwrap();
            let opened = handle
                .update(cx, |v, _, _| v.visual_pane_menu_open())
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                shot("8-right-click-mouse-app", w, a)
            })
            .unwrap();
            eprintln!("right click over a mouse-reporting program: menu={opened}");
            assert!(
                opened,
                "right-click opens the pane menu even when the program takes the mouse"
            );
            // A tab of several splits asks before its × takes them all down; a tab of one
            // pane closes at once.
            handle
                .update(cx, |v, w, a| v.visual_split_down(w, a))
                .unwrap();
            wait(1000).await;
            let (before, shown) = handle.update(cx, |v, _, _| v.visual_tabs()).unwrap();
            handle
                .update(cx, |v, _, cx| v.visual_close_tab(cx))
                .unwrap();
            wait(500).await;
            cx.update_window(handle.into(), |_, w, a| shot("9-close-splits", w, a))
                .unwrap();
            let (asked, after) = handle
                .update(cx, |v, _, _| (v.visual_confirming_close(), v.visual_tabs()))
                .unwrap();
            assert!(asked, "closing a tab of two splits asks first");
            assert_eq!(after, (before, shown), "and keeps the tab until confirmed");
            handle
                .update(cx, |v, _, cx| v.visual_cancel_close(cx))
                .unwrap();
            handle
                .update(cx, |v, _, cx| v.visual_close_focused(cx))
                .unwrap();
            wait(1000).await;
            handle
                .update(cx, |v, _, cx| v.visual_close_tab(cx))
                .unwrap();
            wait(500).await;
            let (asked, (count, _)) = handle
                .update(cx, |v, _, _| (v.visual_confirming_close(), v.visual_tabs()))
                .unwrap();
            assert!(!asked, "a tab of one shell closes without asking");
            assert_eq!(count, before - 1, "and is gone");
            cx.update_window(handle.into(), |_, w, a| shot("10-closed-single", w, a))
                .unwrap();
            eprintln!("tab evidence PASS");
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
}
