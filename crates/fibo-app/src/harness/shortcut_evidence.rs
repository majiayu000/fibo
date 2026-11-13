use super::*;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(_)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_SHORTCUT_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            // A launch under load can outlast any fixed wait: poll for the state instead.
            macro_rules! until {
                ($cond:expr, $why:expr) => {{
                    let mut met = false;
                    for _ in 0..100 {
                        if $cond {
                            met = true;
                            break;
                        }
                        wait(100).await;
                    }
                    assert!(met, "{} within 10s", $why);
                }};
            }
            let focused = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, w, a| v.visual_pane_focused(w, a)).unwrap();
            wait(2000).await;
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            until!(focused(cx), "the first tab's shell takes focus");
            // A launch asked for while the first one was starting runs once it ends: count after.
            wait(3000).await;
            let press = |keys: &str, cx: &mut gpui::AsyncApp| {
                let keystroke = gpui::Keystroke::parse(keys).unwrap();
                let focused = handle.update(cx, |v, w, a| v.visual_pane_focused(w, a)).unwrap();
                assert!(focused, "{keys} is pressed while typing in a terminal");
                cx.update_window(handle.into(), |_, w, a| {
                    w.dispatch_event(gpui::KeyDownEvent { keystroke, is_held: false, prefer_character_input: false }.to_platform_input(), a);
                })
                .unwrap();
            };
            let row = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_tab_row()).unwrap();
            let (start, _, _) = row(cx);
            press("cmd-t", cx);
            until!(row(cx).0.len() == start.len() + 1 && focused(cx), "⌘T's tab takes focus");
            let (tabs, shown, panes) = row(cx);
            assert_eq!(tabs.len(), start.len() + 1, "⌘T opens a tab");
            assert_eq!(shown.as_ref(), tabs.last(), "the new tab shows");
            assert_eq!(panes, 1);
            press("cmd-d", cx);
            until!(row(cx).2 == 2 && focused(cx), "⌘D's pane takes focus");
            assert_eq!(row(cx).2, 2, "⌘D splits right");
            press("cmd-shift-d", cx);
            until!(row(cx).2 == 3 && focused(cx), "⌘⇧D's pane takes focus");
            assert_eq!(row(cx).2, 3, "⌘⇧D splits down");
            press("cmd-1", cx);
            wait(300).await;
            let (tabs, shown, _) = row(cx);
            assert_eq!(shown.as_ref(), tabs.first(), "⌘1 shows the first tab");
            press("cmd-shift-[", cx);
            wait(300).await;
            assert_eq!(row(cx).1.as_ref(), tabs.last(), "⌘⇧[ wraps to the last tab");
            press("cmd-shift-]", cx);
            wait(300).await;
            assert_eq!(row(cx).1.as_ref(), tabs.first(), "⌘⇧] wraps to the first tab");
            press("cmd-9", cx);
            wait(300).await;
            assert_eq!(row(cx).1.as_ref(), tabs.last(), "⌘9 shows the last tab");
            // ⌘] / ⌘[ walk the three panes in layout order, wrapping, with real focus.
            let focus = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, w, a| v.visual_pane_focus(w, a)).unwrap();
            let (order, first) = focus(cx);
            assert_eq!(order.len(), 3);
            let at = order.iter().position(|id| Some(id) == first.as_ref()).expect("a pane has focus");
            for step in 1..=3 {
                press("cmd-]", cx);
                wait(300).await;
                assert_eq!(focus(cx).1.as_ref(), Some(&order[(at + step) % 3]), "⌘] step {step}");
            }
            press("cmd-[", cx);
            wait(300).await;
            assert_eq!(focus(cx).1.as_ref(), Some(&order[(at + 2) % 3]), "⌘[ goes back, wrapping");
            press("cmd-[", cx);
            wait(300).await;
            assert_eq!(focus(cx).1.as_ref(), Some(&order[(at + 1) % 3]), "⌘[ again");
            // ⌘⌥ arrows go by where panes sit: order is a | (b over c).
            let (a, b, c) = (&order[0], &order[1], &order[2]);
            for (keys, want, why) in [
                ("cmd-alt-left", a, "⌘⌥← reaches the left column"),
                ("cmd-alt-left", a, "⌘⌥← at the left edge stays"),
                ("cmd-alt-right", b, "⌘⌥→ takes the first of two equal neighbours"),
                ("cmd-alt-down", c, "⌘⌥↓ goes below"),
                ("cmd-alt-up", b, "⌘⌥↑ goes back up"),
                ("cmd-alt-down", c, "⌘⌥↓ again"),
                ("cmd-alt-left", a, "⌘⌥← from the lower right pane"),
            ] {
                press(keys, cx);
                wait(300).await;
                assert_eq!(focus(cx).1.as_ref(), Some(want), "{why}");
            }
            eprintln!("shortcut native PASS: tabs {}->{}, ⌘T ⌘D ⌘⇧D ⌘1 ⌘⇧[ ⌘⇧] ⌘9, ⌘] ⌘[ cycle 3 panes with focus, ⌘⌥ arrows by position", start.len(), tabs.len());
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
