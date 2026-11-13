//! 自己画一个 with real clicks and keys. MOORING_FACE_CLICKS: 自己画一个, frame 2, eye *, the
//! mouth field, 播放, the name field, 在窗格上试播, 保存并用它, as panel points.
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
    let click = |x: f32, y: f32, cx: &mut gpui::AsyncApp| {
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
    };
    use fibo_core::{effect, plugin};
    let points: Vec<(f32, f32)> = std::env::var("MOORING_FACE_CLICKS")
        .unwrap_or_default()
        .split(';')
        .filter_map(|p| p.split_once(','))
        .map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap()))
        .collect();
    let keys = |text: &str, cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            // A field takes characters once it has been drawn focused.
            w.refresh();
            w.draw(a).clear(a);
            for key in text.chars() {
                w.dispatch_keystroke(gpui::Keystroke::parse(&key.to_string()).unwrap(), a);
            }
        })
        .unwrap();
    };
    handle
        .update(cx, |v, _, a| v.visual_settings_show("插件", a))
        .unwrap();
    wait(400).await;
    shot("faces-start", cx);
    let [draw, second, star, mouth, play, name, try_it, save] = points[..] else {
        if let Some(&(x, y)) = points.first() {
            click(x, y, cx);
            wait(300).await;
            shot("faces-open", cx);
        }
        eprintln!("settings faces SHOTS");
        cx.update(|a| a.quit());
        return;
    };
    click(draw.0, draw.1, cx);
    wait(300).await;
    shot("faces-open", cx);
    click(second.0, second.1, cx);
    wait(200).await;
    click(star.0, star.1, cx);
    wait(200).await;
    shot("faces-eyes", cx);
    click(mouth.0, mouth.1, cx);
    wait(100).await;
    keys("v", cx);
    wait(200).await;
    shot("faces-too-wide", cx);
    cx.update_window(handle.into(), |_, w, a| {
        w.dispatch_keystroke(gpui::Keystroke::parse("backspace").unwrap(), a)
    })
    .unwrap();
    click(play.0, play.1, cx);
    wait(1500).await;
    shot("faces-playing", cx);
    wait(1800).await;
    click(name.0, name.1, cx);
    keys("mine", cx);
    wait(200).await;
    shot("faces-named", cx);
    let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
    pane.update(cx, |p, a| {
        p.visual_type("printf 'trying a drawn face\\n'\r", a)
    });
    wait(300).await;
    click(try_it.0, try_it.1, cx);
    wait(1200).await;
    shot("faces-on-pane", cx);
    assert!(
        handle
            .update(cx, |v, w, a| v.visual_settings(w, a))
            .unwrap()
            .is_none(),
        "试播 closes 设置"
    );
    wait(2200).await;
    handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
    wait(300).await;
    handle
        .update(cx, |v, _, a| v.visual_settings_show("插件", a))
        .unwrap();
    wait(300).await;
    shot("faces-draft-row", cx);
    click(draw.0, draw.1, cx);
    wait(300).await;
    shot("faces-draft-open", cx);
    click(save.0, save.1, cx);
    wait(600).await;
    shot("faces-saved", cx);
    assert!(
        plugin::installed("my-face-1").is_some(),
        "保存 writes and loads the plugin"
    );
    assert_eq!(
        effect::picked(),
        Some(effect::Pick::Plugin("my-face-1".into())),
        "and plays it"
    );
    let c = effect::complete();
    let face = c.face.expect("the drawn frames");
    assert_eq!(
        face.sheet().frames[1].left,
        "*",
        "the eye clicked on frame 2"
    );
    assert_eq!(
        face.sheet().frames[1].mouth,
        "_",
        "backspace undid the too-wide mouth"
    );
    eprintln!("settings faces PASS");
    cx.update(|a| a.quit());
}
