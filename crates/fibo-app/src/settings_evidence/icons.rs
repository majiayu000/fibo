//! 应用图标 with real clicks. MOORING_ICON_CLICKS: the tiles as panel points, `x,y;x,y;x,y`.
//! After each click: the settings page, the saved choice and the Dock as the system draws it.
use super::*;

pub(super) async fn run(
    handle: WindowHandle<workbench::Workbench>,
    directory: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let executor = cx.background_executor().clone();
    let wait = move |ms| executor.timer(Duration::from_millis(ms));
    let shot = |name: &str, cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            w.refresh();
            w.draw(a).clear(a);
            let file = directory.join(format!("{name}.png"));
            w.render_to_image().unwrap().save(file).unwrap();
        })
        .unwrap();
    };
    handle
        .update(cx, |v, _, a| v.visual_settings_show("外观", a))
        .unwrap();
    wait(400).await;
    // 应用图标 sits below the fold: scroll the page the way a wheel does.
    cx.update_window(handle.into(), |_, w, a| {
        let size = w.viewport_size();
        w.dispatch_event(
            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                position: gpui::point(size.width / 2. + px(150.), size.height / 2.),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-1200.))),
                ..Default::default()
            }),
            a,
        );
    })
    .unwrap();
    wait(500).await;
    shot("icons-start", cx);
    let started = cx.update(|a| a.global::<crate::ui::app_icon::Chosen>().icon.label());
    eprintln!("icon at start: {started}");
    let clicks = std::env::var("MOORING_ICON_CLICKS").unwrap_or_default();
    for (i, point) in clicks.split(';').filter(|p| !p.is_empty()).enumerate() {
        let (x, y) = point.split_once(',').expect("x,y");
        let (x, y): (f32, f32) = (x.parse().unwrap(), y.parse().unwrap());
        cx.update_window(handle.into(), |_, w, a| {
            let size = w.viewport_size();
            let origin = gpui::point(
                (size.width - px(theme::Theme::SETTINGS_WIDTH)) / 2.,
                (size.height - px(664.)) / 2.,
            );
            native_click(
                w,
                a,
                origin + gpui::point(px(x), px(y)),
                gpui::MouseButton::Left,
            );
        })
        .unwrap();
        wait(1200).await;
        shot(&format!("icons-{i}"), cx);
        let chosen = cx.update(|a| {
            let c = a.global::<crate::ui::app_icon::Chosen>();
            (
                c.icon.label(),
                std::fs::read_to_string(&c.file).unwrap_or_default(),
            )
        });
        eprintln!("icon click {i}: chosen={} saved={}", chosen.0, chosen.1);
        // MOORING_ICON_DOCK: the screen rect holding the Dock, `x,y,w,h`, drawn by the system.
        if let Ok(rect) = std::env::var("MOORING_ICON_DOCK") {
            let file = directory.join(format!("dock-{i}.png"));
            let _ = std::process::Command::new("screencapture")
                .args(["-x", &format!("-R{rect}")])
                .arg(file)
                .status();
        }
    }
    eprintln!("icon clicks done");
    cx.update(|a| a.quit());
}
