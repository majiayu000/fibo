//! One row lit, wherever the view went: 「选中另一台的对话，侧栏的光标不跟过去」. With
//! peek off a click on another machine's row draws that machine here, with it on the row
//! floats up; either way, and after ⌘J, 回到刚才 and coming back, the sidebar lights the
//! row of what is in front and no other.
use super::*;

pub(super) async fn one_row_lit(
    handle: WindowHandle<workbench::Workbench>,
    cx: &mut gpui::AsyncApp,
) {
    let executor = cx.background_executor().clone();
    let lit =
        |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, cx| v.visual_lit_rows(cx)).unwrap();
    let here = handle
        .update(cx, |v, _, _| v.visual_session_id("here-codex"))
        .unwrap();
    let far = handle
        .update(cx, |v, _, cx| v.visual_remote_row_key(0, "far", cx))
        .unwrap();
    // A machine drawn here the first time is reached first: up to a few seconds.
    let settle = async |want: &dyn Fn(&[String]) -> bool, what: &str, cx: &mut gpui::AsyncApp| {
        let mut now = Vec::new();
        for _ in 0..40 {
            executor.timer(Duration::from_millis(250)).await;
            // Another machine drawn here goes where it was asked to on its next frame.
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh();
                window.draw(app).clear(app);
            })
            .unwrap();
            now = lit(cx);
            if want(&now) {
                return;
            }
        }
        let (hosts, focus) = handle
            .update(cx, |v, _, cx| {
                let hosted = v.visual_hosted().map(|h| h.read(cx).visual_focused());
                (v.visual_hosts(), hosted)
            })
            .unwrap();
        panic!("{what}: lit {now:?}, drawn and reaching {hosts:?}, focused there {focus:?}");
    };
    let only = |key: &str| {
        let key = key.to_owned();
        move |now: &[String]| now == [key.clone()]
    };
    for peek in [true, false] {
        let mode = if peek { "peek on" } else { "peek off" };
        cx.update(|cx| cx.set_global(crate::ui::peek::PeekChoice(peek)));
        handle
            .update(cx, |v, window, cx| {
                v.visual_click_row("here-codex", window, cx)
            })
            .unwrap();
        settle(
            &only(&here),
            &format!("{mode}, this machine's row clicked"),
            cx,
        )
        .await;
        handle
            .update(cx, |v, window, cx| {
                v.visual_click_remote_row(0, "far", window, cx)
            })
            .unwrap();
        settle(
            &only(&far),
            &format!("{mode}, the other machine's row clicked"),
            cx,
        )
        .await;
        // ⌘J goes to this machine's session, the only other one waiting.
        handle
            .update(cx, |v, window, cx| v.visual_next_waiting(window, cx))
            .unwrap();
        settle(
            &only(&here),
            &format!("{mode}, the next-waiting key from the other machine"),
            cx,
        )
        .await;
        handle
            .update(cx, |v, window, cx| v.visual_go_back(window, cx))
            .unwrap();
        settle(
            &only(&far),
            &format!("{mode}, going back to the other machine"),
            cx,
        )
        .await;
        handle
            .update(cx, |v, window, cx| {
                v.visual_close_peek(cx);
                v.visual_show_home(window, cx);
            })
            .unwrap();
        settle(&only(&here), &format!("{mode}, back on this machine"), cx).await;
    }
    cx.update(|cx| cx.set_global(crate::ui::peek::PeekChoice(true)));
    eprintln!("one row lit PASS");
}
