use super::*;

// Runs a real Codex in the shown pane, then scrolls and moves the mouse over it the
// way a user does; reports whether any report reached Codex as typed text.
pub(super) fn start(handle: Option<WindowHandle<workbench::Workbench>>, cx: &mut App) {
    if let Some(handle) = &handle
        && let Ok(command) = std::env::var("MOORING_WHEEL_EVIDENCE")
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            cx.background_executor().timer(Duration::from_secs(3)).await;
            pane.update(cx, |p, cx| p.visual_type(&format!("{command}\r"), cx));
            cx.background_executor()
                .timer(Duration::from_secs(15))
                .await;
            let (tracking, center) = pane.read_with(cx, |p, _| p.visual_mouse_app());
            let center = center.expect("pane laid out");
            eprintln!("wheel evidence: tracking={tracking}");
            let rounds: usize = std::env::var("MOORING_WHEEL_ROUNDS")
                .ok()
                .and_then(|r| r.parse().ok())
                .unwrap_or(40);
            let gap: u64 = std::env::var("MOORING_WHEEL_GAP_MS")
                .ok()
                .and_then(|r| r.parse().ok())
                .unwrap_or(8);
            for i in 0..rounds {
                cx.update_window(handle.into(), |_, window, app| {
                    use gpui::InputEvent;
                    let at =
                        center + gpui::point(px((i % 20) as f32 * 7.), px((i % 7) as f32 * 9.));
                    window.dispatch_event(
                        gpui::MouseMoveEvent {
                            position: at,
                            pressed_button: None,
                            modifiers: Default::default(),
                        }
                        .to_platform_input(),
                        app,
                    );
                    window.dispatch_event(
                        gpui::ScrollWheelEvent {
                            position: at,
                            delta: gpui::ScrollDelta::Lines(gpui::point(
                                0.,
                                if i % 2 == 0 { -3. } else { 3. },
                            )),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }
                        .to_platform_input(),
                        app,
                    );
                })
                .unwrap();
                if gap > 0 {
                    cx.background_executor()
                        .timer(Duration::from_millis(gap))
                        .await;
                }
            }
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let text = pane.read_with(cx, |p, _| p.visual_text());
            eprintln!(
                "wheel evidence: pane error = {:?}",
                pane.read_with(cx, |p, _| p.visual_error())
            );
            let prompt = text
                .lines()
                .rfind(|l| l.trim_start().starts_with('›'))
                .unwrap_or("")
                .to_owned();
            eprintln!("wheel evidence: prompt line = {prompt:?}");
            eprintln!(
                "wheel evidence: stray report text = {}",
                ["<35", "<64", "<65", "[<"].iter().any(|m| text.contains(m))
            );
            // Leave Codex without submitting anything.
            pane.update(cx, |p, cx| p.visual_type("\x03\x03", cx));
            cx.background_executor().timer(Duration::from_secs(2)).await;
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
}
