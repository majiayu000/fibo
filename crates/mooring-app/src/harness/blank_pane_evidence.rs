use super::*;
use std::time::Instant;

// Switching tabs builds a new pane that has nothing to show until its snapshot arrives.
// The host is stopped so the snapshot cannot beat the frame: the waiting pane must show
// the theme's ground, not black, and the screen that follows must sit on the same color.
// Then the two tabs swap repeatedly and the time to each pane's first screen is printed.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_BLANK_PANE_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
            wait(2000).await;
            std::fs::create_dir_all(&directory).unwrap();
            for _ in 0..2 {
                handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
                wait(1500).await;
                // A long session's scrollback, so the snapshot loses the race to the first frame.
                let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
                pane.update(cx, |p, cx| p.visual_type("yes \"$(printf '\\033[31mred\\033[32mgreen\\033[0m %.0s' {1..12})\" | head -n 60000\r", cx));
                wait(4000).await;
            }
            let shot = |name: &str, window: &mut gpui::Window, app: &mut App| {
                window.refresh();
                window.draw(app).clear(app);
                let image = window.render_to_image().unwrap();
                image.save(directory.join(format!("{name}.png"))).unwrap();
                image
            };
            // The bottom right corner: no text, status line or cursor reaches it.
            let corner = |view: &workbench::Workbench, window: &gpui::Window, app: &App| {
                let bounds = view
                    .visual_terminal_bounds(&view.visual_shown_terminal(), app)
                    .expect("the pane painted");
                let scale = window.scale_factor();
                (
                    ((f32::from(bounds.right()) - 4.) * scale) as u32,
                    ((f32::from(bounds.bottom()) - 4.) * scale) as u32,
                )
            };
            let rgb = |p: [u8; 4]| (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]);
            let runtime = handle
                .update(cx, |v, _, _| v.visual_socket())
                .unwrap()
                .parent()
                .unwrap()
                .display()
                .to_string();
            let signal = |flag: &str| {
                let found = std::process::Command::new("/usr/bin/pkill")
                    .args([flag, "-f", &format!("mooring serve --runtime {runtime}")])
                    .status()
                    .unwrap();
                assert!(found.success(), "no host to signal");
            };
            let root = handle.update(cx, |_, _, cx| cx.entity()).unwrap();
            signal("-STOP");
            handle
                .update(cx, |v, window, cx| v.visual_show_other_tab(window, cx))
                .unwrap();
            let (waiting, first) = cx
                .update_window(handle.into(), |_, window, app| {
                    let image = shot("waiting", window, app);
                    let view = root.read(app);
                    let (x, y) = corner(view, window, app);
                    let first = view.visual_shown_pane().read(app).visual_first_screen();
                    (rgb(image.get_pixel(x, y).0), first)
                })
                .unwrap();
            signal("-CONT");
            assert!(!first, "the snapshot beat the stopped host");
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            let started = Instant::now();
            let mut noted = false;
            while !pane.read_with(cx, |p, _| p.visual_first_screen()) {
                wait(5).await;
                note_stall(&pane, started, &mut noted, cx);
            }
            let shown = cx
                .update_window(handle.into(), |_, window, app| {
                    let image = shot("content", window, app);
                    let (x, y) = corner(root.read(app), window, app);
                    rgb(image.get_pixel(x, y).0)
                })
                .unwrap();
            let theme = theme::Theme::terminal_bg();
            // Plain switches, with the host running: how long the screen takes, and how long
            // it lags the snapshot once that has arrived.
            let (mut totals, mut lags) = (Vec::new(), Vec::new());
            for _ in 0..60 {
                wait(120).await;
                let started = Instant::now();
                handle
                    .update(cx, |v, window, cx| v.visual_show_other_tab(window, cx))
                    .unwrap();
                let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
                let mut arrived = None;
                let mut noted = false;
                loop {
                    let (vt, shown) = pane
                        .read_with(cx, |p, _| (p.visual_vt_ready(), p.visual_first_screen()));
                    if vt && arrived.is_none() {
                        arrived = Some(Instant::now());
                    }
                    if shown {
                        break;
                    }
                    note_stall(&pane, started, &mut noted, cx);
                    wait(1).await;
                }
                totals.push(started.elapsed().as_secs_f64() * 1000.);
                lags.push(arrived.map_or(0., |a| a.elapsed().as_secs_f64() * 1000.));
            }
            for (name, times) in [("switch to screen", &mut totals), ("snapshot to screen", &mut lags)] {
                times.sort_by(f64::total_cmp);
                eprintln!(
                    "blank pane: {name} p50 {:.1} ms, p90 {:.1} ms, max {:.1} ms",
                    times[times.len() / 2],
                    times[times.len() * 9 / 10],
                    times[times.len() - 1],
                );
            }
            eprintln!("blank pane: theme {theme:06x}, waiting {waiting:06x}, content {shown:06x}");
            let close = |a: u32, b: u32| {
                [16, 8, 0].iter().all(|s| ((a >> s) & 255).abs_diff((b >> s) & 255) <= 3)
            };
            let pass = close(waiting, theme) && close(shown, theme);
            eprintln!("blank pane {}", if pass { "PASS" } else { "FAIL" });
            std::process::exit(if pass { 0 } else { 1 });
        })
        .detach();
        return true;
    }
    false
}
/// A first screen 3 s late is reported with the pane's state; only 30 s fails, which
/// tells a loaded machine (recovers) from a lost repaint (never does). A locked screen or
/// a sleeping display draws no frames at all, and stalls every build: run it awake.
fn note_stall(
    pane: &gpui::Entity<crate::terminal::pane::Pane>,
    started: Instant,
    noted: &mut bool,
    cx: &mut gpui::AsyncApp,
) {
    let waited = started.elapsed();
    if waited > Duration::from_secs(3) && !*noted {
        *noted = true;
        let state = pane.read_with(cx, |p, _| p.visual_probe());
        eprintln!("blank pane STALL at 3 s (vt, captured, rows, dirty, error): {state:?}");
    }
    assert!(
        waited < Duration::from_secs(30),
        "no first screen after 30 s"
    );
}
