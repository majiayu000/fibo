//! MOORING_SIDEBAR_ROWS_EVIDENCE: the list with ten to act on and eleven running, in a real
//! window with real wheel events and clicks. Every row is listed; opening a session out of
//! sight brings it into sight; scrolling away from it holds; rows changing elsewhere leave
//! what you read where it is; at the top a new row is seen; a fold is not held against.
use super::pane_drag_evidence::send;
use super::*;
use gpui::{WindowHandle, point};
use mooring_core::sidebar::Bucket;
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
    if std::env::var_os("MOORING_SIDEBAR_ROWS_EVIDENCE").is_none() {
        return false;
    }
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
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
        let scroll =
            |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_list_scroll()).unwrap();
        let rows =
            |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_list_rows()).unwrap();
        let box_of = |id: &str, cx: &mut gpui::AsyncApp| {
            let all = handle.update(cx, |v, _, _| v.visual_list_rows()).unwrap();
            all.into_iter()
                .find(|r| r.0 == id)
                .map(|r| (r.1, r.2))
                .unwrap_or_else(|| panic!("{id} is not listed"))
        };
        handle
            .update(cx, |v, w, cx| {
                v.visual_page("Dense-first-standard-light-crowded", w, cx)
            })
            .unwrap();
        wait(400).await;
        shot("rows-tall.png", cx);
        // Short enough that the list has to scroll.
        cx.update_window(handle.into(), |_, w, _| {
            w.resize(gpui::size(px(1000.), px(460.)))
        })
        .unwrap();
        wait(800).await;
        shot("rows-top.png", cx);

        // 1. Every row, whatever the other group holds.
        let all = rows(cx);
        let count = |b: &[Bucket]| all.iter().filter(|r| b.contains(&r.3)).count();
        assert_eq!(
            count(&[Bucket::Attention, Bucket::Done]),
            10,
            "to act on: {all:?}"
        );
        assert_eq!(count(&[Bucket::Running]), 11, "running: {all:?}");
        let (at, height, max) = scroll(cx);
        eprintln!("scroll {at} height {height} max {max}, rows {}", all.len());
        assert!(
            max > 0.,
            "this window must be short enough for the list to scroll"
        );
        assert_eq!(at, 0., "crowd-s0 is at the top");

        // 2. A real wheel moves the one list.
        let over = point(px(120.), px(300.));
        send(
            handle,
            cx,
            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                position: over,
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-60.))),
                ..Default::default()
            }),
        );
        wait(100).await;
        let wheeled = scroll(cx).0;
        assert_eq!(wheeled, 60., "the wheel scrolls the column");

        // 3. Opening a session out of sight brings its whole row into sight.
        handle
            .update(cx, |v, _, cx| v.visual_focus_session("crowd-t20", cx))
            .unwrap();
        wait(200).await;
        shot("rows-opened.png", cx);
        let (at, height, _) = scroll(cx);
        let (top, bottom) = box_of("crowd-s20", cx);
        eprintln!(
            "opened: scroll {at}, row {top}..{bottom}, shows {at}..{}",
            at + height
        );
        assert!(
            top >= at && bottom <= at + height + 0.5,
            "the row is out of sight"
        );

        // 4. It does that once: scroll away and later draws leave you there.
        handle
            .update(cx, |v, _, cx| v.visual_set_list_scroll(40., cx))
            .unwrap();
        wait(200).await;
        handle.update(cx, |_, _, cx| cx.notify()).unwrap();
        wait(200).await;
        assert_eq!(
            scroll(cx).0,
            40.,
            "a redraw pulled the list back to the open session"
        );

        // 5. A running row finishes and joins those to act on, which pushes the running rows newer than it
        // down in the content (crowd-s10 and s11, s12 being the one that left). Read one of
        // those: it stays put on screen.
        let read = "crowd-s11";
        let (top, _) = box_of(read, cx);
        handle
            .update(cx, |v, _, cx| v.visual_set_list_scroll(top - 40., cx))
            .unwrap();
        wait(200).await;
        let screen = top - scroll(cx).0;
        assert_eq!(
            screen,
            40.,
            "the scroll reached the row to read: {:?}",
            scroll(cx)
        );
        handle
            .update(cx, |v, _, cx| v.visual_finish_turn("crowd-s12", cx))
            .unwrap();
        wait(200).await;
        shot("rows-held.png", cx);
        let (moved, _) = box_of(read, cx);
        let after = moved - scroll(cx).0;
        eprintln!("held: {read} content {top} to {moved}, on screen {screen} then {after}");
        assert!(
            moved > top,
            "nothing pushed the row, so this proves nothing"
        );
        assert!((after - screen).abs() < 1., "the list jumped under you");

        // 6. At the very top nothing is held: the row that joined above is seen.
        handle
            .update(cx, |v, _, cx| v.visual_set_list_scroll(0., cx))
            .unwrap();
        wait(200).await;
        handle
            .update(cx, |v, _, cx| v.visual_finish_turn("crowd-s13", cx))
            .unwrap();
        wait(200).await;
        assert_eq!(scroll(cx).0, 0., "at the top the list stays at the top");

        // 7. The real next-waiting key, from the far end of the list: the row it lands on
        // comes into sight.
        let far = scroll(cx).2;
        handle
            .update(cx, |v, _, cx| v.visual_set_list_scroll(far, cx))
            .unwrap();
        wait(200).await;
        handle
            .update(cx, |v, w, cx| v.visual_next_waiting(w, cx))
            .unwrap();
        wait(300).await;
        shot("rows-next.png", cx);
        let (at, height, _) = scroll(cx);
        let (top, bottom) = handle
            .update(cx, |v, _, _| v.visual_shown_row())
            .unwrap()
            .expect("the next-waiting key landed on no row");
        eprintln!(
            "next: scroll {at}, row {top}..{bottom}, shows {at}..{}",
            at + height
        );
        assert!(
            top >= at && bottom <= at + height + 0.5,
            "the row is out of sight"
        );

        // 8. A heading's fold: rows go, nothing fights it, and it settles.
        let before = rows(cx).len();
        handle
            .update(cx, |v, _, cx| v.visual_toggle_group("recent:need", cx))
            .unwrap();
        wait(800).await;
        let folded = rows(cx).len();
        assert!(
            folded < before,
            "the fold hid nothing: {before} to {folded}"
        );
        assert!(
            !handle
                .update(cx, |v, _, _| v.visual_motion_moving())
                .unwrap(),
            "the fold never settled"
        );
        shot("rows-folded.png", cx);
        eprintln!("sidebar rows PASS");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
