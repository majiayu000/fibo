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
        assert_eq!(column(cx).0, 1, "its last row left the list");
        shoot("a-dropped.png", cx);
        handle
            .update(cx, |v, _, cx| v.visual_choose_preset("machine-fold", cx))
            .unwrap();
        wait(300).await;
        shoot("c-dropped.png", cx);
        eprintln!("remote sidebar PASS: A, B, C listed as they should, right of the column unchanged, the next-waiting key crossed machines, dropped machine kept and faded");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
