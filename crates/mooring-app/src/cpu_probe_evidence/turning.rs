//! `frames-turning`: the running mark turning, from 设置 › 外观 › 动效. Needs rows running
//! on screen: `perf-fixture.py` with FEWNEED=1 at prepare and no NORUN.
//! - The cost of a step, drawn 300 times: the sidebar must keep its last draw.
//! - The mark really moves, in the sidebar: a number without that measures nothing.
//! - Folding what lists them takes the marks away and the steps stop; opening brings them back.
//! - Turned off, the view stops drawing.
use super::*;
use crate::ui::running_spin;
use crate::workbench::Workbench;

pub(super) async fn run(
    handle: WindowHandle<Workbench>,
    directory: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) -> String {
    const KEY: &str = "recent:running";
    const COLUMN: u32 = 2 * 316;
    let executor = cx.background_executor().clone();
    let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
    let frame = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
            .unwrap();
    };
    let refreshed = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            w.refresh();
            w.draw(a).clear(a);
        })
        .unwrap();
    };
    let rows = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, _| v.visual_turning_rows())
            .unwrap()
    };
    let step = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, a| v.visual_turning_step(a))
            .unwrap();
    };

    refreshed(cx);
    let at_rest = (rows(cx), Workbench::visual_turning_asks());
    cx.update(|a| running_spin::choose(true, a).unwrap());
    refreshed(cx);
    frame(cx);
    let turning = rows(cx);

    // Only the column's pixels: the terminal's cursor blinks on its own clock.
    let shot = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            w.draw(a).clear(a);
            w.render_to_image().unwrap()
        })
        .unwrap()
    };
    step(cx);
    let before = shot(cx);
    wait(350).await;
    step(cx);
    let after = shot(cx);
    let moved = before
        .enumerate_pixels()
        .filter(|(x, ..)| *x < COLUMN)
        .zip(after.enumerate_pixels().filter(|(x, ..)| *x < COLUMN))
        .filter(|((.., a), (.., b))| a != b)
        .count();

    wait(500).await;
    frame(cx);
    let drawn = handle
        .update(cx, |v, _, cx| v.visual_sidebar_drawn(cx))
        .unwrap();
    let (cpu, ins) = (cpu_secs(), instructions());
    for _ in 0..300 {
        step(cx);
        frame(cx);
    }
    let used = (cpu_secs() - cpu) / 300. * 1000.;
    let ins = (instructions() - ins) as f64 / 300. / 1e6;
    let kept = handle
        .update(cx, |v, _, cx| v.visual_sidebar_drawn(cx))
        .unwrap()
        == drawn;

    let fold = |key: &str, cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, cx| v.visual_toggle_group(key, cx))
            .unwrap();
    };
    fold(KEY, cx);
    fold("recent:today", cx);
    wait(600).await;
    refreshed(cx);
    let folded = rows(cx);
    let main_rows = handle
        .update(cx, |v, _, _| v.visual_turning_frame_bounds().len())
        .unwrap();
    // Folded rows stop contributing; visible tab marks may still keep the clock alive.
    frame(cx);
    let asks = Workbench::visual_turning_asks();
    for _ in 0..3 {
        wait(150).await;
        frame(cx);
    }
    let stopped = Workbench::visual_turning_asks() - asks;
    fold(KEY, cx);
    fold("recent:today", cx);
    wait(600).await;
    refreshed(cx);
    let opened = rows(cx);

    // The sidebar can disappear while tabs and pane headers still need the same clock.
    handle
        .update(cx, |v, _, a| {
            v.visual_turning_focus(a);
            v.visual_toggle_sidebar(a);
        })
        .unwrap();
    refreshed(cx);
    let bounds = handle
        .update(cx, |v, _, _| v.visual_turning_frame_bounds())
        .unwrap();
    assert!(
        !bounds.is_empty(),
        "running headers/tabs did not register with the shared clock"
    );
    let scale = cx
        .update_window(handle.into(), |_, w, _| w.scale_factor())
        .unwrap();
    step(cx);
    let before = shot(cx);
    wait(350).await;
    step(cx);
    let after = shot(cx);
    before
        .save(directory.join("running-main-before.png"))
        .unwrap();
    after
        .save(directory.join("running-main-after.png"))
        .unwrap();
    let top = crate::ui::theme::Theme::TOP;
    assert!(
        bounds.iter().any(|b| f32::from(b.origin.y) < top),
        "running tab did not register"
    );
    assert!(
        bounds.iter().any(|b| f32::from(b.origin.y) >= top),
        "running pane header did not register"
    );
    for bound in &bounds {
        let x = (f32::from(bound.origin.x) * scale) as u32;
        let y = (f32::from(bound.origin.y) * scale) as u32;
        let size = (f32::from(bound.size.width) * scale) as u32;
        assert!(
            (y..(y + size).min(before.height())).any(|y| {
                (x..(x + size).min(before.width()))
                    .any(|x| before.get_pixel(x, y) != after.get_pixel(x, y))
            }),
            "a running mark stayed still with the sidebar collapsed: {bound:?}"
        );
    }
    mooring_core::effect::set_reduce_motion(true);
    refreshed(cx);
    assert!(
        handle
            .update(cx, |v, _, _| v.visual_turning_frame_bounds().is_empty())
            .unwrap()
    );
    let asks = Workbench::visual_turning_asks();
    wait(150).await;
    frame(cx);
    assert_eq!(
        Workbench::visual_turning_asks(),
        asks,
        "reduce motion kept stepping"
    );
    mooring_core::effect::set_reduce_motion(false);
    handle
        .update(cx, |v, _, a| v.visual_toggle_sidebar(a))
        .unwrap();
    cx.update(|a| running_spin::choose(false, a).unwrap());
    refreshed(cx);
    let off_rows = rows(cx);
    assert!(
        handle
            .update(cx, |v, _, _| v.visual_turning_frame_bounds().is_empty())
            .unwrap()
    );
    let asks = Workbench::visual_turning_asks();
    for _ in 0..3 {
        wait(150).await;
        frame(cx);
    }
    let off_draws = Workbench::visual_turning_asks() - asks;
    format!(
        "frames-turning: {used:.2} ms CPU per step, {ins:.2} M instructions; at rest {} rows {} draws; rows {turning}; moved {moved} px; sidebar kept its draw {kept}; folded {folded} drew {stopped} main {main_rows} opened {opened}; off {off_rows} rows {off_draws} draws",
        at_rest.0, at_rest.1
    )
}
