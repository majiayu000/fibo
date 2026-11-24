//! Typing into a form draws the form: the sidebar and tab strip keep their last draw.
use super::*;

pub(super) fn typing_check(
    handle: WindowHandle<workbench::Workbench>,
    cx: &mut gpui::AsyncApp,
) -> String {
    use workbench::Workbench;

    const BATCH: usize = 30;
    let frame = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
            .unwrap();
    };
    let sidebar = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, cx| v.visual_sidebar_drawn(cx))
            .unwrap()
            .expect("sidebar must be on screen")
    };
    let image = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, _| w.render_to_image().unwrap())
            .unwrap()
    };
    let field = handle
        .update(cx, |v, window, cx| v.visual_task_form(window, cx))
        .unwrap();
    frame(cx);
    frame(cx);
    let before = image(cx);
    let mut last = sidebar(cx);
    let mut draws = 0;
    let strip = Workbench::visual_strip_draws();
    for i in 0..BATCH {
        // What a key press does to the field: new text, then the field notifies.
        field.update(cx, |f, cx| {
            let text = format!("{}{}", f.text, char::from(b'a' + (i % 26) as u8));
            f.set_text(&text);
            cx.notify();
        });
        frame(cx);
        let now = sidebar(cx);
        draws += usize::from(now != last);
        last = now;
    }
    let strip = Workbench::visual_strip_draws() - strip;
    let pixels = before
        .pixels()
        .zip(image(cx).pixels())
        .filter(|(a, b)| a != b)
        .count();
    handle
        .update(cx, |v, _, cx| v.visual_close_form(cx))
        .unwrap();
    frame(cx);
    format!(
        "typing-check: {draws} sidebar draws, {strip} strip draws for {BATCH} keys; {pixels} px changed"
    )
}
