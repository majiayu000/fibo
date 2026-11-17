//! MOORING_FOCUS_JUMP_EVIDENCE: where the keyboard lands after a jump, in the sample's split
//! tab (p0 | p1 / p2, all three under one todo). A sidebar row's click and ⌘J, from the same
//! tab, another tab, another space and a zoomed pane: the window's focused pane, the pane
//! holding the keyboard and the sidebar's selected row must all be the target, and stay so
//! a few frames later. Run against `scripts/visual-fixture.py prepare`.
use super::*;
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
        let mut expect =
            async |case: &str, want: &str, tab: Option<&str>, cx: &mut gpui::AsyncApp| {
                frames(cx);
                let now = state(cx);
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
                let ok = lands(&now) && lands(&later);
                eprintln!(
                    "FOCUS-JUMP {case}: want {want} -> {} now {now:?} later {later:?}",
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
        row("visual-2", cx);
        expect("split-row", &p2, tab, cx).await;
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

        assert!(wrong.is_empty(), "focus landed elsewhere: {wrong:?}");
        eprintln!("focus jump native PASS: every jump lands on its pane and stays");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
