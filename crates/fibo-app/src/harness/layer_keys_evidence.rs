use super::*;

// MOORING_LAYER_KEYS_EVIDENCE: Esc closes the close question and the tab menu without
// reaching the terminal behind them, and Return or ⌃C under the question reaches nothing.
// The control at the end: Esc with nothing open does reach the terminal, so the count
// can see a key.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(_)) = (&handle, evidence)
        && std::env::var_os("MOORING_LAYER_KEYS_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
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
            let focused = |cx: &mut gpui::AsyncApp| {
                handle.update(cx, |v, w, a| v.visual_pane_focused(w, a)).unwrap()
            };
            let layers = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_layers()).unwrap();
            let press = |keys: &str, cx: &mut gpui::AsyncApp| {
                let keystroke = gpui::Keystroke::parse(keys).unwrap();
                cx.update_window(handle.into(), |_, w, a| {
                    w.dispatch_event(
                        gpui::KeyDownEvent { keystroke, is_held: false, prefer_character_input: false }
                            .to_platform_input(),
                        a,
                    );
                })
                .unwrap();
            };
            wait(2000).await;
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            until!(focused(cx), "the tab's shell takes focus");
            wait(1500).await;
            // Two splits: closing the tab now asks first.
            press("cmd-d", cx);
            until!(handle.update(cx, |v, _, _| v.visual_tab_row()).unwrap().2 == 2 && focused(cx), "⌘D splits");
            wait(1500).await;
            let pane = handle
                .update(cx, |v, w, a| v.visual_focused_pane(w, a))
                .unwrap()
                .expect("a focused pane");
            let typed = std::rc::Rc::new(std::cell::Cell::new(0usize));
            let _typed = cx.update(|cx| {
                let typed = typed.clone();
                cx.subscribe(&pane, move |_, event, _| {
                    if matches!(event, crate::terminal::pane::PaneEvent::Typed) {
                        typed.set(typed.get() + 1);
                    }
                })
            });
            handle.update(cx, |v, _, cx| v.visual_close_tab(cx)).unwrap();
            wait(300).await;
            assert!(layers(cx).0, "a tab of two splits asks before closing");
            for keys in ["enter", "ctrl-c", "cmd-w"] {
                press(keys, cx);
                wait(200).await;
                assert_eq!(typed.get(), 0, "{keys} under the close question reaches the terminal");
                assert!(layers(cx).0, "{keys} leaves the close question open");
            }
            assert_eq!(handle.update(cx, |v, _, _| v.visual_tab_row()).unwrap().2, 2, "nothing closed");
            press("escape", cx);
            wait(300).await;
            assert!(!layers(cx).0, "Esc cancels the close question");
            assert_eq!(typed.get(), 0, "Esc under the close question reaches the terminal");
            assert!(focused(cx), "the pane keeps the focus after Esc");
            // The tab menu: the pointer never left the pane, so the terminal keeps the focus.
            handle.update(cx, |v, _, cx| v.visual_open_tab_menu(cx)).unwrap();
            wait(300).await;
            assert!(layers(cx).1, "the tab menu opens");
            press("escape", cx);
            wait(300).await;
            assert!(!layers(cx).1, "Esc closes the tab menu");
            assert_eq!(typed.get(), 0, "Esc over the tab menu reaches the terminal");
            // Control: with nothing open the same Esc is the terminal's.
            press("escape", cx);
            wait(300).await;
            assert_eq!(typed.get(), 1, "Esc with nothing open reaches the terminal");
            eprintln!("layer keys native PASS: close question holds Return ⌃C ⌘W, Esc cancels it and the tab menu without reaching the terminal; Esc with nothing open does");
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
