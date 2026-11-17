//! MOORING_TAB_DRAG_EVIDENCE: a tab carried along the strip (direction C), against a
//! real host (scripts/tab-drag-scenes.py), each step saved as a picture:
//! 1. picked up and moved right in the strip: the tab stays on its row, a neighbour
//!    gives way, nothing is pulled out;
//! 2. pulled down over the panes: its panes hang under it and the gap is outlined;
//! 3. moved right while out: the gap follows;
//! 4. let go over the panes: the strip keeps the order the gap showed;
//! 5. carried and let go over the sidebar: nothing moves.
use super::pane_drag_evidence::{down, send, up};
use super::*;
use gpui::{InputEvent, MouseButton, Pixels, Point, point};

type T = theme::Theme;

fn moved(at: Point<Pixels>) -> gpui::PlatformInput {
    gpui::MouseMoveEvent {
        position: at,
        pressed_button: Some(MouseButton::Left),
        modifiers: Default::default(),
    }
    .to_platform_input()
}

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_TAB_DRAG_EVIDENCE").is_none() {
        return false;
    }
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
        wait(2500).await;
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
        let lift = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, a| v.visual_tab_lift(a)).unwrap();
        let titles = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_tab_titles()).unwrap();
        let before = titles(cx);
        eprintln!("tabs: {before:?}");
        shot("0-before.png", cx);

        // 1. The first tab, a little right of the sidebar on the title row.
        let row = px(T::TOP / 2.);
        let grab = point(px(T::SIDEBAR + 48.), row);
        send(handle, cx, down(grab));
        let mut x = grab.x;
        let mut shown = Vec::new();
        // GPUI starts the drag on the first move and reports moves from the next one.
        for step in 0..80 {
            x += px(6.);
            send(handle, cx, moved(point(x, row)));
            let (lifted, order, pulled) = lift(cx);
            if step >= 2 {
                assert_eq!(lifted.as_deref(), Some(before[0].as_str()), "the first tab is carried");
            }
            assert!(!pulled, "on the title row nothing is pulled out");
            if order != before {
                shown = order;
                break;
            }
        }
        assert!(!shown.is_empty(), "a neighbour gives way in the strip");
        eprintln!("in the strip: {shown:?}");
        shot("1-in-strip.png", cx);

        // 2. Down over the panes: the tab grows its panes.
        let low = row + px(T::S20 * 7.);
        for i in 1..=8 {
            send(handle, cx, moved(point(x, row + (low - row) * (i as f32 / 8.))));
        }
        let (_, at_pull, pulled) = lift(cx);
        assert!(pulled, "pulled out over the panes");
        assert_eq!(at_pull, shown, "pulling out keeps the gap");
        shot("2-pulled-out.png", cx);

        // 3. Right while out: the gap follows the pointer along the strip.
        let mut out_order = at_pull.clone();
        for _ in 0..60 {
            x += px(6.);
            send(handle, cx, moved(point(x, low)));
            let (_, order, pulled) = lift(cx);
            assert!(pulled);
            if order != at_pull {
                out_order = order;
                break;
            }
        }
        assert_ne!(out_order, at_pull, "the gap follows the pointer while out");
        eprintln!("out, moved right: {out_order:?}");
        shot("3-out-moved.png", cx);

        // 4. Let go over the panes: the order the gap showed.
        send(handle, cx, up(point(x, low)));
        wait(1000).await;
        shot("4-dropped.png", cx);
        assert_eq!(titles(cx), out_order, "the tab lands where its gap was");

        // 5. Carried, then let go over the sidebar: nothing moves.
        let landed = titles(cx);
        send(handle, cx, down(grab));
        for i in 1..=12 {
            send(handle, cx, moved(point(grab.x + px(14. * i as f32), row)));
        }
        let away = point(px(T::SIDEBAR / 2.), px(T::TOP * 4.));
        for i in 1..=8 {
            let k = i as f32 / 8.;
            send(handle, cx, moved(point(grab.x + px(168.), row) + (away - point(grab.x + px(168.), row)) * k));
        }
        send(handle, cx, up(away));
        wait(1000).await;
        shot("5-cancelled.png", cx);
        assert_eq!(titles(cx), landed, "let go over the sidebar, the strip is unchanged");

        std::fs::write(
            directory.join("result.txt"),
            "PASS: tab carried in the strip, pulled out with its panes, dropped in its gap, cancelled over the sidebar\n",
        )
        .unwrap();
        eprintln!("tab drag evidence PASS");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
