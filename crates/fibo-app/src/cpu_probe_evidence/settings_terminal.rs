use super::*;

pub(super) fn run(
    handle: WindowHandle<workbench::Workbench>,
    directory: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) -> String {
    handle
        .update(cx, |v, w, cx| {
            v.show_settings(w, cx);
            v.visual_settings_show("终端", cx);
        })
        .unwrap();
    cx.update_window(handle.into(), |_, w, a| {
        const STEPS: usize = 45;
        const FRAMES: usize = STEPS * 4;
        let position = gpui::point(w.viewport_size().width / 2., w.viewport_size().height / 2.);
        let shot = |w: &mut gpui::Window, name| {
            let image = w.render_to_image().unwrap();
            image.save(directory.join(name)).unwrap();
            image
        };
        let scroll = |w: &mut gpui::Window, a: &mut App, dy| {
            w.dispatch_event(
                gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                    position,
                    delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))),
                    modifiers: Default::default(),
                    touch_phase: gpui::TouchPhase::Moved,
                }),
                a,
            );
            w.draw(a).clear(a);
        };
        w.draw(a).clear(a);
        let top = shot(w, "terminal-scroll-top.png");
        for _ in 0..STEPS {
            scroll(w, a, -20.);
        }
        assert_ne!(
            top.as_raw(),
            shot(w, "terminal-scroll-down.png").as_raw(),
            "the settings page must scroll"
        );
        for _ in 0..STEPS {
            scroll(w, a, 20.);
        }
        let mut samples = Vec::with_capacity(FRAMES);
        let before = instructions();
        for dy in [-20., 20., -20., 20.] {
            for _ in 0..STEPS {
                let start = Instant::now();
                scroll(w, a, dy);
                samples.push(start.elapsed());
            }
        }
        let millions = (instructions() - before) as f64 / FRAMES as f64 / 1e6;
        samples.sort_unstable();
        format!(
            "frames-settings-terminal: {FRAMES} wheel frames, {:.2} ms median, {:.2} ms p95, {millions:.2} M instructions/frame",
            samples[FRAMES / 2].as_secs_f64() * 1000.,
            samples[FRAMES * 95 / 100].as_secs_f64() * 1000.,
        )
    })
    .unwrap()
}
