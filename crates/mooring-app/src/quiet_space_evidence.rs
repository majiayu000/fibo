//! MOORING_QUIET_SPACE_EVIDENCE: 先不管这个空间 in a real window against a real host
//! (docs/quiet-space/spec.md), with real mouse events on the chip and its menu. The outer
//! script starts the host with two Codex sessions in space `s`: one finished for you, one working. Checks the
//! state table: going, what stops counting, looking not ending it, ⌘P, coming back. A second
//! launch (MOORING_QUIET_SPACE_RESTART) checks the host kept it.
use super::pane_drag_evidence::{down, send, up};
use super::*;
use gpui::{InputEvent, MouseButton, WindowHandle, point};
use std::time::Duration;
use workbench::Workbench;

const SPACE: &str = "s";

pub(super) fn start(
    handle: Option<WindowHandle<Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_QUIET_SPACE_EVIDENCE").is_none() {
        return false;
    }
    let restart = std::env::var_os("MOORING_QUIET_SPACE_RESTART").is_some();
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
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
        let quiet = |cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, _, _| v.visual_space_quiet(SPACE))
                .unwrap()
        };
        let wants =
            |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_wants()).unwrap();
        if restart {
            let mut read_back = false;
            for _ in 0..40 {
                wait(250).await;
                read_back = quiet(cx);
                if read_back {
                    break;
                }
            }
            assert!(read_back, "the host forgot the space put aside");
            // This run's host is new, so the terminals ended with the old one: both
            // conversations are still listed to go back to, and none counts.
            let mut rows = 0;
            for _ in 0..40 {
                wait(250).await;
                rows = handle
                    .update(cx, |v, _, _| v.visual_quiet_space_rows(SPACE))
                    .unwrap();
                if rows == 2 {
                    break;
                }
            }
            assert_eq!(rows, 2, "nothing was lost");
            assert_eq!(wants(cx), 0, "still counts none of it after a restart");
            handle
                .update(cx, |v, _, cx| v.visual_unfold_quiet_space(SPACE, true, cx))
                .unwrap();
            wait(200).await;
            shot("8-after-restart.png", cx);
            // A conversation a restart cut off opens from its row, and still counts nothing.
            handle
                .update(cx, |v, w, cx| v.visual_open_quiet_row(SPACE, w, cx))
                .unwrap();
            wait(1500).await;
            assert!(quiet(cx), "going back to one does not take the space back");
            shot("8-reopened.png", cx);
            println!("PASS quiet space restart");
            cx.update(|cx| cx.quit());
            return;
        }
        // The session is there and asks you to check it.
        let mut chip = None;
        for _ in 0..80 {
            wait(250).await;
            let running = handle
                .update(cx, |v, _, _| v.visual_space_running(SPACE))
                .unwrap();
            chip = handle
                .update(cx, |v, _, _| v.visual_space_chip(SPACE))
                .unwrap()
                .filter(|_| wants(cx) == 1 && running == 1);
            if chip.is_some() {
                break;
            }
        }
        let (x, y) = chip.expect("the chip and the session arrive");
        shot("1-before.png", cx);
        assert!(
            handle
                .update(cx, |v, _, _| v.visual_listed_in_column(SPACE))
                .unwrap()
        );
        let cards = handle.update(cx, |v, _, _| v.visual_board_cards()).unwrap();
        assert_eq!(cards, 2, "on the board before");

        // Narrowed to this space when it goes: that must not come back with it.
        handle
            .update(cx, |v, _, cx| v.visual_narrow_to_space(SPACE, cx))
            .unwrap();
        assert_eq!(
            handle.update(cx, |v, _, _| v.visual_stored_space_filter()).unwrap().as_deref(),
            Some(SPACE)
        );
        wait(100).await;
        // Real right press on the chip opens its menu; a press on the entry only asks while
        // something runs there.
        let at = point(px(x + 4.), px(y + 4.));
        let right = gpui::MouseDownEvent {
            button: MouseButton::Right,
            position: at,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input();
        send(handle, cx, right);
        wait(100).await;
        assert!(
            handle
                .update(cx, |v, _, _| v.visual_space_menu_open())
                .unwrap()
        );
        shot("2-menu.png", cx);
        // 重命名, 更换目录, then this entry.
        let entry = handle
            .update(cx, |v, _, _| v.visual_space_menu_point(2))
            .unwrap();
        send(handle, cx, down(entry));
        send(handle, cx, up(entry));
        wait(100).await;
        assert!(!quiet(cx), "the first press only says what keeps running");
        assert!(
            handle
                .update(cx, |v, _, _| v.visual_space_menu_open())
                .unwrap()
        );
        shot("3-confirm.png", cx);
        send(handle, cx, down(entry));
        send(handle, cx, up(entry));
        for _ in 0..40 {
            wait(100).await;
            if quiet(cx) {
                break;
            }
        }
        assert!(
            quiet(cx),
            "the second press puts it aside, through the host"
        );

        // What stops: every count, the column, the board. What does not: what waits.
        assert_eq!(wants(cx), 0, "the window counts none of it");
        assert!(
            !handle
                .update(cx, |v, _, _| v.visual_listed_in_column(SPACE))
                .unwrap()
        );
        assert_eq!(
            handle.update(cx, |v, _, _| v.visual_board_cards()).unwrap(),
            0
        );
        assert_eq!(
            handle
                .update(cx, |v, _, _| v.visual_quiet_space_waiting(SPACE))
                .unwrap(),
            1,
            "its line still says one waits"
        );
        let (looking, strip) = handle.update(cx, |v, _, _| v.visual_looking_at()).unwrap();
        assert_ne!(
            looking, SPACE,
            "leaving the space you are in shows the next"
        );
        assert!(!strip);
        // The working one finishes while put aside: its state moves, nothing calls you.
        let log = PathBuf::from(std::env::var("HOME").unwrap())
            .join(".codex/sessions/busy-codex.jsonl");
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
            writeln!(
                file,
                r#"{{"timestamp":{at},"type":"event_msg","payload":{{"type":"task_complete","last_agent_message":"迁移跑完了。"}}}}"#
            )
            .unwrap();
        }
        let mut waiting = 0;
        for _ in 0..80 {
            wait(250).await;
            waiting = handle
                .update(cx, |v, _, _| v.visual_quiet_space_waiting(SPACE))
                .unwrap();
            if waiting == 2 {
                break;
            }
        }
        assert_eq!(waiting, 2, "the line sees it finish");
        assert_eq!(wants(cx), 0, "and the window still counts none of it");
        wait(1200).await; // The flight lands and the glow fades.
        shot("4-quiet.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_show_board(cx))
            .unwrap();
        wait(200).await;
        shot("5-board.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_leave_board(cx))
            .unwrap();

        // Looking is not taking it back.
        handle
            .update(cx, |v, _, cx| v.visual_unfold_quiet_space(SPACE, true, cx))
            .unwrap();
        wait(200).await;
        shot("6-unfolded.png", cx);
        handle
            .update(cx, |v, w, cx| v.visual_open_quiet_row(SPACE, w, cx))
            .unwrap();
        wait(300).await;
        let (looking, strip) = handle.update(cx, |v, _, _| v.visual_looking_at()).unwrap();
        // A click may open the peek rather than go to the space; neither ends it.
        println!("looking at {looking}, strip {strip}");
        assert!(quiet(cx), "looking does not take it back");
        assert_eq!(wants(cx), 0);
        shot("7-looking.png", cx);
        handle.update(cx, |v, _, cx| v.visual_close_peek(cx)).ok();
        handle
            .update(cx, |v, _, cx| {
                v.visual_pick_space(SPACE, cx);
            })
            .unwrap();
        wait(200).await;
        let (looking, strip) = handle.update(cx, |v, _, _| v.visual_looking_at()).unwrap();
        assert_eq!(looking, SPACE);
        assert!(strip, "the strip says so while you are in it");
        shot("7-strip.png", cx);

        // ⌘P finds the way back, and coming back returns the count without moving anything.
        let title = handle
            .update(cx, |v, _, cx| v.visual_space_hit("工作", cx))
            .unwrap();
        assert_eq!(title.as_deref(), Some("接着管 空间 工作"));
        for _ in 0..40 {
            wait(100).await;
            if !quiet(cx) && wants(cx) == 2 {
                break;
            }
        }
        assert!(!quiet(cx));
        assert_eq!(wants(cx), 2, "what waited, and what finished meanwhile, is counted");
        assert_eq!(
            handle.update(cx, |v, _, _| v.visual_stored_space_filter()).unwrap(),
            None,
            "coming back does not narrow the list again"
        );
        assert_eq!(
            handle.update(cx, |v, _, _| v.visual_board_cards()).unwrap(),
            2
        );
        assert!(
            handle
                .update(cx, |v, _, _| v.visual_listed_in_column(SPACE))
                .unwrap()
        );
        wait(600).await;
        shot("9-back.png", cx);
        // Many put aside share one line, in every layout of the column.
        for id in [SPACE, "a", "b"] {
            handle
                .update(cx, |v, _, cx| v.visual_set_space_quiet(id, true, cx))
                .unwrap();
            wait(600).await;
        }
        wait(1200).await;
        assert_eq!(wants(cx), 0);
        shot("10-many.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_unfold_quiet_spaces(true, cx))
            .unwrap();
        wait(200).await;
        shot("11-many-open.png", cx);
        let layouts = handle
            .update(cx, |v, _, _| v.visual_sidebar_presets())
            .unwrap();
        for id in &layouts {
            handle
                .update(cx, |v, _, cx| v.visual_choose_preset(id, cx))
                .unwrap();
            wait(500).await;
            shot(&format!("12-layout-{id}.png"), cx);
        }
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset(&layouts[0], cx))
            .unwrap();
        for id in ["a", "b"] {
            handle
                .update(cx, |v, _, cx| v.visual_set_space_quiet(id, false, cx))
                .unwrap();
        }
        wait(400).await;
        // Left put aside for the second launch.
        handle
            .update(cx, |v, _, cx| v.visual_set_space_quiet(SPACE, true, cx))
            .unwrap();
        for _ in 0..40 {
            wait(100).await;
            if quiet(cx) {
                break;
            }
        }
        assert!(quiet(cx));
        println!("PASS quiet space");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
