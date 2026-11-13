use super::*;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && (std::env::var_os("MOORING_SPLIT_EVIDENCE").is_some()
            || std::env::var_os("MOORING_RESIZE_EVIDENCE").is_some())
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            if std::env::var_os("MOORING_RESIZE_EVIDENCE").is_some() {
                resize_regression(handle, &directory, cx).await;
                cx.update(|cx| cx.quit());
                return;
            }
            for name in ["single","nested","menu","eight"] {
                handle.update(cx,|view,window,cx|{
                    if window.viewport_size().height < px(theme::Theme::WINDOW_HEIGHT) {window.toggle_simple_fullscreen();}
                    window.resize(size(px(theme::Theme::WINDOW_WIDTH),px(theme::Theme::WINDOW_HEIGHT)));window.bounds_changed(cx);
                    view.visual_layout(name,window,cx);
                }).unwrap();
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let root=if name=="menu"{Some(handle.update(cx,|v,_,_|v.visual_bounds()).unwrap())}else{None};
                cx.update_window(handle.into(),|_,window,app|{
                    window.refresh();window.draw(app).clear(app);
                    if let Some(bounds)=root {native_click(window,app,bounds.origin+gpui::point(px(40.),px(80.)),gpui::MouseButton::Right);}
                    assert_eq!(window.viewport_size(),size(px(theme::Theme::WINDOW_WIDTH),px(theme::Theme::WINDOW_HEIGHT)));
                    screen::verify_native_geometry(window,app);
                    std::fs::create_dir_all(&directory).unwrap();
                    window.render_to_image().unwrap().save(directory.join(format!("{name}-native.png"))).unwrap();
                }).unwrap();
                if let Some(bounds)=root {
                    // Native event dispatch tests; simulated mouse, not human/CUA acceptance.
                    for expected in [1,4] {
                        let at=handle.update(cx,|v,_,_|v.visual_menu_point(if expected==1 {"放大"} else {"还原"})).unwrap();
                        cx.update_window(handle.into(),|_,window,app|native_click(window,app,at,gpui::MouseButton::Left)).unwrap();
                        handle.update(cx,|v,_,_|v.visual_check_panes(expected)).unwrap();
                        if expected==1 {cx.update_window(handle.into(),|_,window,app|native_click(window,app,bounds.origin+gpui::point(px(40.),px(80.)),gpui::MouseButton::Right)).unwrap();}
                    }
                    cx.update_window(handle.into(),|_,window,app|{
                        use gpui::InputEvent;
                        let from=gpui::point(bounds.left()+(bounds.size.width-px(theme::Theme::S6))*0.58+px(theme::Theme::S6/2.),bounds.top()+px(30.));
                        let to=gpui::point(bounds.left()+bounds.size.width*0.67,from.y);
                        window.dispatch_event(gpui::MouseDownEvent{button:gpui::MouseButton::Left,position:from,modifiers:Default::default(),click_count:1,first_mouse:false}.to_platform_input(),app);
                        window.dispatch_event(gpui::MouseMoveEvent{position:to,pressed_button:Some(gpui::MouseButton::Left),modifiers:Default::default()}.to_platform_input(),app);
                        window.dispatch_event(gpui::MouseUpEvent{button:gpui::MouseButton::Left,position:to,modifiers:Default::default(),click_count:1}.to_platform_input(),app);
                        window.refresh();window.draw(app).clear(app);
                    }).unwrap();
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    handle.update(cx,|v,_,_|v.visual_check_saved_ratio()).unwrap();
                    eprintln!("native events PASS: right menu, zoom 4->1->4, divider drag ratio persisted");
                }
            }
            if std::env::var_os("KL_NATIVE_ONLY").is_some() {
                let bounds=handle.update(cx,|v,_,_|v.visual_bounds()).unwrap();
                cx.update_window(handle.into(),|_,w,a|native_click(w,a,bounds.origin+gpui::point(px(40.),px(80.)),gpui::MouseButton::Right)).unwrap();
                let at=handle.update(cx,|v,_,_|v.visual_menu_point("向右分屏")).unwrap();
                cx.update_window(handle.into(),|_,w,a|native_click(w,a,at,gpui::MouseButton::Left)).unwrap();
                cx.background_executor().timer(Duration::from_secs(1)).await;
                handle.update(cx,|v,w,a|{v.visual_assert_new_shell(9,w,a);v.visual_split_down(w,a);}).unwrap();
                cx.background_executor().timer(Duration::from_secs(1)).await;
                handle.update(cx,|v,w,a|{v.visual_assert_new_shell(10,w,a);v.visual_close_focused(a);}).unwrap();
                cx.background_executor().timer(Duration::from_secs(1)).await;
                handle.update(cx,|v,_,a|{v.visual_check_panes(9);v.visual_check_closed_processes(1);v.visual_close_focused(a);}).unwrap();
                cx.background_executor().timer(Duration::from_secs(1)).await;
                handle.update(cx,|v,_,_|{v.visual_check_panes(8);v.visual_check_closed_processes(2);}).unwrap();
                eprintln!("native actions PASS: right/down launch 8->9->10, shell cwd and focus, close 10->9->8 stops the closed PTYs");
            }
            cx.update(|cx|cx.quit());
        }).detach();
        return true;
    }
    false
}

async fn resize_regression(
    handle: WindowHandle<workbench::Workbench>,
    out: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) {
    use super::pane_drag_evidence::{down, send, up};
    use fibo_core::layout::Axis;
    use gpui::InputEvent;
    std::fs::create_dir_all(out).unwrap();
    let original = handle
        .update(cx, |v, _, _| v.visual_resize_workspace())
        .unwrap();
    let mut failures = vec![];
    let draw = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            w.refresh();
            w.draw(a).clear(a);
        })
        .unwrap();
    };
    let shot = |name: String, cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, _| {
            w.render_to_image().unwrap().save(out.join(name)).unwrap();
        })
        .unwrap();
    };
    let ordinary = [
        (Axis::Down, false),
        (Axis::Right, false),
        (Axis::Down, true),
    ]
    .into_iter()
    .flat_map(|(axis, nested)| [1., 4., 7.].map(|grab| (axis, nested, grab, 8000, false)));
    let extra = [
        (Axis::Down, false, 4., 9800, false),
        (Axis::Down, false, 4., 200, false),
        (Axis::Right, true, 4., 4000, true),
    ];
    for (axis, nested, grab, initial, child_update) in ordinary.chain(extra) {
        let mut path = handle
            .update(cx, |v, _, a| {
                v.visual_resize_setup(original.clone(), axis, nested, initial, a)
            })
            .unwrap();
        if child_update {
            path.clear();
        }
        draw(cx);
        let mut stale = handle
            .update(cx, |v, _, _| v.visual_resize_workspace())
            .unwrap();
        let (from, usable) = handle
            .update(cx, |v, _, _| v.visual_resize_geometry(&path, axis, grab))
            .unwrap();
        let expected_at = |delta| {
            (initial as f32 + px(delta) / usable * 10000.)
                .clamp(initial.min(500) as f32, initial.max(9500) as f32)
                .round() as u16
        };
        let label =
            format!("{axis:?}-nested-{nested}-grab-{grab}-initial-{initial}-child-{child_update}");
        shot(format!("{label}-before.png"), cx);
        send(handle, cx, down(from));
        for (step, delta) in [1., 20., 19., -1.].into_iter().enumerate() {
            let position = from
                + match axis {
                    Axis::Down => gpui::point(px(0.), px(delta)),
                    Axis::Right => gpui::point(px(delta), px(0.)),
                };
            send(
                handle,
                cx,
                gpui::MouseMoveEvent {
                    position,
                    pressed_button: Some(gpui::MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
            );
            let actual = handle
                .update(cx, |v, _, _| v.visual_resize_ratio(&path))
                .unwrap();
            let expected = expected_at(delta);
            eprintln!("{label} step={step} delta={delta} actual={actual} expected={expected}");
            if actual != expected {
                failures.push(format!("{label} step {step}: {actual} != {expected}"));
            }
            if step == 0 {
                shot(format!("{label}-moved.png"), cx);
            }
            stale = handle
                .update(cx, |v, _, a| {
                    if child_update {
                        v.visual_resize_child_snapshot(a)
                    } else {
                        v.visual_resize_snapshot(a)
                    }
                })
                .unwrap();
            draw(cx);
            if step == 0 {
                shot(format!("{label}-refreshed.png"), cx);
            }
            if handle
                .update(cx, |v, _, _| v.visual_resize_ratio(&path))
                .unwrap()
                != actual
            {
                failures.push(format!("{label}: snapshot reset the divider"));
            }
        }
        let expected = expected_at(-1.);
        let release = from
            - match axis {
                Axis::Down => gpui::point(px(0.), px(1.)),
                Axis::Right => gpui::point(px(1.), px(0.)),
            };
        send(handle, cx, up(release));
        send(
            handle,
            cx,
            gpui::MouseMoveEvent {
                position: from,
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
        );
        assert_eq!(
            handle
                .update(cx, |v, _, _| v.visual_resize_ratio(&path))
                .unwrap(),
            expected,
            "hover after mouse-up must not resize"
        );
        // A snapshot already in flight can arrive after mouse-up, before the
        // resize receipt. Its revision is real, not artificially advanced.
        handle
            .update(cx, |v, _, a| v.visual_resize_apply(stale, a))
            .unwrap();
        draw(cx);
        if handle
            .update(cx, |v, _, _| v.visual_resize_ratio(&path))
            .unwrap()
            != expected
        {
            failures.push(format!("{label}: mouse-up snapshot reset the divider"));
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
            if handle
                .update(cx, |v, _, _| v.visual_resize_saved(&path, expected))
                .unwrap()
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "resize receipt timed out: {label}"
            );
        }
        if child_update {
            assert_eq!(
                handle
                    .update(cx, |v, _, _| v.visual_resize_ratio(&[true]))
                    .unwrap(),
                7000
            );
        }
        eprintln!("{label}: mouse-up persisted {expected} and reply kept the position");
    }
    std::fs::write(
        out.join("result.txt"),
        if failures.is_empty() {
            "PASS: both axes, nested split, three grab positions, reverse moves, incoming snapshots, mouse-up and saved ratios, existing 2%/98% ratios, child updates during parent drag"
                .into()
        } else {
            failures.join("\n")
        },
    )
    .unwrap();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
