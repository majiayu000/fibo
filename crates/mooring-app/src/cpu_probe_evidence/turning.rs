//! `frames-turning`: the running mark turning, from 设置 › 外观 › 动效. Needs rows running
//! on screen: `perf-fixture.py` with FEWNEED=1 at prepare and no NORUN.
//! - The cost of a step, drawn 300 times: the sidebar must keep its last draw.
//! - The mark really moves, in the sidebar: a number without that measures nothing.
//! - Folding what lists them takes the marks away and the steps stop; opening brings them back.
//! - Turned off, the view stops drawing.
use super::*;
use crate::ui::running_spin;
use crate::workbench::Workbench;

pub(super) async fn run(handle: WindowHandle<Workbench>, cx: &mut gpui::AsyncApp) -> String {
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
    // Nothing on screen to turn: the view stops asking, whatever is listed further down.
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

    cx.update(|a| running_spin::choose(false, a).unwrap());
    refreshed(cx);
    let off_rows = rows(cx);
    let asks = Workbench::visual_turning_asks();
    for _ in 0..3 {
        wait(150).await;
        frame(cx);
    }
    let off_draws = Workbench::visual_turning_asks() - asks;
    format!(
        "frames-turning: {used:.2} ms CPU per step, {ins:.2} M instructions; at rest {} rows {} draws; rows {turning}; moved {moved} px; sidebar kept its draw {kept}; folded {folded} drew {stopped} opened {opened}; off {off_rows} rows {off_draws} draws",
        at_rest.0, at_rest.1
    )
}
