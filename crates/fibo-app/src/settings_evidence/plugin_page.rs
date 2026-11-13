//! 插件 with real clicks, keys and pointing. MOORING_PLUGIN_POINTS are panel points: a card
//! in 全部, 查看全部, a text style's card once scrolled, its 安装并使用, ← 发现, the 外观 chip, the
//! theme card, ← 发现, the search field, 已安装, a row there.
use super::*;

pub(super) async fn run(
    handle: WindowHandle<workbench::Workbench>,
    directory: &Path,
    cx: &mut gpui::AsyncApp,
) {
    use gpui::InputEvent as _;
    let executor = cx.background_executor().clone();
    let wait = move |ms| executor.timer(Duration::from_millis(ms));
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
    let at = |x: f32, y: f32, w: &gpui::Window| {
        let size = w.viewport_size();
        gpui::point(
            (size.width - px(theme::Theme::SETTINGS_WIDTH)) / 2. + px(x),
            (size.height - px(664.)) / 2. + px(y),
        )
    };
    let click = |x: f32, y: f32, cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            let p = at(x, y, w);
            native_click(w, a, p, gpui::MouseButton::Left);
        })
        .unwrap();
    };
    let point_at = |x: f32, y: f32, cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            let position = at(x, y, w);
            w.dispatch_event(
                gpui::MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                a,
            );
        })
        .unwrap();
    };
    let points: Vec<(f32, f32)> = std::env::var("MOORING_PLUGIN_POINTS")
        .unwrap_or_default()
        .split(';')
        .filter_map(|p| p.split_once(','))
        .map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap()))
        .collect();
    handle
        .update(cx, |v, _, a| v.visual_settings_show("插件", a))
        .unwrap();
    wait(500).await;
    shot("page-discover", cx);
    use fibo_core::{effect, plugin};
    let mut left = points.into_iter();
    macro_rules! next {
        () => {
            match left.next() {
                Some(p) => p,
                None => {
                    eprintln!("settings plugin-page SHOTS");
                    cx.update(|a| a.quit());
                    return;
                }
            }
        };
    }
    // Pointing plays that one card and only that one, and leaving stops it.
    let card = next!();
    point_at(card.0, card.1, cx);
    wait(900).await;
    shot("page-hover-a", cx);
    wait(500).await;
    shot("page-hover-b", cx);
    point_at(5., 5., cx);
    wait(200).await;
    let all = next!();
    click(all.0, all.1, cx);
    wait(400).await;
    shot("page-animations", cx);
    // What the full grid costs a frame, standing still and with one card playing.
    let frames = |label: &str, cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            let mut took: Vec<_> = (0..30)
                .map(|_| {
                    let start = std::time::Instant::now();
                    w.refresh();
                    w.draw(a).clear(a);
                    start.elapsed()
                })
                .collect();
            took.sort();
            eprintln!(
                "settings plugin-page frame {label}: median {:?} worst {:?}",
                took[15], took[29]
            );
        })
        .unwrap();
    };
    frames("grid still", cx);
    point_at(card.0, card.1, cx);
    wait(500).await;
    frames("grid one playing", cx);
    point_at(5., 5., cx);
    wait(200).await;
    // The ones that move the terminal's own text, further down.
    cx.update_window(handle.into(), |_, w, a| {
        let position = at(600., 400., w);
        w.dispatch_event(
            gpui::ScrollWheelEvent {
                position,
                delta: gpui::ScrollDelta::Lines(gpui::point(0., -30.)),
                modifiers: Default::default(),
                touch_phase: gpui::TouchPhase::Moved,
            }
            .to_platform_input(),
            a,
        );
    })
    .unwrap();
    wait(300).await;
    shot("page-animations-more", cx);
    let text_card = next!();
    point_at(text_card.0, text_card.1, cx);
    wait(1500).await;
    shot("page-text-style-hover", cx);
    click(text_card.0, text_card.1, cx);
    wait(1400).await;
    shot("page-detail-playing", cx);
    wait(3500).await;
    shot("page-detail-stopped", cx);
    assert!(effect::picked().is_none(), "opening one picks nothing");
    let get = next!();
    click(get.0, get.1, cx);
    wait(2000).await;
    shot("page-detail-installed", cx);
    let count = plugin::report().iter().filter(|f| f.result.is_ok()).count();
    assert_eq!(count, 1, "安装并使用 from the detail view installs it");
    assert!(
        effect::picked().is_some(),
        "安装并使用 makes it the one playing"
    );
    let back = next!();
    click(back.0, back.1, cx);
    wait(400).await;
    shot("page-back", cx);
    cx.update_window(handle.into(), |_, w, a| {
        let position = at(600., 400., w);
        w.dispatch_event(
            gpui::ScrollWheelEvent {
                position,
                delta: gpui::ScrollDelta::Lines(gpui::point(0., 60.)),
                modifiers: Default::default(),
                touch_phase: gpui::TouchPhase::Moved,
            }
            .to_platform_input(),
            a,
        );
    })
    .unwrap();
    wait(300).await;
    let look = next!();
    click(look.0, look.1, cx);
    wait(400).await;
    shot("page-look", cx);
    let theme_card = next!();
    click(theme_card.0, theme_card.1, cx);
    wait(400).await;
    shot("page-detail-theme", cx);
    let back = next!();
    click(back.0, back.1, cx);
    wait(300).await;
    let search = next!();
    click(search.0, search.1, cx);
    wait(100).await;
    cx.update_window(handle.into(), |_, w, a| {
        w.refresh();
        w.draw(a).clear(a);
        for key in ["z", "z", "z"] {
            w.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), a);
        }
    })
    .unwrap();
    wait(300).await;
    shot("page-search-empty", cx);
    let installed = next!();
    click(installed.0, installed.1, cx);
    wait(400).await;
    shot("page-installed", cx);
    let row = next!();
    click(row.0, row.1, cx);
    wait(400).await;
    shot("page-detail-from-installed", cx);
    // The same in light: stages, chips and the filled button read from the palette.
    let back = next!();
    click(back.0, back.1, cx);
    wait(200).await;
    let discover = next!();
    click(discover.0, discover.1, cx);
    wait(200).await;
    cx.update(|a| crate::ui::appearance::set_mode(crate::ui::appearance::Mode::Light, a).unwrap());
    wait(400).await;
    shot("page-light", cx);
    eprintln!("settings plugin-page PASS");
    cx.update(|a| a.quit());
}
