use super::*;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_SPLIT_EVIDENCE").is_none()
        && std::env::var_os("MOORING_REASON_EVIDENCE").is_none()
        && std::env::var_os("MOORING_PINNED_ASK_EVIDENCE").is_none()
        && std::env::var_os("MOORING_TAB_EVIDENCE").is_none()
        && std::env::var_os("MOORING_PAGE_EVIDENCE").is_none()
        && std::env::var_os("MOORING_OVERVIEW_EVIDENCE").is_none()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            // Test-only single shots: native renderer output, no desktop capture permission.
            cx.background_executor().timer(Duration::from_secs(2)).await;
            handle
                .update(cx, |_, window, _| {
                    if window.viewport_size().height < px(theme::Theme::WINDOW_HEIGHT) {
                        window.toggle_simple_fullscreen();
                    }
                })
                .unwrap();
            cx.background_executor().timer(Duration::from_secs(1)).await;
            for overview in [false, true] {
                handle
                    .update(cx, |view, window, cx| {
                        window.resize(size(
                            px(theme::Theme::WINDOW_WIDTH),
                            px(theme::Theme::WINDOW_HEIGHT),
                        ));
                        window.bounds_changed(cx);
                        view.visual_view(overview, window, cx);
                    })
                    .unwrap();
                cx.background_executor().timer(Duration::from_secs(1)).await;
                handle
                    .update(cx, |_, window, app| {
                        assert_eq!(
                            window.viewport_size(),
                            size(
                                px(theme::Theme::WINDOW_WIDTH),
                                px(theme::Theme::WINDOW_HEIGHT)
                            ),
                            "visual comparison viewport"
                        );
                        eprintln!(
                            "{}: viewport={:?}, scale={}, mono={}",
                            if overview { "Overview" } else { "Main" },
                            window.viewport_size(),
                            window.scale_factor(),
                            theme::Theme::mono(app).family
                        );
                        if !overview {
                            screen::verify_native_geometry(window, app);
                        }
                        std::fs::create_dir_all(&directory).unwrap();
                        window
                            .render_to_image()
                            .unwrap()
                            .save(directory.join(if overview {
                                "overview-native.png"
                            } else {
                                "main-native.png"
                            }))
                            .unwrap();
                    })
                    .unwrap();
                if !overview {
                    let marked = handle
                        .update(cx, |view, window, cx| {
                            view.visual_marked_overlay(window, cx)
                        })
                        .unwrap();
                    // Render explicitly: an idle/occluded display link may not
                    // have presented the notification yet. Do not hold the root
                    // entity borrowed while draw renders it again.
                    cx.update_window(handle.into(), |_, window, app| {
                            window.refresh();
                            window.draw(app).clear(app);
                            window
                                .render_to_image()
                                .unwrap()
                                .save(directory.join("ime-native.png"))
                                .unwrap();
                            if let Some(marked) = marked {
                                let scale = window.scale_factor();
                                eprintln!("IME rendered underlines: {:?}", window.painted_underlines());
                                assert!(
                                    window.painted_underlines().iter().any(|line| {
                                        let x = line.bounds.origin.x.0 / scale;
                                        let y = line.bounds.origin.y.0 / scale;
                                        (x - f32::from(marked.left())).abs() < 1.
                                            && y >= f32::from(marked.top())
                                            && y <= f32::from(marked.bottom())
                                    }),
                                    "marked underline must be painted at the IME range, not the pane bottom"
                                );
                            }
                        })
                        .unwrap();
                }
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
}
