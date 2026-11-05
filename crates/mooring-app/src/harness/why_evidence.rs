use super::*;

// The sidebar shows the space's branch; the pane menu explains the pane's state.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_WHY_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            // Near the top of the shown pane so the menu opens where it was asked,
            // not pushed up to fit; clicks hit what was last drawn.
            // Below the pane header, inside the screen.
            let at = gpui::point(
                px(theme::Theme::SIDEBAR + 120.),
                px(theme::Theme::TOP + theme::Theme::TABS + theme::Theme::PANE_HEADER + 120.),
            );
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh();
                window.draw(app).clear(app);
                native_click(window, app, at, gpui::MouseButton::Right)
            })
            .unwrap();
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            // 为什么是这个状态, after 在编辑器里看滚动内容.
            let item = handle
                .update(cx, |v, _, _| v.visual_menu_point("为什么是这个状态"))
                .unwrap();
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh();
                window.draw(app).clear(app);
                window
                    .render_to_image()
                    .unwrap()
                    .save(directory.join("1-menu.png"))
                    .unwrap();
                native_click(window, app, item, gpui::MouseButton::Left);
            })
            .unwrap();
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            assert!(
                handle.update(cx, |v, _, _| v.visual_menu_why()).unwrap(),
                "为什么是这个状态 turns the menu into the explanation"
            );
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh();
                window.draw(app).clear(app);
                window
                    .render_to_image()
                    .unwrap()
                    .save(directory.join("2-why.png"))
                    .unwrap();
            })
            .unwrap();
            eprintln!("why native PASS: the pane menu explains the state");
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
