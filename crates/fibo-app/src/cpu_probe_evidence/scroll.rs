//! `frames-scroll`: an Agent's output scrolling by, one line a frame. The shown pane holds
//! lines as an Agent prints them (Chinese that differs line to line, box drawing,
//! bullets), and each frame shows
//! history one row further down: every row's text moved up a row since the last frame.
//! After each, a frame of the same screen, as a spinner or the turning mark draws between
//! two lines. 300 such pairs, reported per pair. `frames-pane` draws the same screen again and again, so a cache by
//! row alone looks free there and costs a CoreText layout per run here.
use super::*;
use crate::workbench::Workbench;

pub(super) async fn run(
    handle: WindowHandle<Workbench>,
    pane: &gpui::Entity<crate::terminal::pane::Pane>,
    cx: &mut gpui::AsyncApp,
) -> String {
    const LINES: usize = 800;
    const FRAMES: usize = 300;
    let executor = cx.background_executor().clone();
    let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
    pane.update(cx, |p, cx| {
        p.visual_type(
            &format!(
                "clear; perl -CS -e 'for $i (1..{LINES}) {{ print \"\\x{{23FA}} \", join(\"\", map {{ chr(0x4e00 + ($i * 131 + $_ * 17) % 6000) }} 0..17), \" \\x{{2500}}\\x{{2500}} \\x{{2502}} \\x{{2713}} $i\\n\" }}'\r"
            ),
            cx,
        )
    });
    let total = |cx: &mut gpui::AsyncApp| pane.read_with(cx, |p, _| p.visual_viewport().1);
    let at = Instant::now();
    // The pane learns how much history there is when it draws, and nothing draws an
    // off-screen window but the probe.
    let draw = |cx: &mut gpui::AsyncApp| {
        pane.update(cx, |_, cx| cx.notify());
        cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
            .unwrap();
    };
    while total(cx) < LINES && at.elapsed() < Duration::from_secs(60) {
        wait(200).await;
        draw(cx);
    }
    let rows = total(cx);
    assert!(rows >= LINES, "the shell printed {rows} of {LINES} lines");
    // Between two lines of output the window draws frames where the pane shows the same
    // screen (a spinner, the running mark turning): one such frame after each line.
    let frame = |row: usize, cx: &mut gpui::AsyncApp| {
        pane.update(cx, |p, cx| p.visual_scroll_row(row, cx));
        cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
            .unwrap();
        draw(cx);
    };
    frame(0, cx);
    use std::sync::atomic::Ordering::Relaxed;
    let shaped = crate::terminal::screen::SHAPED.load(Relaxed);
    let (cpu, at, ins) = (cpu_secs(), Instant::now(), instructions());
    for row in 1..=FRAMES {
        frame(row, cx);
    }
    let used = cpu_secs() - cpu;
    let ins = (instructions() - ins) as f64 / FRAMES as f64 / 1e6;
    let shaped = (crate::terminal::screen::SHAPED.load(Relaxed) - shaped) as f64 / FRAMES as f64;
    // A number without the scroll measures a still screen.
    let shown = pane.read_with(cx, |p, _| p.visual_viewport().0);
    assert_eq!(shown, FRAMES, "the pane did not scroll");
    pane.update(cx, |p, cx| p.visual_type("clear\r", cx));
    wait(500).await;
    format!(
        "frames-scroll: {:.2} ms CPU per line scrolled and a still frame, {ins:.2} M instructions ({:.2} ms wall), {shaped:.1} runs shaped",
        used / FRAMES as f64 * 1000.,
        at.elapsed().as_secs_f64() / FRAMES as f64 * 1000.,
    )
}
