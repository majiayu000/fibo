//! MOORING_REMOTE_BOARD_EVIDENCE: another machine's sessions on the board (multi-machine
//! step 4, direction A). The outer script starts that machine's host with one Codex
//! session waiting for review in terminal `far`. Checks the card shows up in 要你处理,
//! moves to 做完了 when dropped there, and that 「＋」 shows its start page, whose shell opens
//! docked in the main area and worked from here; its right-click splits Codex beside it,
//! and 「关闭终端」 ends both.
use super::*;
use fibo_core::layout::Axis;

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

        // 「＋」 shows that machine's start page where the tab's panes go.
        handle
            .update(cx, |v, _, cx| v.visual_remote_start(0, cx))
            .unwrap();
        wait(800).await;
        assert!(
            handle.update(cx, |v, _, _| v.visual_remote_start_shown()).unwrap(),
            "「＋」 showed no start page"
        );
        shoot("remote-start.png", cx);
        handle
            .update(cx, |v, window, cx| v.visual_open_remote_shell(0, window, cx))
            .unwrap();
        let mut panes = vec![];
        for _ in 0..60 {
            wait(250).await;
            panes = handle.update(cx, |v, _, cx| v.visual_dock_panes(cx)).unwrap();
            if !panes.is_empty() {
                break;
            }
        }
        assert!(
            panes.len() == 1 && panes[0].1 && panes[0].0.contains(":remote-"),
            "the start page's shell is not docked and taken: {panes:?}"
        );
        assert!(
            !handle.update(cx, |v, _, _| v.visual_remote_start_shown()).unwrap(),
            "the start page stayed over the opened shell"
        );
        wait(1500).await;
        shoot("remote-open-shell.png", cx);
        // Right-click: split right, then pick Codex; it opens beside the shell, worked too.
        handle
            .update(cx, |v, _, cx| v.visual_dock_menu(None, cx))
            .unwrap();
        wait(400).await;
        shoot("remote-menu.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_dock_menu(Some(Axis::Right), cx))
            .unwrap();
        wait(400).await;
        shoot("remote-menu-right.png", cx);
        handle
            .update(cx, |v, window, cx| {
                v.visual_dock_split(Axis::Right, Some(fibo_core::agent::Agent::Codex), window, cx)
            })
            .unwrap();
        for _ in 0..60 {
            wait(250).await;
            panes = handle.update(cx, |v, _, cx| v.visual_dock_panes(cx)).unwrap();
            if panes.len() == 2 {
                break;
            }
        }
        assert!(
            panes.len() == 2 && panes.iter().all(|p| p.1),
            "the split did not open a second worked pane: {panes:?}"
        );
        let focused = handle.update(cx, |v, _, _| v.visual_peek_key()).unwrap();
        assert_eq!(focused.as_ref(), Some(&panes[1].0), "the new pane is not focused");
        wait(1500).await;
        shoot("remote-split.png", cx);
        // 「关闭终端」 on each ends it there; the last one leaves the main area to the tab.
        for left in [1, 0] {
            handle
                .update(cx, |v, _, cx| v.visual_end_remote(cx))
                .unwrap();
            for _ in 0..60 {
                wait(250).await;
                panes = handle.update(cx, |v, _, cx| v.visual_dock_panes(cx)).unwrap();
                if panes.len() == left {
                    break;
                }
            }
            assert_eq!(panes.len(), left, "「关闭终端」 left {panes:?}");
        }
        eprintln!(
            "remote board PASS: card in 要你处理, kept in 做完了, start page shown, shell docked, split right with Codex, both ended"
        );
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
