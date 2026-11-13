use super::*;

// A space is renamed in place from its right-click menu, Esc keeps the old name,
// and a space whose folder is gone can still be renamed.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_SPACE_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            wait(2000).await;
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
                    .update(cx, |v, w, a| v.visual_space_state(w, a))
                    .unwrap()
            };
            let key = |key: &str| {
                gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }
                .to_platform_input()
            };
            handle
                .update(cx, |v, _, cx| v.visual_lose_space_dir(cx))
                .unwrap();
            wait(800).await;
            let (_, _, spaces) = state(cx);
            eprintln!("spaces: {spaces:?}");
            let before = spaces[0].0.clone();
            cx.update_window(handle.into(), |_, w, a| shot("0-space-missing", w, a))
                .unwrap();
            // The first space row sits under the preset switch and the 空间 heading.
            let row = gpui::point(
                px(theme::Theme::S20 + 20.),
                px(std::env::var("KL_SPACE_ROW_Y")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(132.)),
            );
            cx.update_window(handle.into(), |_, w, a| {
                native_click(w, a, row, gpui::MouseButton::Right)
            })
            .unwrap();
            let (menu, renaming, _) = state(cx);
            assert!(
                menu && renaming.is_none(),
                "right click opens the space menu"
            );
            cx.update_window(handle.into(), |_, w, a| shot("1-space-menu", w, a))
                .unwrap();
            let at = handle
                .update(cx, |v, _, _| v.visual_space_menu_point(0))
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                native_click(w, a, at, gpui::MouseButton::Left)
            })
            .unwrap();
            wait(300).await;
            let (menu, renaming, _) = state(cx);
            eprintln!("menu rename: menu={menu} field={renaming:?}");
            assert!(
                !menu && renaming == Some((before.clone(), true)),
                "重命名 opens a focused field holding the name"
            );
            cx.update_window(handle.into(), |_, w, a| shot("2-space-renaming", w, a))
                .unwrap();
            // Esc puts the old name back.
            handle
                .update(cx, |v, _, cx| v.visual_space_rename_type("不该留下", cx))
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_event(key("escape"), a);
            })
            .unwrap();
            wait(800).await;
            let (_, renaming, spaces) = state(cx);
            assert!(renaming.is_none(), "Esc ends the rename");
            assert_eq!(spaces[0].0, before, "and keeps the old name");
            // Enter keeps the new one, with the folder still missing.
            handle
                .update(cx, |v, w, cx| v.visual_rename_first_space(w, cx))
                .unwrap();
            handle
                .update(cx, |v, _, cx| {
                    v.visual_space_rename_type("改名后的空间", cx)
                })
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| shot("3-space-typed", w, a))
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_event(key("enter"), a);
            })
            .unwrap();
            wait(1500).await;
            let (_, renaming, spaces) = state(cx);
            eprintln!("after enter: {spaces:?}");
            assert!(renaming.is_none(), "Enter ends the rename");
            assert_eq!(spaces[0].0, "改名后的空间", "and saves the name");
            assert_eq!(
                spaces[0].1, "/nonexistent/mooring-space-evidence",
                "without touching the missing folder"
            );
            cx.update_window(handle.into(), |_, w, a| shot("4-space-renamed", w, a))
                .unwrap();
            eprintln!("space evidence PASS");
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
