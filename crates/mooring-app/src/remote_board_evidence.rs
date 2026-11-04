//! MOORING_REMOTE_BOARD_EVIDENCE: another machine's sessions on the board (multi-machine
//! step 4, direction A). The outer script starts that machine's host with one Codex
//! session waiting for review in terminal `far`. Checks the card shows up in 要你处理,
//! moves to 做完了 when dropped there, and that 「＋」 opens a shell there which
//! 「关闭终端」 ends; saves the board before and after.
use super::*;

pub(super) fn start(
    handle: WindowHandle<workbench::Workbench>,
    directory: PathBuf,
    cx: &mut App,
) -> bool {
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        std::fs::create_dir_all(&directory).unwrap();
        let shoot = |name: &str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh();
                window.draw(app).clear(app);
                let shot = window.render_to_image().unwrap();
                shot.save(directory.join(name)).unwrap();
            })
            .unwrap();
        };
        let columns = |cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, _, _| v.visual_remote_columns())
                .unwrap()
        };
        let mut seen = vec![];
        for _ in 0..120 {
            wait(250).await;
            seen = columns(cx);
            if seen.iter().any(|(t, c)| t == "far" && *c == "needs") {
                break;
            }
        }
        assert!(
            seen.iter().any(|(t, c)| t == "far" && *c == "needs"),
            "the other machine's session never reached 要你处理: {seen:?}"
        );
        handle
            .update(cx, |v, _, cx| v.visual_show_board(cx))
            .unwrap();
        wait(800).await;
        shoot("remote-board.png", cx);

        handle
            .update(cx, |v, _, cx| v.visual_settle_remote(0, "far", true, cx))
            .unwrap();
        let mut done = false;
        // The card moves at once; the other machine's next list must keep it there.
        for _ in 0..80 {
            wait(250).await;
            done = columns(cx).iter().any(|(t, c)| t == "far" && *c == "done");
        }
        assert!(done, "dropped on 做完了 but the other machine put it back");
        shoot("remote-board-done.png", cx);

        handle
            .update(cx, |v, window, cx| {
                v.visual_open_remote_shell(0, window, cx)
            })
            .unwrap();
        let mut key = None;
        for _ in 0..60 {
            wait(250).await;
            key = handle.update(cx, |v, _, _| v.visual_peek_key()).unwrap();
            if key.is_some() {
                break;
            }
        }
        let key = key.expect("「＋」 opened no peek");
        assert!(key.contains(":remote-"), "{key}");
        wait(1500).await;
        shoot("remote-open.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_end_remote(cx))
            .unwrap();
        let mut closed = false;
        for _ in 0..60 {
            wait(250).await;
            closed = handle
                .update(cx, |v, _, _| v.visual_peek_key())
                .unwrap()
                .is_none();
            if closed {
                break;
            }
        }
        assert!(closed, "「关闭终端」 left the peek open");
        eprintln!(
            "remote board PASS: card in 要你处理, kept in 做完了, shell opened {key} and ended"
        );
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
