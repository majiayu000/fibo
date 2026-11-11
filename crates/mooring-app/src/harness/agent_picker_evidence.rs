//! Real input and host readback for the default Agent dropdown.
use super::*;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let Ok(phase) = std::env::var("MOORING_AGENT_PICKER_EVIDENCE") else {
        return false;
    };
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    assert!(
        std::env::var_os("XDG_CONFIG_HOME").is_some(),
        "isolated config required"
    );
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        wait(2000).await;
        std::fs::create_dir_all(&directory).unwrap();
        handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
        wait(300).await;
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
        let click = |x: f32, y: f32, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                let origin = gpui::point(
                    (w.viewport_size().width - px(theme::Theme::SETTINGS_WIDTH)) / 2.,
                    (w.viewport_size().height - px(664.)) / 2.,
                );
                native_click(
                    w,
                    a,
                    origin + gpui::point(px(x), px(y)),
                    gpui::MouseButton::Left,
                );
            })
            .unwrap();
        };
        let key = |key: &str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), a);
                w.refresh();
                w.draw(a).clear(a);
            })
            .unwrap();
        };
        let menu_open = |cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, _, a| v.visual_agent_menu_open(a))
                .unwrap()
        };
        let selected = |cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, _, _| v.visual_settings_values().default_agent)
                .unwrap()
        };
        use mooring_core::model::Agent;
        shot("closed", cx);
        if phase == "readback" {
            assert_eq!(
                selected(cx),
                Agent::Claude,
                "previous process saved its choice"
            );
        } else {
            let original = selected(cx);
            click(810., 148., cx);
            wait(200).await;
            assert!(menu_open(cx), "click opens the dropdown");
            shot("open", cx);
            click(810., 148., cx);
            wait(200).await;
            assert!(
                !menu_open(cx),
                "clicking its trigger again closes the dropdown"
            );
            click(810., 148., cx);
            wait(200).await;
            key("escape", cx);
            wait(200).await;
            assert!(
                handle
                    .update(cx, |v, _, _| v.visual_settings_open())
                    .unwrap(),
                "Esc closes the picker, not settings"
            );
            assert_eq!(selected(cx), original, "cancel keeps the saved choice");
            assert!(!menu_open(cx));
            shot("escape", cx);
            click(810., 148., cx);
            wait(200).await;
            click(300., 90., cx);
            wait(200).await;
            assert!(!menu_open(cx));
            shot("outside", cx);
            assert_eq!(selected(cx), original, "outside click does not save");
            // Every registered agent, including the last item, can be chosen by keyboard.
            let agents = Agent::all();
            for _ in 0..agents.len() {
                let before = selected(cx);
                let i = agents.iter().position(|a| *a == before).unwrap();
                click(810., 148., cx);
                wait(100).await;
                key("down", cx);
                key("enter", cx);
                wait(600).await;
                assert_eq!(
                    selected(cx),
                    agents[(i + 1) % agents.len()],
                    "keyboard choice reached the host"
                );
            }
            // Mouse selection of Claude, the second menu row.
            click(810., 148., cx);
            wait(200).await;
            click(810., 215., cx);
            wait(600).await;
            assert_eq!(selected(cx), Agent::Claude, "mouse choice reached the host");
            handle
                .update(cx, |v, _, a| v.visual_settings_close(a))
                .unwrap();
            handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
            wait(200).await;
            assert_eq!(
                selected(cx),
                Agent::Claude,
                "reopened panel keeps the choice"
            );
            shot("saved", cx);
        }
        eprintln!("agent picker {phase} PASS");
        cx.update(|a| a.quit());
    })
    .detach();
    true
}
