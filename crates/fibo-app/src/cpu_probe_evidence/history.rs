use super::*;
use workbench::Workbench;

type Handle = WindowHandle<Workbench>;

fn frame(handle: Handle, cx: &mut gpui::AsyncApp) {
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
}

pub(super) async fn run(
    handle: Handle,
    directory: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) -> String {
    let (base, active, history) = handle
        .update(cx, |v, window, cx| v.visual_history_fixture(window, cx))
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(600))
        .await;
    handle
        .update(cx, |v, _, cx| v.visual_history_apply(base.clone(), cx))
        .unwrap();
    frame(handle, cx);
    let max = handle
        .update(cx, |v, _, _| v.visual_list_scroll().2)
        .unwrap();
    assert!(max > 500., "the history must extend below the viewport");
    let snapshot = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, _| w.render_to_image().unwrap())
            .unwrap()
    };
    let mut rows = vec![];
    let mut changed = base.clone();
    Workbench::visual_history_waiting(&mut changed, &active);
    handle
        .update(cx, |v, _, cx| v.visual_history_apply(changed.clone(), cx))
        .unwrap();
    frame(handle, cx);
    for (name, offset) in [("top", 0.), ("middle", max * 0.6), ("bottom", max)] {
        let capture = |full, cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, _, cx| v.visual_set_list_scroll(0., cx))
                .unwrap();
            frame(handle, cx);
            Workbench::visual_full_history(full);
            handle
                .update(cx, |v, _, cx| v.visual_set_list_scroll(offset, cx))
                .unwrap();
            frame(handle, cx);
            let skipped = Workbench::visual_offscreen_history();
            let geometry = handle
                .update(cx, |v, _, _| (v.visual_list_scroll(), v.visual_list_rows()))
                .unwrap();
            (snapshot(cx), skipped, geometry)
        };
        // At the same offset, force both paths to run through the sidebar. The test
        // covers the first frame after the jump, when no row at the new offset is cached.
        let (whole, _, a) = capture(true, cx);
        let (lazy, skipped, b) = capture(false, cx);
        assert_eq!(whole.dimensions(), lazy.dimensions());
        let pixels = whole
            .pixels()
            .zip(lazy.pixels())
            .filter(|(x, y)| x != y)
            .count();
        if pixels > 0 || a != b {
            whole
                .save(directory.join(format!("history-{name}-full.png")))
                .unwrap();
            lazy.save(directory.join(format!("history-{name}-visible.png")))
                .unwrap();
        }
        assert!(skipped > 0, "{name}: no history row was outside the clip");
        assert_eq!(a, b, "{name}: row positions or scroll changed");
        assert_eq!(pixels, 0, "{name}: history pixels changed");
        let cost = |full, cx: &mut gpui::AsyncApp| {
            Workbench::visual_full_history(full);
            let at = instructions();
            for _ in 0..12 {
                handle
                    .update(cx, |v, _, cx| v.visual_set_list_scroll(offset, cx))
                    .unwrap();
                frame(handle, cx);
            }
            (instructions() - at) as f64 / 12e6
        };
        let before = cost(true, cx);
        let after = cost(false, cx);
        let after = (after + cost(false, cx)) / 2.;
        let before = (before + cost(true, cx)) / 2.;
        rows.push(format!("{name} {skipped} skipped, {pixels} differing pixels, {before:.2} -> {after:.2} M instructions"));
    }
    // The row with an open summary can change height and must keep its normal layout.
    handle
        .update(cx, |v, _, cx| {
            v.visual_history_expand(&history, cx);
        })
        .unwrap();
    frame(handle, cx);
    let top = handle
        .update(cx, |v, _, _| {
            v.visual_list_rows()
                .into_iter()
                .find(|row| row.0 == history)
                .unwrap()
                .1
        })
        .unwrap();
    let offset = (top - 24.).max(0.);
    handle
        .update(cx, |v, _, cx| v.visual_set_list_scroll(offset, cx))
        .unwrap();
    frame(handle, cx);
    let previous = snapshot(cx);
    std::sync::Arc::make_mut(changed.content.get_mut(&history).unwrap()).last_reply =
        "Updated summary from the fixture".into();
    handle
        .update(cx, |v, _, cx| v.visual_history_apply(changed, cx))
        .unwrap();
    frame(handle, cx);
    let summary = snapshot(cx);
    let changed_pixels = previous
        .pixels()
        .zip(summary.pixels())
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed_pixels > 0, "history summary remained stale");
    Workbench::visual_full_history(true);
    handle
        .update(cx, |v, _, cx| v.visual_set_list_scroll(offset, cx))
        .unwrap();
    frame(handle, cx);
    let reference = snapshot(cx);
    assert!(
        summary.pixels().eq(reference.pixels()),
        "open history summary differs"
    );
    Workbench::visual_full_history(false);
    format!(
        "history-check: {}; expanded summary updated {changed_pixels} px",
        rows.join("; ")
    )
}
