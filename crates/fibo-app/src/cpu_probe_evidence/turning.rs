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
    // Tabs: the selected one shows × instead, so look from a tab with nothing running.
    handle
        .update(cx, |v, _, a| v.visual_turning_focus_idle(a))
        .unwrap();
    refreshed(cx);
    let tabs: Vec<_> = handle
        .update(cx, |v, _, _| v.visual_turning_frame_bounds())
        .unwrap()
        .into_iter()
        .filter(|b| f32::from(b.origin.y) < top)
        .collect();
    step(cx);
    let before = shot(cx);
    wait(350).await;
    step(cx);
    let after = shot(cx);
    after.save(directory.join("running-tabs.png")).unwrap();
    assert!(!tabs.is_empty(), "running tab did not register");
    for bound in &tabs {
        let x = (f32::from(bound.origin.x) * scale) as u32;
        let y = (f32::from(bound.origin.y) * scale) as u32;
        let size = (f32::from(bound.size.width) * scale) as u32;
        assert!(
            (y..(y + size).min(before.height())).any(|y| {
                (x..(x + size).min(before.width()))
                    .any(|x| before.get_pixel(x, y) != after.get_pixel(x, y))
            }),
            "a running tab mark stayed still: {bound:?}"
        );
    }
    // The column and both drawer headers use exactly the same overlay and setting.
    for drawer in [false, true] {
        handle
            .update(cx, |v, _, a| v.visual_turning_board(drawer, a))
            .unwrap();
        // An animation starts at its first draw, not at the state change. A covered
        // probe window may draw no automatic frame during the wait below.
        refreshed(cx);
        wait(600).await;
        refreshed(cx);
        let bounds = handle
            .update(cx, |v, _, _| v.visual_turning_frame_bounds())
            .unwrap();
        let board: Vec<_> = bounds
            .iter()
            .filter(|b| f32::from(b.origin.y) >= top)
            .collect();
        let state = handle
            .update(cx, |v, _, a| v.visual_turning_state(a))
            .unwrap();
        shot(cx)
            .save(directory.join(format!("board-{drawer}-registered.png")))
            .unwrap();
        assert!(
            board.len() >= if drawer { 2 } else { 1 },
            "board/drawer did not share the running clock: {board:?}; {state}"
        );
        step(cx);
        let before = shot(cx);
        wait(350).await;
        step(cx);
        let after = shot(cx);
        for b in board {
            let x = (f32::from(b.origin.x) * scale) as u32;
            let y = (f32::from(b.origin.y) * scale) as u32;
            let n = (f32::from(b.size.width) * scale) as u32;
            assert!(
                (y..(y + n).min(before.height())).any(|y| (x..(x + n).min(before.width()))
                    .any(|x| before.get_pixel(x, y) != after.get_pixel(x, y))),
                "board mark stayed still: {b:?}"
            );
        }
        before
            .save(directory.join(format!("running-board-{drawer}-before.png")))
            .unwrap();
        after
            .save(directory.join(format!("running-board-{drawer}-after.png")))
            .unwrap();
        cx.update(|a| running_spin::choose(false, a).unwrap());
        refreshed(cx);
        assert!(
            handle
                .update(cx, |v, _, _| v.visual_turning_frame_bounds().is_empty())
                .unwrap(),
            "board ignored the running-spin switch"
        );
        cx.update(|a| running_spin::choose(true, a).unwrap());
        refreshed(cx);
    }
    fibo_core::effect::set_reduce_motion(true);
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
    fibo_core::effect::set_reduce_motion(false);
    handle
        .update(cx, |v, _, a| {
            v.visual_turning_focus(a);
            v.visual_toggle_sidebar(a);
        })
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

/// A foreground-only native check, separate from the unattended performance gate.
pub(super) async fn board_motion(
    handle: WindowHandle<Workbench>,
    directory: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) -> String {
    let executor = cx.background_executor().clone();
    let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
    handle
        .update(cx, |v, _, a| v.visual_toggle_sidebar(a))
        .unwrap();
    let refreshed = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            w.refresh();
            w.draw(a).clear(a);
        })
        .unwrap();
    };
    let shot = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            w.draw(a).clear(a);
            w.render_to_image().unwrap()
        })
        .unwrap()
    };
    let scale = cx
        .update_window(handle.into(), |_, w, _| w.scale_factor())
        .unwrap();
    let top = crate::ui::theme::Theme::TOP;
    // The running overlay is not the board's only motion. The Needs halo and the
    // Running cards' gold sweep must also stop. Activate the probe: an inactive
    // window suppresses both even in the broken version and would prove nothing.
    handle
        .update(cx, |v, w, a| {
            v.visual_turning_board(false, a);
            w.activate_window();
            a.activate(true);
        })
        .unwrap();
    wait(300).await;
    cx.update(|a| running_spin::choose(false, a).unwrap());
    for reduced in [false, true] {
        // Reacquire focus for each capture pair; a different probe/window may have
        // become key while this foreground-only check changed its mode.
        cx.update_window(handle.into(), |_, w, a| {
            a.activate(true);
            w.activate_window();
        })
        .unwrap();
        fibo_core::effect::set_reduce_motion(reduced);
        refreshed(cx);
        wait(300).await;
        assert!(
            cx.update_window(handle.into(), |_, w, _| w.is_window_active())
                .unwrap(),
            "the motion probe must be active"
        );
        refreshed(cx);
        let before = shot(cx);
        wait(350).await;
        refreshed(cx);
        let after = shot(cx);
        assert!(
            cx.update_window(handle.into(), |_, w, _| w.is_window_active())
                .unwrap(),
            "the motion probe lost focus during capture"
        );
        before
            .save(directory.join(format!("board-reduced-{reduced}-before.png")))
            .unwrap();
        after
            .save(directory.join(format!("board-reduced-{reduced}-after.png")))
            .unwrap();
        // The sidebar is folded; five columns fill the window. Colored pixels
        // isolate the gold sweep and orange halo from elapsed-time text.
        for (column, name) in [(1, "running sweep"), (2, "needs halo")] {
            let warm = |p: &image::Rgba<u8>| {
                u16::from(p[0]) > u16::from(p[2]) * 3 / 2
                    && u16::from(p[1]) > u16::from(p[2]) * 3 / 2
            };
            let x0 = before.width() * column / 5;
            let x1 = before.width() * (column + 1) / 5;
            let moved = before
                .enumerate_pixels()
                .filter(|(x, y, _)| *x >= x0 && *x < x1 && *y > (top * scale) as u32)
                .filter(|(x, y, p)| {
                    let q = after.get_pixel(*x, *y);
                    *p != q && (warm(p) || warm(q))
                })
                .count();
            assert_eq!(
                moved == 0,
                reduced,
                "{name}: reduced={reduced}, moved={moved}"
            );
        }
    }

    fibo_core::effect::set_reduce_motion(false);
    "frames-board-motion: running sweep and needs halo move normally and stop with reduced motion"
        .into()
}
