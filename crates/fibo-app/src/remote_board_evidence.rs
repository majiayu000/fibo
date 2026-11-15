//! MOORING_REMOTE_BOARD_EVIDENCE: another machine's sessions on the board (multi-machine
//! step 4, direction A). The outer script starts that machine's host with one Codex
//! session waiting for review in terminal `far`. Checks the card shows up in 要你处理,
//! moves to 做完了 when dropped there, that 「＋」 opens a shell in a new tab there, drawn
//! here, and that 「关闭终端」 in a peek of its terminal ends it; saves the board before and
//! after. Run with peek on (the script turns it on).
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

        // 「＋」: a shell in a new tab there, that machine drawn here.
        let far = handle
            .update(cx, |v, _, _| v.visual_machine_workbench(0))
            .unwrap()
            .expect("the other machine was never reached");
        let before = far.read_with(cx, |w, _| w.visual_tab_ids());
        handle
            .update(cx, |v, window, cx| {
                v.visual_open_remote_shell(0, window, cx)
            })
            .unwrap();
        let mut opened = None;
        for _ in 0..60 {
            wait(250).await;
            let now = far.read_with(cx, |w, _| w.visual_tab_ids());
            opened = now.into_iter().find(|t| !before.contains(t));
            if opened.is_some() {
                break;
            }
        }
        let opened = opened.expect("「＋」 opened no tab there");
        let shown = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
        assert!(shown.0.is_some(), "「＋」 did not draw that machine here");
        wait(1500).await;
        shoot("remote-open.png", cx);
        handle
            .update(cx, |_, _, cx| far.update(cx, |w, cx| w.visual_close_tab_id(&opened, cx)))
            .unwrap();
        // 「关闭终端」 in a peek of its session's terminal ends it there.
        let mut peeked = false;
        for _ in 0..40 {
            wait(250).await;
            peeked = handle
                .update(cx, |v, window, cx| v.visual_peek_remote(0, "far", window, cx))
                .unwrap();
            if peeked {
                break;
            }
        }
        assert!(peeked, "no peek of its terminal");
        wait(1000).await;
        handle
            .update(cx, |v, _, cx| v.visual_end_remote(cx))
            .unwrap();
        let mut ended = false;
        for _ in 0..60 {
            wait(250).await;
            let peek = handle.update(cx, |v, _, _| v.visual_peek_key()).unwrap();
            let open = far.read_with(cx, |w, _| w.visual_tab_ids()).contains(&"t".to_owned());
            ended = peek.is_none() && !open;
            if ended {
                break;
            }
        }
        assert!(ended, "「关闭终端」 left its terminal there");
        eprintln!(
            "remote board PASS: card in 要你处理, kept in 做完了, 「＋」 opened a tab there, 「关闭终端」 ended one"
        );
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
