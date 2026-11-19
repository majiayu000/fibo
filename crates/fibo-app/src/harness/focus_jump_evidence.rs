//! MOORING_FOCUS_JUMP_EVIDENCE: where the keyboard lands after a jump, in the sample's split
//! tab (p0 | p1 / p2, all three under one todo). A sidebar row's click and ⌘J, from the same
//! tab, another tab, another space and a zoomed pane: the window's focused pane, the pane
//! holding the keyboard and the sidebar's selected row must all be the target, and stay so
//! a few frames later, and the pane rings when it shares the view (none alone). Run against `scripts/visual-fixture.py prepare`.
use super::cpu_probe_evidence::{cpu_secs, instructions};
use super::*;
use std::time::Instant;
use workbench::Workbench;

pub(super) fn start(
    handle: Option<WindowHandle<Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_FOCUS_JUMP_EVIDENCE").is_none() {
        return false;
    }
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        std::fs::create_dir_all(&directory).unwrap();
        let frames = |cx: &mut gpui::AsyncApp| {
            for _ in 0..3 {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                })
                .unwrap();
            }
        };
        let shot = |name: &str, cx: &mut gpui::AsyncApp| {
            let path = directory.join(name);
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh();
                w.draw(a).clear(a);
                w.render_to_image().unwrap().save(path).unwrap();
            })
            .unwrap();
        };
        for _ in 0..120 {
            wait(250).await;
            let ready = handle
                .update(cx, |v, _, _| v.visual_session_ids().len() >= 3)
                .unwrap();
            if ready {
                break;
            }
        }
        let terminal = |cli: &str, cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, _, _| v.visual_terminal_of(cli))
                .unwrap()
                .unwrap_or_else(|| panic!("{cli} runs in no terminal"))
        };
        let (p0, p1, p2) = (
            terminal("visual-0", cx),
            terminal("visual-1", cx),
            terminal("visual-2", cx),
        );
        let state = |cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, w, cx| v.visual_focus_state(w, cx))
                .unwrap()
        };
        let mut wrong = Vec::new();
        // Lands on `want`, and is still there after a pause and more frames.
        let mut expect = async |case: &str,
                                want: &str,
                                tab: Option<&str>,
                                cx: &mut gpui::AsyncApp| {
            frames(cx);
            let now = state(cx);
            // Among three panes the one landed on rings while it fades.
            let ring = handle.update(cx, |v, _, _| v.visual_landing()).unwrap();
            wait(400).await;
            frames(cx);
            let later = state(cx);
            let lands = |s: &workbench::FocusState| {
                s.focused.as_deref() == Some(want)
                    && s.keyboard.as_deref() == Some(want)
                    && s.row.as_deref() == Some(want)
                    && s.zoomed.as_deref().is_none_or(|z| z == want)
                    && tab.is_none_or(|t| s.tab.as_deref() == Some(t))
            };
            let ok = lands(&now) && lands(&later) && ring.as_deref() == Some(want);
            eprintln!(
                "FOCUS-JUMP {case}: want {want} -> {} ring {ring:?} now {now:?} later {later:?}",
                if ok { "ok" } else { "WRONG" }
            );
            if !ok {
                wrong.push(case.to_owned());
            }
            shot(&format!("{case}.png"), cx);
        };
        let page = |cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, w, cx| v.visual_page("Main", w, cx))
                .unwrap();
        };
        let in_pane = |id: &str, cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, w, cx| v.visual_click_pane(id, w, cx))
                .unwrap();
        };
        let row = |cli: &str, cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, w, cx| v.visual_click_row(cli, w, cx))
                .unwrap();
        };
        page(cx);
        wait(1500).await;
        frames(cx);
        let tab = state(cx).tab.expect("the split tab is shown");
        let tab = Some(tab.as_str());

        // 1. In the split, the keyboard in p0: p2's row, then p1's.
        in_pane(&p0, cx);
        frames(cx);
        fibo_core::effect::set_reduce_motion(false);
        let requests = Workbench::visual_landing_frame_requests;
        let before_jump = requests();
        row("visual-2", cx);
        expect("split-row", &p2, tab, cx).await;
        assert!(requests() > before_jump, "the visible ring never requested a frame");

        // Let the real clock expire. Even a forced redraw must not restart the ring's
        // frame loop, including on a machine with no visible display link.
        wait(1_000).await;
        assert!(handle.update(cx, |v, _, _| v.visual_landing().is_none()).unwrap());
        let visible = handle.update(cx, |_, w, _| w.is_visible()).unwrap();
        let (before, cpu, ins, started) = (requests(), cpu_secs(), instructions(), Instant::now());
        wait(3_000).await;
        let elapsed = started.elapsed().as_secs_f64();
        let (used, retired, idle_requests) = (
            cpu_secs() - cpu,
            instructions().saturating_sub(ins),
            requests() - before,
        );
        frames(cx);
        let forced_requests = requests() - before - idle_requests;
        eprintln!(
            "FOCUS-JUMP expired ring: {idle_requests} idle requests in {elapsed:.2}s; {forced_requests} from forced draws; {:.2}% CPU, {:.2} M instructions/s; window visible {visible}",
            100. * used / elapsed,
            retired as f64 / elapsed / 1e6
        );
        assert_eq!(idle_requests + forced_requests, 0, "the expired ring requested another frame");

        row("visual-1", cx);
        expect("split-row-again", &p1, tab, cx).await;

        // 2. ⌘J from there: wherever it goes, all three agree on it.
        handle
            .update(cx, |v, w, cx| v.visual_next_waiting(w, cx))
            .unwrap();
        frames(cx);
        let went = state(cx).focused.expect("⌘J focused a pane");
        expect("next-waiting", &went, None, cx).await;

        // 3. From another tab of the same space.
        handle
            .update(cx, |v, _, cx| v.visual_show_tab("start", cx))
            .unwrap();
        frames(cx);
        row("visual-1", cx);
        expect("other-tab", &p1, tab, cx).await;

        // 4. From another space.
        handle
            .update(cx, |v, _, cx| v.visual_pick_space("s1", cx))
            .unwrap();
        frames(cx);
        row("visual-0", cx);
        expect("other-space", &p0, tab, cx).await;

        // 5. With p0 zoomed: a session row opens the split again on p2.
        handle
            .update(cx, |v, _, cx| v.visual_zoom(&p0, cx))
            .unwrap();
        frames(cx);
        row("visual-2", cx);
        expect("zoomed-row", &p2, tab, cx).await;

        // 6. With p0 zoomed, a jump straight to a terminal (an unlinked row, another
        // machine's card, 标记完成 from a card): p2, not the zoomed p0.
        handle
            .update(cx, |v, _, cx| v.visual_zoom(&p0, cx))
            .unwrap();
        frames(cx);
        handle
            .update(cx, |v, w, cx| v.visual_focus_terminal(&p2, w, cx))
            .unwrap();
        expect("zoomed-terminal", &p2, tab, cx).await;

        // 7. A pane alone in its tab is the whole main area: it lands without a ring.
        handle
            .update(cx, |v, _, cx| v.visual_show_tab("start", cx))
            .unwrap();
        frames(cx);
        handle
            .update(cx, |v, w, cx| v.visual_focus_terminal("pstart", w, cx))
            .unwrap();
        frames(cx);
        let alone = state(cx);
        let ring = handle.update(cx, |v, _, _| v.visual_landing()).unwrap();
        let ok = alone.keyboard.as_deref() == Some("pstart") && ring.is_none();
        eprintln!(
            "FOCUS-JUMP single-pane: {} ring {ring:?} {alone:?}",
            if ok { "ok" } else { "WRONG" }
        );
        if !ok {
            wrong.push("single-pane".into());
        }
        shot("single-pane.png", cx);

        assert!(wrong.is_empty(), "focus landed elsewhere: {wrong:?}");
        eprintln!("focus jump native PASS: every jump lands on its pane and stays");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
