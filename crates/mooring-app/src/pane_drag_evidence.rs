//! MOORING_PANE_DRAG_EVIDENCE: panes moved by dragging their title bars, against a real
//! host, each step saved as a picture and checked against the tab's layout:
//! 1. the top of two stacked panes onto the right edge of the other: side by side;
//! 2. one onto the middle of the other: they trade places;
//! 3. one rested on another tab: that tab opens, and the pane lands under its pane;
//! 4. one onto 「＋」: a tab of its own.
//!
//! It also times 300 drag moves that stay in one landing zone, the case a drag spends
//! most of its time in, which should not redraw anything.
use super::*;
use gpui::{InputEvent, MouseButton, Pixels, Point, WindowHandle, point};
use std::time::{Duration, Instant};
use workbench::Workbench;

type T = theme::Theme;

fn send(handle: WindowHandle<Workbench>, cx: &mut gpui::AsyncApp, input: gpui::PlatformInput) {
    cx.update_window(handle.into(), |_, w, a| {
        w.dispatch_event(input, a);
        w.refresh();
        w.draw(a).clear(a);
    })
    .unwrap();
}
fn down(at: Point<Pixels>) -> gpui::PlatformInput {
    gpui::MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Default::default(),
        click_count: 1,
        first_mouse: false,
    }
    .to_platform_input()
}
fn moved(at: Point<Pixels>) -> gpui::PlatformInput {
    gpui::MouseMoveEvent {
        position: at,
        pressed_button: Some(MouseButton::Left),
        modifiers: Default::default(),
    }
    .to_platform_input()
}
fn up(at: Point<Pixels>) -> gpui::PlatformInput {
    gpui::MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Default::default(),
        click_count: 1,
    }
    .to_platform_input()
}
/// Presses at `from` and moves to `to` in twelve steps, without letting go.
fn carry(
    handle: WindowHandle<Workbench>,
    cx: &mut gpui::AsyncApp,
    from: Point<Pixels>,
    to: Point<Pixels>,
) {
    send(handle, cx, down(from));
    for i in 1..=12 {
        let k = i as f32 / 12.;
        send(handle, cx, moved(from + (to - from) * k));
    }
}

pub(super) fn start(
    handle: Option<WindowHandle<Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_PANE_DRAG_EVIDENCE").is_none() {
        return false;
    }
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
        wait(2000).await;
        std::fs::create_dir_all(&directory).unwrap();
        let shot = |name: &str, cx: &mut gpui::AsyncApp| {
            let path = directory.join(name);
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh();
                w.draw(a).clear(a);
                w.render_to_image().unwrap().save(path).unwrap();
            })
            .unwrap();
        };
        let shown = |cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, _, _| v.visual_shown_layout())
                .unwrap()
        };
        let drop = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_drop()).unwrap();
        for _ in 0..2 {
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            wait(1500).await;
        }
        let home = drop(cx).0;
        handle
            .update(cx, |v, w, cx| v.visual_split_down(w, cx))
            .unwrap();
        wait(1500).await;
        let stacked = shown(cx);
        eprintln!("stacked: {stacked}");
        let (a, b) = stacked
            .trim_matches(['(', ')'])
            .split_once('/')
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .expect("two stacked panes");
        let area = handle.update(cx, |v, _, _| v.visual_bounds()).unwrap();
        let (l, t, w, h) = (area.left(), area.top(), area.size.width, area.size.height);
        let header = px(T::PANE_HEADER / 2.);
        shot("1-stacked.png", cx);

        // 1. The top pane onto the bottom one's right edge.
        let grip = point(l + w * 0.3, t + header);
        let edge = point(l + w - px(T::S20), t + h * 0.75);
        carry(handle, cx, grip, edge);
        shot("2-over-right-edge.png", cx);
        send(handle, cx, up(edge));
        wait(800).await;
        shot("3-side-by-side.png", cx);
        assert_eq!(shown(cx), format!("({b}|{a})"), "a lands right of b");

        // Moves inside one landing zone: nothing changes, so nothing should redraw.
        let grip = point(l + w * 0.75, t + header);
        let middle = point(l + w * 0.25, t + h * 0.5);
        carry(handle, cx, grip, middle);
        let at = Instant::now();
        for i in 0..300 {
            let jiggle = px((i % 7) as f32);
            cx.update_window(handle.into(), |_, win, app| {
                win.dispatch_event(moved(middle + point(jiggle, jiggle)), app);
            })
            .unwrap();
        }
        let per_move = at.elapsed().as_secs_f64() * 1e6 / 300.;
        eprintln!("PANE-DRAG 300 moves in one zone: {per_move:.1} µs per move");
        shot("4-over-middle.png", cx);

        // 2. Let go over the middle: the two trade places.
        send(handle, cx, up(middle));
        wait(800).await;
        shot("5-swapped.png", cx);
        assert_eq!(shown(cx), format!("({a}|{b})"), "a and b trade places");

        // 3. Rest a pane on the other tab: it opens, and the pane goes under its pane.
        let grip = point(l + w * 0.25, t + header);
        let row = T::TOP / 2.;
        carry(handle, cx, grip, point(px(T::SIDEBAR + 10.), px(row)));
        let mut over = None;
        for step in 0..80 {
            let x = px(T::SIDEBAR + 10. + 8. * step as f32);
            send(handle, cx, moved(point(x, px(row))));
            if let (_, Some(tab), _) = drop(cx)
                && Some(&tab) != home.as_ref()
            {
                over = Some((x, tab));
                break;
            }
        }
        let (x, other) = over.expect("another tab lights up under the dragged pane");
        shot("6-resting-on-tab.png", cx);
        wait(800).await;
        assert_eq!(drop(cx).0, Some(other), "resting opened the tab");
        let lower = point(l + w * 0.5, t + h - px(T::S20));
        for i in 1..=12 {
            let k = i as f32 / 12.;
            send(
                handle,
                cx,
                moved(point(x, px(row)) + (lower - point(x, px(row))) * k),
            );
        }
        shot("7-over-other-tab-pane.png", cx);
        send(handle, cx, up(lower));
        wait(800).await;
        shot("8-joined-other-tab.png", cx);
        let joined = shown(cx);
        eprintln!("other tab now: {joined}");
        assert!(
            joined.ends_with(&format!("/{a})")),
            "a under the other tab's pane"
        );

        // 4. The upper pane onto 「＋」: a tab of its own.
        carry(
            handle,
            cx,
            point(l + w * 0.3, t + header),
            point(x, px(row)),
        );
        let mut lit = false;
        for step in 0..80 {
            let px_x = x + px(8. * step as f32);
            send(handle, cx, moved(point(px_x, px(row))));
            if drop(cx).2 {
                lit = true;
                shot("9-over-plus.png", cx);
                send(handle, cx, up(point(px_x, px(row))));
                break;
            }
        }
        assert!(lit, "「＋」 lights up under a pane that can leave its tab");
        wait(1000).await;
        shot("10-own-tab.png", cx);
        let alone = shown(cx);
        eprintln!("own tab: {alone}");
        assert!(!alone.contains(['|', '/']), "one pane in its own tab");
        eprintln!("pane drag evidence PASS");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
