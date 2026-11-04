//! MOORING_SUBAGENT_EVIDENCE: the `-marks` sidebar drawn in both themes, then real mouse
//! events: a click on a branch of the other session (at the y in the variable's value)
//! opens that session, as a click on its row does.
use super::pane_drag_evidence::{down, send, up};
use super::*;
use gpui::{Pixels, Point, WindowHandle, point};
use std::time::Duration;
use workbench::Workbench;

pub(super) fn start(
    handle: Option<WindowHandle<Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    let Ok(y) = std::env::var("MOORING_SUBAGENT_EVIDENCE") else {
        return false;
    };
    let at: Point<Pixels> = point(px(120.), px(y.parse().expect("the branch's y")));
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        executor.timer(Duration::from_secs(2)).await;
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
        let page = |name: &str, cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, w, cx| v.visual_page(name, w, cx))
                .unwrap();
        };
        page("Dense-first-standard-light-marks", cx);
        shot("marks-light.png", cx);
        page("Dense-first-standard-dark-marks", cx);
        shot("marks-dark.png", cx);

        let before = handle.update(cx, |v, _, _| v.visual_nav()).unwrap();
        send(handle, cx, down(at));
        send(handle, cx, up(at));
        shot("marks-clicked.png", cx);
        let after = handle.update(cx, |v, _, _| v.visual_nav()).unwrap();
        eprintln!("subagent click at y={y}: {before:?} -> {after:?}");
        assert_ne!(before, after, "a branch's click opens its session");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
