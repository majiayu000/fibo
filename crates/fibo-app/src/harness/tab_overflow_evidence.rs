use super::*;

// Enough tabs to overflow the strip: the right edge says how many are out of sight,
// and clicking it scrolls them in until the left edge takes over the count.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_TAB_OVERFLOW_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            std::fs::create_dir_all(&directory).unwrap();
            wait(2000).await;
            for _ in 0..16 {
                handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
                wait(300).await;
            }
            wait(1500).await;
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                    w.render_to_image()
                        .unwrap()
                        .save(directory.join(format!("{name}.png")))
                        .unwrap();
                })
                .unwrap();
            };
            let overflow = |cx: &mut gpui::AsyncApp| {
                handle
                    .update(cx, |v, _, _| v.visual_tab_overflow())
                    .unwrap()
            };
            // Opening a tab shows it, so the strip may end anywhere; start from the left.
            handle
                .update(cx, |v, _, cx| v.visual_tab_scroll_home(cx))
                .unwrap();
            wait(300).await;
            shot("tabs-overflow-start", cx);
            wait(300).await;
            let (left, right, at) = overflow(cx);
            eprintln!("tab overflow: start left={left} right={right}");
            assert!(
                left == 0 && right > 0,
                "the strip starts scrolled left with tabs out of sight on the right"
            );
            let mut last = right;
            while last > 0 {
                cx.update_window(handle.into(), |_, w, a| {
                    native_click(w, a, at, gpui::MouseButton::Left)
                })
                .unwrap();
                wait(300).await;
                let (_, right, _) = overflow(cx);
                eprintln!("tab overflow: after click right={right}");
                assert!(
                    right < last,
                    "each click on the marker brings one more tab into view"
                );
                last = right;
            }
            shot("tabs-overflow-end", cx);
            let (left, right, _) = overflow(cx);
            eprintln!("tab overflow: end left={left} right={right}");
            assert!(
                left > 0 && right == 0,
                "scrolled to the end, the count moves to the left edge"
            );
            eprintln!("tab overflow native PASS");
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
