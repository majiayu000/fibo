use super::*;

// A brand-new space with no tab: 「运行 Claude Code」 opens a terminal with `claude`
// started in it, not a bare shell. A `claude` stand-in on PATH says it ran.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_NEW_SPACE_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            wait(2000).await;
            std::fs::create_dir_all(&directory).unwrap();
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
            handle
                .update(cx, |v, _, cx| v.visual_add_space("fresh", "/tmp", cx))
                .unwrap();
            wait(800).await;
            handle
                .update(cx, |v, _, cx| v.visual_pick_space("fresh", cx))
                .unwrap();
            wait(500).await;
            let (space, tab, _) = handle.update(cx, |v, _, a| v.visual_shown(a)).unwrap();
            assert_eq!(space, "fresh");
            assert!(tab.is_none(), "a new space opens with no tab");
            shot("0-new-space", cx);
            handle
                .update(cx, |v, _, cx| v.visual_run_agent("claude", cx))
                .unwrap();
            let mut text = String::new();
            for _ in 0..60 {
                wait(250).await;
                text = handle.update(cx, |v, _, a| v.visual_shown(a)).unwrap().2;
                if text.contains("FAKE-CLAUDE") {
                    break;
                }
            }
            eprintln!("screen after 运行 Claude Code:\n{text}");
            shot("1-claude-started", cx);
            assert!(
                text.contains("FAKE-CLAUDE") && !text.contains("FAKE-CLAUDE --session-id"),
                "运行 Claude Code started claude in the new terminal"
            );
            // The next new space offers the Agent just used as its one solid button.
            handle
                .update(cx, |v, _, cx| v.visual_add_space("second", "/tmp", cx))
                .unwrap();
            wait(800).await;
            handle
                .update(cx, |v, _, cx| v.visual_pick_space("second", cx))
                .unwrap();
            wait(500).await;
            let first = handle.update(cx, |v, _, _| v.visual_first_agent()).unwrap();
            assert_eq!(first, "claude", "the Agent used last comes first");
            shot("2-second-space", cx);
            eprintln!("new space evidence PASS: 运行 Claude Code starts claude in a new space, and leads the next one");
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
