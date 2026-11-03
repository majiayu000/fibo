//! MOORING_CPU_PROBE: what the window costs, as this process's own CPU time. Phases are
//! comma separated:
//! - `frames-main`, `frames-overview`: one full redraw of that view, drawn 300 times.
//!   Anything that asks for a frame per vsync costs this times the display rate.
//! - `frames-pane`: the frame terminal output asks for: only the shown pane notifies,
//!   so cached views elsewhere may be reused. Drawn 300 times.
//! - `redraw-check`: not a cost but a check that caching hides nothing: after each of a
//!   few clicks and a host change, a plain frame must match a refreshed one pixel for
//!   pixel (run with `MOORING_NO_CATCH_UP=1`), and the sidebar's catch-up must not
//!   keep the window drawing once nothing changes.
//!
//! Frames report retired instructions beside CPU time: on a busy machine the same frame
//! took 1.1 or 2.7 ms, but the same instructions to within 1%.
//! - `spinner:S`, `flood:S`: S seconds of a shown shell printing a status line ten times
//!   a second, or printing as fast as it can.
//!
//! Evidence windows are drawn off screen, so the display link never runs here: rates of
//! real frames are not measured, only the cost of each.
use super::*;
use gpui::WindowHandle;
use std::time::{Duration, Instant};

fn cpu_secs() -> f64 {
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    let t = |v: libc::timeval| v.tv_sec as f64 + v.tv_usec as f64 / 1e6;
    t(u.ru_utime) + t(u.ru_stime)
}

/// Instructions this process retired: unlike CPU time, the same work counts the same
/// on a busy machine, on an efficiency core or at a lower clock.
fn instructions() -> u64 {
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    let buffer = (&raw mut info).cast::<libc::rusage_info_t>();
    // SAFETY: the kernel writes one rusage_info_v4 into the local handed to it.
    let ok =
        unsafe { libc::proc_pid_rusage(std::process::id() as i32, libc::RUSAGE_INFO_V4, buffer) };
    if ok == 0 { info.ri_instructions } else { 0 }
}

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    let Ok(plan) = std::env::var("MOORING_CPU_PROBE") else {
        return false;
    };
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
        wait(4000).await;
        std::fs::create_dir_all(&directory).unwrap();
        handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
        wait(2000).await;
        let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
        let mut rows = Vec::new();
        for phase in plan.split(',') {
            let (name, secs) = match phase.split_once(':') {
                Some((n, s)) => (n, s.parse::<u64>().unwrap_or(15)),
                None => (phase, 15),
            };
            let line = if name == "redraw-check" {
                redraw_check(handle, &pane, &directory, cx).await
            } else if name == "frames-pane" {
                wait(500).await;
                let (cpu, at, ins) = (cpu_secs(), Instant::now(), instructions());
                for _ in 0..300 {
                    pane.update(cx, |_, cx| cx.notify());
                    cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
                        .unwrap();
                }
                let used = cpu_secs() - cpu;
                let ins = (instructions() - ins) as f64 / 300. / 1e6;
                format!(
                    "{name}: {:.2} ms CPU per pane frame, {ins:.2} M instructions ({:.2} ms wall); at 60 fps {:.0}% of a core",
                    used / 300. * 1000.,
                    at.elapsed().as_secs_f64() / 300. * 1000.,
                    used / 300. * 60. * 100.
                )
            } else if let Some(view) = name.strip_prefix("frames-") {
                let overview = view == "overview";
                handle
                    .update(cx, |v, w, cx| v.visual_view(overview, w, cx))
                    .unwrap();
                wait(500).await;
                let (cpu, at, ins) = (cpu_secs(), Instant::now(), instructions());
                for _ in 0..300 {
                    cx.update_window(handle.into(), |_, w, a| {
                        w.refresh();
                        w.draw(a).clear(a);
                    })
                    .unwrap();
                }
                let used = cpu_secs() - cpu;
                let ins = (instructions() - ins) as f64 / 300. / 1e6;
                handle
                    .update(cx, |v, w, cx| v.visual_view(false, w, cx))
                    .unwrap();
                format!(
                    "{name}: {:.2} ms CPU per full draw, {ins:.2} M instructions ({:.2} ms wall); at 120 fps {:.0}% of a core",
                    used / 300. * 1000.,
                    at.elapsed().as_secs_f64() / 300. * 1000.,
                    used / 300. * 120. * 100.
                )
            } else {
                let command = match name {
                    "spinner" => {
                        "i=0; while :; do i=$((i+1)); printf '\\r· Thinking… %d tokens' $i; sleep 0.1; done\r"
                    }
                    "flood" => {
                        "while :; do printf 'compiling crate %05d  0123456789 abcdefghij\\n' $RANDOM; done\r"
                    }
                    other => panic!("unknown CPU probe phase {other}"),
                };
                pane.update(cx, |p, cx| p.visual_type(command, cx));
                wait(1500).await;
                let (cpu, at) = (cpu_secs(), Instant::now());
                wait(secs * 1000).await;
                let (used, dt) = (cpu_secs() - cpu, at.elapsed().as_secs_f64());
                pane.update(cx, |p, cx| p.visual_type("\x03\rclear\r", cx));
                wait(1500).await;
                format!("{name}: {:.1}% of a core ({used:.2}s over {dt:.1}s)", used / dt * 100.)
            };
            eprintln!("CPU-PROBE {line}");
            rows.push(line);
        }
        std::fs::write(directory.join("cpu.txt"), rows.join("\n") + "\n").unwrap();
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}

/// Every change the window shows must be drawn without a full refresh: after each step,
/// a plain frame and a refreshed one are compared pixel for pixel. Terminal output must
/// leave the sidebar's last draw in place.
async fn redraw_check(
    handle: WindowHandle<workbench::Workbench>,
    pane: &gpui::Entity<crate::terminal::pane::Pane>,
    directory: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) -> String {
    let executor = cx.background_executor().clone();
    let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
    let check = |label: &str, cx: &mut gpui::AsyncApp| {
        let (cached, fresh) = cx
            .update_window(handle.into(), |_, w, app| {
                w.draw(app).clear(app);
                let cached = w.render_to_image().unwrap();
                w.refresh();
                w.draw(app).clear(app);
                (cached, w.render_to_image().unwrap())
            })
            .unwrap();
        let differ = cached
            .pixels()
            .zip(fresh.pixels())
            .filter(|(a, b)| a != b)
            .count();
        if differ > 0 || label == "rename" {
            cached
                .save(directory.join(format!("redraw-{label}-cached.png")))
                .unwrap();
            fresh
                .save(directory.join(format!("redraw-{label}-fresh.png")))
                .unwrap();
        }
        match differ {
            0 => format!("{label}: same"),
            n => format!("{label}: DIFF {n} px"),
        }
    };
    let frame = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, app| w.draw(app).clear(app))
            .unwrap();
    };
    let mut lines = vec![check("start", cx)];
    frame(cx);
    let drawn = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, cx| v.visual_sidebar_drawn(cx))
            .unwrap()
    };
    let before = drawn(cx);
    for _ in 0..30 {
        pane.update(cx, |_, cx| cx.notify());
        frame(cx);
    }
    let kept = before.is_some() && drawn(cx) == before;
    lines.push(format!("pane-frames: sidebar kept its draw {kept}"));
    pane.update(cx, |p, cx| p.visual_type("echo redraw-check\r", cx));
    wait(800).await;
    lines.push(check("pane-output", cx));
    for step in [
        "collapse",
        "expand",
        "filter",
        "unfilter",
        "focus",
        "pick-space",
        "rename",
    ] {
        handle
            .update(cx, |v, w, cx| v.visual_redraw_step(step, w, cx))
            .unwrap();
        if step == "rename" {
            let mut back = false;
            for _ in 0..100 {
                wait(100).await;
                back = handle.update(cx, |v, _, _| v.visual_renamed()).unwrap();
                if back {
                    break;
                }
            }
            lines.push(format!("rename came back {back}"));
        } else {
            wait(300).await;
        }
        lines.push(check(step, cx));
    }
    // Terminal output for 2.5 s: the sidebar catches up about once a second.
    let mut draws = 0;
    let mut last = drawn(cx);
    for _ in 0..25 {
        pane.update(cx, |_, cx| cx.notify());
        frame(cx);
        wait(100).await;
        let now = drawn(cx);
        draws += usize::from(now != last);
        last = now;
    }
    lines.push(format!("sidebar draws over 2.5 s of pane frames: {draws}"));
    // Then quiet. The sidebar draws, one pane frame sets a catch-up, and from then on a
    // frame is drawn only when a catch-up fires, as the display link would: the catch-up's
    // own frame must not set the next one.
    let catch_ups = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, cx| v.visual_catch_ups(cx))
            .unwrap()
    };
    handle
        .update(cx, |v, _, cx| v.visual_ask_sidebar(cx))
        .unwrap();
    frame(cx);
    let (start, ..) = catch_ups(cx);
    pane.update(cx, |_, cx| cx.notify());
    frame(cx);
    let mut was_due = catch_ups(cx).2;
    for _ in 0..50 {
        wait(100).await;
        let due = catch_ups(cx).2;
        if was_due && !due {
            frame(cx);
        }
        was_due = catch_ups(cx).2;
    }
    let (end, ..) = catch_ups(cx);
    lines.push(format!(
        "catch-ups after one pane frame and 5 quiet s: {} (1 expected)",
        end - start
    ));
    let line = lines.join("; ");
    eprintln!("CPU-PROBE redraw-check {line}");
    format!("redraw-check: {line}")
}
