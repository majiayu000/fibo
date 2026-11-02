//! MOORING_CPU_PROBE: what the window costs, as this process's own CPU time. Phases are
//! comma separated:
//! - `frames-main`, `frames-overview`: one full redraw of that view, drawn 300 times.
//!   Anything that asks for a frame per vsync costs this times the display rate.
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
            let line = if let Some(view) = name.strip_prefix("frames-") {
                let overview = view == "overview";
                handle
                    .update(cx, |v, w, cx| v.visual_view(overview, w, cx))
                    .unwrap();
                wait(500).await;
                let (cpu, at) = (cpu_secs(), Instant::now());
                for _ in 0..300 {
                    cx.update_window(handle.into(), |_, w, a| {
                        w.refresh();
                        w.draw(a).clear(a);
                    })
                    .unwrap();
                }
                let used = cpu_secs() - cpu;
                handle
                    .update(cx, |v, w, cx| v.visual_view(false, w, cx))
                    .unwrap();
                format!(
                    "{name}: {:.2} ms CPU per full draw ({:.2} ms wall); at 120 fps {:.0}% of a core",
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
