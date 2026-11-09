use super::*;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_PAGE_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            handle
                .update(cx, |_, window, _| {
                    if window.viewport_size().height < px(theme::Theme::WINDOW_HEIGHT) {
                        window.toggle_simple_fullscreen();
                    }
                })
                .unwrap();
            cx.background_executor().timer(Duration::from_secs(1)).await;
            std::fs::create_dir_all(&directory).unwrap();
            let pages = std::env::var("MOORING_PAGE_LIST").unwrap_or_else(|_| {
                "Main,Overview,TodoNew,TodoDetail,TodoStart,Cleanup,Menubar".into()
            });
            for name in pages.split(',').map(str::to_owned).collect::<Vec<_>>() {
                let name = name.as_str();
                // `Page@640` renders the page in a window that wide; the file keeps the name.
                let (page, width) = match name.split_once('@') {
                    Some((page, width)) => {
                        (page, width.parse().unwrap_or(theme::Theme::WINDOW_WIDTH))
                    }
                    None => (name, theme::Theme::WINDOW_WIDTH),
                };
                handle
                    .update(cx, |view, window, cx| {
                        window.resize(size(px(width), px(theme::Theme::WINDOW_HEIGHT)));
                        window.bounds_changed(cx);
                        view.visual_page(page, window, cx);
                    })
                    .unwrap();
                if name == "HereTyped" {
                    use gpui::InputEvent;
                    // A plain keystroke into the terminal folds the card over it.
                    let key = gpui::KeyDownEvent {
                        keystroke: gpui::Keystroke::parse("x").unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    };
                    pane_drag_evidence::send(handle, cx, key.to_platform_input());
                    let open = handle.update(cx, |v, _, _| v.visual_here_open()).unwrap();
                    assert!(!open, "typing into the pane folds its 你在这里 card");
                    eprintln!("HereTyped: card folded after a keystroke");
                }
                if name == "HereClickOut" {
                    use gpui::InputEvent;
                    // A press anywhere outside the card puts it away.
                    cx.update_window(handle.into(), |_, w, a| {
                        w.refresh();
                        w.draw(a).clear(a);
                        w.dispatch_event(
                            gpui::MouseDownEvent {
                                button: gpui::MouseButton::Left,
                                position: gpui::point(px(8.), px(theme::Theme::WINDOW_HEIGHT - 8.)),
                                modifiers: Default::default(),
                                click_count: 1,
                                first_mouse: false,
                            }
                            .to_platform_input(),
                            a,
                        );
                    })
                    .unwrap();
                    let open = handle.update(cx, |v, _, _| v.visual_here_open()).unwrap();
                    assert!(!open, "a click outside folds its 你在这里 card");
                    eprintln!("HereClickOut: card folded after an outside click");
                }
                // A real drag on the board: pick a card up, carry it over to 做完了,
                // and let go, with shots while carried, in flight and after landing.
                if name == "BoardDrag" || name == "BoardRefuseDrag" {
                    use gpui::InputEvent;
                    cx.background_executor()
                        .timer(Duration::from_millis(1500))
                        .await;
                    cx.update_window(handle.into(), |_, w, a| {
                        w.refresh();
                        w.draw(a).clear(a);
                    })
                    .unwrap();
                    let (from, steps) = if name == "BoardDrag" {
                        ("review", 1.)
                    } else {
                        ("needs", 2.)
                    };
                    let (start, step) = handle
                        .update(cx, |v, _, _| {
                            (v.visual_card_center(from), v.visual_column_step())
                        })
                        .unwrap();
                    let start = start.expect("a card to drag on the board");
                    let target = gpui::point(start.x + step * steps, start.y + px(60.));
                    let shot = |label: &str, cx: &mut gpui::AsyncApp| {
                        let path = directory.join(format!("{}-{label}.png", name.to_lowercase()));
                        cx.update_window(handle.into(), |_, w, a| {
                            w.refresh();
                            w.draw(a).clear(a);
                            w.render_to_image().unwrap().save(path).unwrap();
                        })
                        .unwrap();
                    };
                    cx.update_window(handle.into(), |_, w, a| {
                        w.dispatch_event(
                            gpui::MouseDownEvent {
                                button: gpui::MouseButton::Left,
                                position: start,
                                modifiers: Default::default(),
                                click_count: 1,
                                first_mouse: false,
                            }
                            .to_platform_input(),
                            a,
                        );
                    })
                    .unwrap();
                    for n in 1..=12 {
                        let t = n as f32 / 12.;
                        let at = gpui::point(
                            start.x + (target.x - start.x) * t,
                            start.y + (target.y - start.y) * t,
                        );
                        cx.update_window(handle.into(), |_, w, a| {
                            w.dispatch_event(
                                gpui::MouseMoveEvent {
                                    position: at,
                                    pressed_button: Some(gpui::MouseButton::Left),
                                    modifiers: Default::default(),
                                }
                                .to_platform_input(),
                                a,
                            );
                            w.refresh();
                            w.draw(a).clear(a);
                        })
                        .unwrap();
                        cx.background_executor()
                            .timer(Duration::from_millis(30))
                            .await;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(400))
                        .await;
                    shot("1-carried", cx);
                    cx.update_window(handle.into(), |_, w, a| {
                        w.dispatch_event(
                            gpui::MouseUpEvent {
                                button: gpui::MouseButton::Left,
                                position: target,
                                modifiers: Default::default(),
                                click_count: 1,
                            }
                            .to_platform_input(),
                            a,
                        );
                    })
                    .unwrap();
                    cx.background_executor()
                        .timer(Duration::from_millis(120))
                        .await;
                    shot("2-flying", cx);
                    cx.background_executor()
                        .timer(Duration::from_millis(800))
                        .await;
                    shot("3-landed", cx);
                    eprintln!("{name}: dragged from {from} to done");
                }
                // Starting an Agent and placing the task text takes a few quiet checks.
                let settle = match name {
                    "TodoStart" => 4,
                    // A login shell with a rich prompt takes longer before the command goes in.
                    "ActionRun" => 6,
                    // The host reads the turn's edits before the panel has lines to show.
                    "ChangesOpen" => 3,
                    _ => 1,
                };
                cx.background_executor()
                    .timer(Duration::from_secs(settle))
                    .await;
                cx.update_window(handle.into(), |_, window, app| {
                    assert_eq!(
                        window.viewport_size(),
                        size(px(width), px(theme::Theme::WINDOW_HEIGHT))
                    );
                    window.refresh();
                    window.draw(app).clear(app);
                    let path = directory.join(format!("{}-native.png", name.to_lowercase()));
                    if name == "Menubar" {
                        desktop::snapshot(&path);
                    } else {
                        window.render_to_image().unwrap().save(path).unwrap();
                    }
                    eprintln!(
                        "{name}: native viewport {:?}, scale {}",
                        window.viewport_size(),
                        window.scale_factor()
                    );
                })
                .unwrap();
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
}
