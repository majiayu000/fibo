use super::*;

// Selection against the pixels: each row holds `ab`, a middle group and `cd`, apart
// by four blanks. The middle group's ink is found in the screenshot, the drag runs
// from its first to its last visible ink, and the highlight that appears must cover
// that ink and none of the outer groups'. Repeated after the sidebar folds and the
// text zooms, so stale pane bounds or cell metrics show as a miss.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_SELECTION_PIXEL_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            let pane = handle.update(cx, |view, _, _| view.visual_shown_pane()).unwrap();
            const ROWS: [&str; 6] = ["中文选区测试", "⏺ ⎿ ──── ✻", "🙂🚀✅", "hello_world", "中a文b⏺c", "▐▛███▜▌"];
            let script: String = ROWS.iter().map(|m| format!(" 'ab    {m}    cd'")).collect();
            pane.update(cx, |p, cx| p.visual_type(&format!("clear; printf '%s\\n'{script}\r"), cx));
            for _ in 0..150 {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if pane.update(cx, |p, _| p.visual_text_shown()).lines().next().is_some_and(|l| l.starts_with("ab")) { break; }
            }
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let frame = |name: &str, window: &mut gpui::Window, app: &mut App| {
                window.refresh(); window.draw(app).clear(app);
                let image = window.render_to_image().unwrap();
                image.save(directory.join(format!("{name}.png"))).unwrap();
                image
            };
            // Cell size and grid origin, as the pane maps them (logical px).
            let grid = |p: &crate::terminal::pane::Pane| {
                let (a, b, c) = (p.visual_cell(0, 0).unwrap(), p.visual_cell(0, 1).unwrap(), p.visual_cell(1, 0).unwrap());
                let (w, h) = (f32::from(b.x - a.x), f32::from(c.y - a.y));
                (f32::from(a.x) - w / 2., f32::from(a.y) - h / 2., w, h, p.visual_bounds().unwrap())
            };
            let drag = |window: &mut gpui::Window, app: &mut App, from: gpui::Point<gpui::Pixels>, to: gpui::Point<gpui::Pixels>, clicks: usize| {
                window.dispatch_event(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position: from, modifiers: Default::default(), click_count: clicks, first_mouse: false }.to_platform_input(), app);
                if from != to {
                    window.dispatch_event(gpui::MouseMoveEvent { position: to, pressed_button: Some(gpui::MouseButton::Left), modifiers: Default::default() }.to_platform_input(), app);
                }
                window.dispatch_event(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position: to, modifiers: Default::default(), click_count: clicks }.to_platform_input(), app);
            };
            let differs = |a: &[u8; 4], b: &[u8; 4], by: i32| (0..3).any(|i| (a[i] as i32 - b[i] as i32).abs() > by);
            let mut failures: Vec<String> = Vec::new();
            type Change = fn(&mut crate::workbench::Workbench, &gpui::Entity<crate::terminal::pane::Pane>, &mut gpui::Context<crate::workbench::Workbench>);
            let states: [(&str, Change); 4] = [
                ("initial", |_, _, _| {}),
                ("sidebar-folded", |w, _, cx| w.visual_toggle_sidebar(cx)),
                ("zoomed-in", |_, p, cx| { p.update(cx, |p, cx| { p.visual_zoom(Some(1), cx); p.visual_zoom(Some(1), cx); p.visual_zoom(Some(1), cx); }); }),
                ("restored", |w, p, cx| { w.visual_toggle_sidebar(cx); p.update(cx, |p, cx| p.visual_zoom(None, cx)); }),
            ];
            for (state, change) in states {
                // Where a click lands in the frame the change happens in, before it redraws.
                let before_frame = pane.read_with(cx, |p, _| p.visual_bounds());
                handle.update(cx, |w, _, cx| change(w, &pane, cx)).unwrap();
                let stale = pane.read_with(cx, |p, _| p.visual_bounds());
                cx.background_executor().timer(Duration::from_millis(800)).await;
                // Clear any selection with a click on a blank row, then the clean frame.
                let blank = pane.read_with(cx, |p, _| p.visual_cell(ROWS.len() as u16 + 3, 2).unwrap());
                let (s, clean, (ox, oy, w, h, bounds)) = cx.update_window(handle.into(), |_, window, app| {
                    drag(window, app, blank, blank, 1);
                    (window.scale_factor(), frame(&format!("{state}-0-clean"), window, app), grid(pane.read(app)))
                }).unwrap();
                eprintln!("[{state}] scale {s} cell {w:.3}x{h:.3} origin ({ox:.2},{oy:.2}); bounds before change {:?} -> right after (no frame) {:?} -> after frame {:?}",
                    before_frame.map(|b| b.origin), stale.map(|b| b.origin), bounds.origin);
                let bg = clean.get_pixel(((f32::from(bounds.right()) - 4. * w) * s) as u32, ((oy + h * (ROWS.len() as f32 + 1.5)) * s) as u32).0;
                for (row, middle) in ROWS.iter().enumerate() {
                    let (y0, y1) = (((oy + h * row as f32) * s).round() as u32, ((oy + h * (row + 1) as f32) * s).round() as u32);
                    let x_end = ((f32::from(bounds.right()) - 2. * w) * s) as u32;
                    let ink_col = |x: u32| (y0..y1).any(|y| differs(&clean.get_pixel(x, y).0, &bg, 60));
                    // Ink clusters split by gaps of 2.5 cells or more.
                    let mut clusters: Vec<(u32, u32)> = Vec::new();
                    for x in ((ox * s) as u32)..x_end {
                        if !ink_col(x) { continue; }
                        match clusters.last_mut() {
                            Some(c) if (x - c.1) as f32 <= 2.5 * w * s => c.1 = x,
                            _ => clusters.push((x, x)),
                        }
                    }
                    if clusters.len() != 3 {
                        failures.push(format!("[{state}] row {row} {middle:?}: {} ink groups, expected 3: {clusters:?}", clusters.len()));
                        continue;
                    }
                    let mid = clusters[1];
                    let ink_rows: Vec<u32> = (y0..y1).filter(|&y| (mid.0..=mid.1).any(|x| differs(&clean.get_pixel(x, y).0, &bg, 60))).collect();
                    let (iy0, iy1) = (ink_rows[0], *ink_rows.last().unwrap());
                    let at = |x: u32| gpui::point(px(x as f32 / s), px((iy0 + iy1) as f32 / 2. / s));
                    let (shot, cols, text) = cx.update_window(handle.into(), |_, window, app| {
                        // A third of a cell outside the ink on each side: Ghostty takes a cell once the
                        // pointer passes 60% of it, so this takes exactly the cells the ink sits in.
                        let third = (w * s * 0.3) as u32;
                        drag(window, app, at(mid.0.saturating_sub(third)), at(mid.1 + third), 1);
                        let shot = frame(&format!("{state}-{}-{row}", 1), window, app);
                        let p = pane.read(app);
                        (shot, p.visual_selection_cols(), p.visual_selection().1)
                    }).unwrap();
                    // Highlighted = changed from the clean frame, within this row's band.
                    let lit_col = |x: u32| (y0..y1).any(|y| differs(&shot.get_pixel(x, y).0, &clean.get_pixel(x, y).0, 6));
                    let lit: Vec<u32> = ((ox * s) as u32..x_end).filter(|&x| lit_col(x)).collect();
                    let Some((&hx0, &hx1)) = lit.first().zip(lit.last()) else {
                        failures.push(format!("[{state}] row {row} {middle:?}: nothing highlighted; cols {cols:?}"));
                        continue;
                    };
                    let lit_rows: Vec<u32> = (y0.saturating_sub(4)..y1 + 4).filter(|&y| (hx0..=hx1).any(|x| differs(&shot.get_pixel(x, y).0, &clean.get_pixel(x, y).0, 6))).collect();
                    let (hy0, hy1) = (lit_rows[0], *lit_rows.last().unwrap());
                    let line = format!(
                        "[{state}] row {row} {middle:?}: ink x {}..{} y {iy0}..{iy1} | highlight x {hx0}..{hx1} y {hy0}..{hy1} | cells {:?} | copied {text:?}",
                        mid.0, mid.1, cols.get(row).copied().flatten());
                    eprintln!("{line}");
                    let mut wrong = Vec::new();
                    if hx0 > mid.0 || hx1 < mid.1 { wrong.push(format!("ink sticks out left {} / right {} px", mid.0 as i64 - hx0 as i64, mid.1 as i64 - hx1 as i64)); }
                    if hx0 <= clusters[0].1 || hx1 >= clusters[2].0 { wrong.push("highlight reaches ab or cd".into()); }
                    if hy0 > iy0 || hy1 < iy1 { wrong.push(format!("ink sticks out top {} / bottom {} px", iy0 as i64 - hy0 as i64, iy1 as i64 - hy1 as i64)); }
                    if lit_rows.iter().any(|&y| y < y0.saturating_sub(1) || y > y1) { wrong.push(format!("highlight leaves the row band {y0}..{y1}")); }
                    if text.as_deref() != Some(*middle) { wrong.push(format!("copied {text:?}")); }
                    if !wrong.is_empty() { failures.push(format!("{line} => {}", wrong.join("; "))); }
                }
            }
            if failures.is_empty() {
                eprintln!("selection pixels PASS: every middle group's ink sits inside its highlight, in all layouts");
            } else {
                eprintln!("selection pixels FAIL ({}):", failures.len());
                for f in &failures { eprintln!("  {f}"); }
            }
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
