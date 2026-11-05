//! MOORING_REMOTE_SIDEBAR_EVIDENCE: the three layouts for other machines in the left
//! column (docs/multi-machine/spec.md). The outer script starts this machine's host and
//! another's, each with one Codex session to review, and a third machine that never
//! connects. Checks what each layout lists and that nothing right of the column changes
//! between them, follows the next-waiting key across machines, then stops the other machine and checks its
//! row stays, faded. Saves every step.
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
                {
                    // The window's pixels right of the column, which no layout may touch.
                    let left = 316 * window.scale_factor() as u32;
                    let mut right = Vec::new();
                    for y in 0..shot.height() {
                        for x in left..shot.width() {
                            right.extend_from_slice(&shot.get_pixel(x, y).0);
                        }
                    }
                    right
                }
            })
            .unwrap()
        };
        let column = |cx: &mut gpui::AsyncApp| {
            handle.update(cx, |v, _, _| v.visual_column()).unwrap()
        };
        if std::env::var_os("MOORING_QUIET_RESTART").is_some() {
            // The other machine is stopped by now, so what this can tell is the state read
            // back and its line; the counts were checked while it was up.
            let mut quiet = false;
            for _ in 0..40 {
                wait(250).await;
                quiet = handle.update(cx, |v, _, _| v.visual_machine_quiet(0)).unwrap();
                if quiet && handle.update(cx, |v, _, _| v.visual_wants()).unwrap() == 1 {
                    break;
                }
            }
            assert!(quiet, "先不管 was forgotten across a launch");
            assert_eq!(column(cx).0, 0, "a machine quiet since the last launch is listed");
            shoot("quiet-relaunch.png", cx);
            eprintln!("quiet relaunch PASS");
            cx.update(|cx| cx.quit());
            return;
        }
        let mut listed = 0;
        for _ in 0..160 {
            wait(250).await;
            listed = column(cx).0;
            if listed == 1 {
                break;
            }
        }
        assert_eq!(listed, 1, "the other machine's session never reached the list");
        let offered = handle
            .update(cx, |v, _, _| v.visual_sidebar_presets())
            .unwrap();
        assert!(
            offered.iter().any(|p| p == "machine-rail")
                && offered.iter().any(|p| p == "machine-fold"),
            "{offered:?}"
        );
        wait(800).await;

        // Both machines' waiting counts in the title row, which says the same in every layout.
        let wants = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_wants()).unwrap();
        assert_eq!(wants(cx), 2, "the title row counts this machine's and the other's");

        // A: every machine in one list; a chip shows one alone.
        assert_eq!(column(cx), (1, None, false), "A lists both machines");
        let base = shoot("a-mixed.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_scope_machine(Some(0), cx))
            .unwrap();
        wait(300).await;
        assert_eq!(column(cx), (1, Some(0), true), "A's chip shows one machine");
        let scoped = shoot("a-one-machine.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_scope_machine(None, cx))
            .unwrap();

        // B: the rail picks the machine the whole column shows.
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("machine-rail", cx))
            .unwrap();
        wait(300).await;
        assert_eq!(column(cx), (0, None, false), "B starts on this machine");
        let rail_here = shoot("b-this-machine.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_scope_machine(Some(0), cx))
            .unwrap();
        wait(300).await;
        assert_eq!(column(cx), (1, Some(0), true), "B shows that machine alone");
        let rail_far = shoot("b-other-machine.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_scope_machine(None, cx))
            .unwrap();

        // C: a section per machine, none of them mixed into this machine's list.
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("machine-fold", cx))
            .unwrap();
        wait(300).await;
        assert_eq!(column(cx), (0, None, false), "C keeps its sections apart");
        let sections = shoot("c-sections.png", cx);

        // Only the column changed.
        for (name, shot) in [
            ("A with a chip", &scoped),
            ("B on this machine", &rail_here),
            ("B on the other machine", &rail_far),
            ("C", &sections),
        ] {
            assert!(*shot == base, "{name} changed something right of the column");
        }

        // the next-waiting key goes across machines and the rail follows.
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("machine-rail", cx))
            .unwrap();
        let press = |cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, window, cx| v.visual_next_waiting(window, cx))
                .unwrap()
        };
        // This machine's session is the one in front, so the first goes across.
        press(cx);
        wait(1000).await;
        assert_eq!(column(cx).1, Some(0), "the first the next-waiting key goes to the other machine");
        let peek = handle.update(cx, |v, _, _| v.visual_peek_key()).unwrap();
        assert!(peek.is_some_and(|k| k.ends_with(":far")), "the next-waiting key opened no peek there");
        shoot("b-after-next.png", cx);
        press(cx);
        wait(500).await;
        assert_eq!(column(cx).1, None, "the next the next-waiting key comes back");
        let peek = handle.update(cx, |v, _, _| v.visual_peek_key()).unwrap();
        assert!(peek.is_none(), "coming back left the other machine's peek open");

        // The column's motion, each moment held still and looked at. It moves nothing right
        // of the column, and once it ends the column is what it was and nothing is asked.
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("machine-fold", cx))
            .unwrap();
        wait(800).await;
        let hold = |ms: Option<u64>, cx: &mut gpui::AsyncApp| {
            handle.update(cx, |v, _, _| v.visual_hold_motion(ms)).unwrap();
        };
        let moving = |cx: &mut gpui::AsyncApp| {
            handle.update(cx, |v, _, _| v.visual_motion_moving()).unwrap()
        };
        let ask = |cx: &mut gpui::AsyncApp| {
            handle.update(cx, |v, _, cx| v.visual_motion_frame(cx)).unwrap();
        };
        let local = "machine:local";
        hold(Some(0), cx);
        let rest = shoot("motion-rest.png", cx);
        handle.update(cx, |v, _, cx| v.visual_toggle_group(local, cx)).unwrap();
        for (ms, name) in [(40, "close-40"), (90, "close-90")] {
            hold(Some(ms), cx);
            ask(cx);
            let shot = shoot(&format!("motion-{name}.png"), cx);
            assert!(moving(cx), "{name} should still be moving");
            assert!(shot == rest, "{name} moved something right of the column");
        }
        hold(Some(1000), cx);
        ask(cx);
        let shut = shoot("motion-closed.png", cx);
        assert!(!moving(cx), "closing never ended");
        assert!(shut == rest, "closed moved something right of the column");
        handle.update(cx, |v, _, cx| v.visual_toggle_group(local, cx)).unwrap();
        for (ms, name) in [(1060, "open-60"), (1110, "open-110"), (1170, "open-170")] {
            hold(Some(ms), cx);
            ask(cx);
            let shot = shoot(&format!("motion-{name}.png"), cx);
            assert!(moving(cx), "{name} should still be moving");
            assert!(shot == rest, "{name} moved something right of the column");
        }
        // A section moves as one block: done with its one row, not at the longest stagger.
        hold(Some(1200), cx);
        ask(cx);
        shoot("motion-open-200.png", cx);
        assert!(!moving(cx), "the section kept asking for frames after it had opened");
        hold(Some(3000), cx);
        ask(cx);
        shoot("motion-opened.png", cx);
        assert!(!moving(cx), "opening never ended");
        // Compared with the same view drawn before any of it, the whole window included.
        let whole = |name: &str| {
            let read = |n: &str| std::fs::read(directory.join(n)).unwrap();
            read(name) == read("motion-rest.png")
        };
        assert!(whole("motion-opened.png"), "the column did not come back to rest as it was");

        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("machine-rail", cx))
            .unwrap();
        hold(Some(10_000), cx);
        ask(cx);
        let rail = shoot("motion-rail-rest.png", cx);
        handle.update(cx, |v, _, cx| v.visual_scope_machine(Some(0), cx)).unwrap();
        for (ms, name) in [(10_060, "rail-60"), (10_130, "rail-130"), (10_210, "rail-210")] {
            hold(Some(ms), cx);
            ask(cx);
            let shot = shoot(&format!("motion-{name}.png"), cx);
            assert!(shot == rail, "{name} moved something right of the column");
        }
        hold(Some(11_000), cx);
        ask(cx);
        shoot("motion-rail-end.png", cx);
        assert!(!moving(cx), "the rail's motion never ended");
        // The machine's spaces narrow the list: the empty one to nothing, its own to its row.
        let ids = handle.update(cx, |v, _, _| v.visual_machine_space_ids(0)).unwrap();
        assert_eq!(ids.len(), 2, "the other machine has two spaces: {ids:?}");
        let empty = ids.iter().find(|i| *i == "empty").unwrap().clone();
        let own = ids.iter().find(|i| *i != "empty").unwrap().clone();
        let settle = |at, cx: &mut gpui::AsyncApp| {
            hold(Some(at), cx);
            ask(cx);
        };
        handle.update(cx, |v, _, cx| v.visual_machine_space(&empty, cx)).unwrap();
        settle(13_000, cx);
        shoot("space-empty.png", cx);
        assert_eq!(column(cx).0, 0, "the empty space still lists the row");
        handle.update(cx, |v, _, cx| v.visual_machine_space(&own, cx)).unwrap();
        settle(14_000, cx);
        let on_own = shoot("space-own.png", cx);
        assert_eq!(column(cx).0, 1, "its own space lost the row");
        assert!(on_own == rail, "a machine's space moved something right of the column");
        handle.update(cx, |v, _, cx| v.visual_machine_space(&own, cx)).unwrap();
        settle(15_000, cx);
        assert_eq!(column(cx).0, 1, "all spaces again");
        // Going to another machine forgets it.
        handle.update(cx, |v, _, cx| v.visual_machine_space(&empty, cx)).unwrap();
        handle.update(cx, |v, _, cx| v.visual_scope_machine(None, cx)).unwrap();
        settle(16_000, cx);
        handle.update(cx, |v, _, cx| v.visual_scope_machine(Some(0), cx)).unwrap();
        settle(17_000, cx);
        assert_eq!(column(cx).0, 1, "the space stayed picked after leaving the machine");
        handle.update(cx, |v, _, cx| v.visual_scope_machine(None, cx)).unwrap();
        settle(18_000, cx);
        hold(None, cx);

        // The rail's foot opens the page where computers are added.
        handle.update(cx, |v, w, cx| v.visual_rail_add(w, cx)).unwrap();
        let page = handle.update(cx, |v, _, cx| v.visual_settings_page_name(cx)).unwrap();
        assert_eq!(page, "手机与远程", "the rail's + opened another page");
        handle.update(cx, |v, _, cx| v.visual_settings_close(cx)).unwrap();
        wait(200).await;


        // 先不管这台: out of every count, the key and the board, in every layout; looking at
        // its sessions does not end it, and the words and the file follow both ways.
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("recent", cx))
            .unwrap();
        wait(300).await;
        let file = std::path::PathBuf::from(std::env::var("HOME").unwrap())
            .join(".config/mooring/machines.json");
        let saved = || std::fs::read_to_string(&file).unwrap().contains("\"quiet\": true");
        assert!(!saved(), "a machine started out quiet");
        assert_eq!(wants(cx), 2, "both machines before 先不管");
        handle
            .update(cx, |v, _, cx| v.visual_scope_machine(Some(0), cx))
            .unwrap();
        wait(300).await;
        handle
            .update(cx, |v, _, cx| v.visual_machine_menu(0, true, cx))
            .unwrap();
        wait(200).await;
        shoot("quiet-menu.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_set_machine_quiet(0, true, cx))
            .unwrap();
        wait(300).await;
        assert!(saved(), "先不管 was not saved");
        assert_eq!(column(cx), (0, None, false), "先不管 left the column on that machine");
        assert_eq!(wants(cx), 1, "a quiet machine counts in the title row");
        assert_eq!(
            handle.update(cx, |v, _, _| v.visual_quiet_waiting(0)).unwrap(),
            1,
            "its line lost count of what waits"
        );
        assert!(
            handle.update(cx, |v, _, _| v.visual_remote_columns()).unwrap().iter().all(|(id, _)| id != "far"),
            "a quiet machine's card stayed on the board"
        );
        shoot("quiet-a.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_scope_machine(Some(0), cx))
            .unwrap();
        assert_eq!(column(cx).1, None, "a quiet machine can be shown alone");
        handle
            .update(cx, |v, _, cx| v.visual_scope_machine(None, cx))
            .unwrap();
        for (preset, shot) in [("machine-rail", "quiet-b.png"), ("machine-fold", "quiet-c.png")] {
            handle
                .update(cx, |v, _, cx| v.visual_choose_preset(preset, cx))
                .unwrap();
            wait(300).await;
            assert_eq!(column(cx).0, 0, "{preset} lists a quiet machine");
            assert_eq!(wants(cx), 1, "{preset} counts a quiet machine");
            shoot(shot, cx);
        }
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("recent", cx))
            .unwrap();
        handle
            .update(cx, |v, _, cx| v.visual_quiet_unfold(0, true, cx))
            .unwrap();
        wait(300).await;
        shoot("quiet-open.png", cx);
        let opened = handle
            .update(cx, |v, w, cx| v.visual_peek_remote(0, "far", w, cx))
            .unwrap();
        assert!(opened, "a quiet machine's session did not open");
        wait(500).await;
        shoot("quiet-peek.png", cx);
        assert_eq!(wants(cx), 1, "looking at a quiet machine ended 先不管");
        assert!(saved(), "looking at a quiet machine ended 先不管");
        handle.update(cx, |v, _, cx| v.visual_close_peek(cx)).unwrap();
        handle.update(cx, |v, _, cx| v.visual_show_board(cx)).unwrap();
        wait(400).await;
        shoot("quiet-board.png", cx);
        handle.update(cx, |v, _, cx| v.visual_leave_board(cx)).unwrap();
        wait(200).await;
        // Back by ⌘P's words, then away by them.
        let back = handle
            .update(cx, |v, _, cx| v.visual_machine_hit("接着管", cx))
            .unwrap();
        wait(300).await;
        assert_eq!(back.as_deref(), Some("接着管 far"));
        assert!(!saved(), "接着管 was not saved");
        assert_eq!(wants(cx), 2, "接着管 did not bring its count back");
        assert_eq!(column(cx).0, 1, "接着管 did not bring its row back");
        shoot("resumed-a.png", cx);
        let away = handle
            .update(cx, |v, _, cx| v.visual_machine_hit("先不管", cx))
            .unwrap();
        assert_eq!(away.as_deref(), Some("先不管 far"));
        wait(300).await;
        // ⌘J stays on this machine with nothing else waiting there.
        handle.update(cx, |v, w, cx| v.visual_next_waiting(w, cx)).unwrap();
        wait(300).await;
        let peek = handle.update(cx, |v, _, _| v.visual_peek_key()).unwrap();
        assert!(
            !peek.is_some_and(|k| k.starts_with("remote:")),
            "the next-waiting key went to a quiet machine"
        );

        // The other machine drops off: its row stays, faded, and it counts as down.
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("recent", cx))
            .unwrap();
        let far = std::env::var("MOORING_FAR_PID").expect("MOORING_FAR_PID");
        std::process::Command::new("kill").arg(&far).status().unwrap();
        let mut down = false;
        for _ in 0..80 {
            wait(250).await;
            down = handle
                .update(cx, |v, _, _| v.visual_machine_online(0))
                .unwrap()
                == Some(false);
            if down {
                break;
            }
        }
        assert!(down, "the stopped machine still reads as connected");
        // Quiet through the drop: nothing moves, and it still counts what it listed last.
        assert_eq!(wants(cx), 1, "a drop moved a quiet machine's count");
        assert_eq!(
            handle.update(cx, |v, _, _| v.visual_quiet_waiting(0)).unwrap(),
            1,
            "a drop changed a quiet machine's line"
        );
        wait(300).await;
        shoot("quiet-dropped.png", cx);
        // Back while it is down: it reads as any machine that dropped off.
        handle
            .update(cx, |v, _, cx| v.visual_set_machine_quiet(0, false, cx))
            .unwrap();
        wait(300).await;
        assert!(!saved(), "接着管 while down was not saved");
        assert_eq!(wants(cx), 1, "a machine that dropped off still counts in the title row");
        assert_eq!(column(cx).0, 1, "its last row left the list");
        shoot("a-dropped.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("machine-fold", cx))
            .unwrap();
        wait(300).await;
        shoot("c-dropped.png", cx);
        // Left quiet for the next launch, which the outer script makes.
        handle
            .update(cx, |v, _, cx| v.visual_set_machine_quiet(0, true, cx))
            .unwrap();
        wait(200).await;
        assert!(saved(), "the last 先不管 was not saved");
        eprintln!("remote sidebar PASS: A, B, C listed as they should, right of the column unchanged, the next-waiting key crossed machines, dropped machine kept and faded");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
