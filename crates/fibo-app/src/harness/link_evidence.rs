use super::*;

// Plain-text URLs in Agent prose: hovering one underlines it, as an OSC 8 link does.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_LINK_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            cx.background_executor().timer(Duration::from_secs(2)).await;
            std::fs::create_dir_all(&directory).unwrap();
            let pane = handle.update(cx, |view, _, _| view.visual_shown_pane()).unwrap();
            pane.update(cx, |pane, cx| {
                pane.visual_type(
                    "clear; printf '7. Hunter / AI 雷达\\n- 加了 Rust 标签页，已经上线到 3002 端口：http://100.113.31.23:3002/radar?s=rust\\n- 见 (https://en.wikipedia.org/wiki/Rust_(language))。\\n'\r",
                    cx,
                )
            });
            for _ in 0..30 {
                cx.background_executor().timer(Duration::from_millis(500)).await;
                if pane.update(cx, |p, _| p.visual_text()).contains("3002 端口：http") {
                    break;
                }
            }
            for (name, prefix, cols) in [
                ("hover-radar.png", "http://100.113.31.23", 38),
                ("hover-wiki.png", "https://en.wikipedia.org", 45),
            ] {
                let (at, found) = pane
                    .update(cx, |pane, _| pane.visual_link(prefix))
                    .unwrap_or_else(|| panic!("no link for {prefix} in\n{}", pane.update(cx, |p, _| p.visual_text())));
                assert_eq!(found, cols, "{prefix}");
                cx.update_window(handle.into(), |_, window, app| {
                    // Underlined only while ⌘ is held, when a click would open it.
                    window.dispatch_event(
                        gpui::MouseMoveEvent { position: at, pressed_button: None, modifiers: gpui::Modifiers { platform: true, ..Default::default() } }
                            .to_platform_input(),
                        app,
                    );
                    window.refresh(); window.draw(app).clear(app);
                    window.render_to_image().unwrap().save(directory.join(name)).unwrap();
                }).unwrap();
            }
            // A path longer than the pane is wide wraps onto the next row; a script beside it.
            let deep = directory.join(format!("{}/{}", "a".repeat(120), "b".repeat(120)));
            std::fs::create_dir_all(&deep).unwrap();
            let page = deep.join("index.html");
            std::fs::write(&page, "<p>hi</p>").unwrap();
            let script = directory.join("run.sh");
            std::fs::write(&script, "echo hi").unwrap();
            pane.update(cx, |pane, cx| {
                pane.visual_type(
                    &format!(
                        "clear; printf '8. 其他\\n- 把 3 个 Codex 的 skill 软链接到了 ~/.claude/skills。\\n- 改了 crates/fibo-app/Cargo.toml:3 和 full_v18.mp4\\n- 页面 {} 和脚本 {}\\n'\r",
                        page.display(),
                        script.display()
                    ),
                    cx,
                )
            });
            for _ in 0..30 {
                cx.background_executor().timer(Duration::from_millis(500)).await;
                if pane.update(cx, |p, _| p.visual_text()).contains("和 full_v18") {
                    break;
                }
            }
            let page_word = page.display().to_string();
            let script_word = script.display().to_string();
            let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
            for (name, word, expect) in [
                ("hover-home.png", "~/.claude/skills", Some(home.join(".claude/skills"))),
                (
                    "hover-relative.png",
                    "crates/fibo-app/Cargo.toml:3",
                    Some(std::env::current_dir().unwrap().join("crates/fibo-app/Cargo.toml")),
                ),
                ("hover-missing.png", "full_v18.mp4", None),
                ("hover-wrapped.png", page_word.as_str(), Some(page.clone())),
                ("hover-script.png", script_word.as_str(), Some(script.clone())),
            ] {
                let (at, _) = pane
                    .update(cx, |pane, _| pane.visual_path(word))
                    .unwrap_or_else(|| panic!("no path word {word}"));
                let hover = |modifiers: gpui::Modifiers, cx: &mut gpui::AsyncApp| {
                    cx.update_window(handle.into(), |_, window, app| {
                        window.dispatch_event(
                            gpui::MouseMoveEvent { position: at, pressed_button: None, modifiers }
                                .to_platform_input(),
                            app,
                        );
                    }).unwrap();
                };
                hover(Default::default(), cx);
                // A relative word waits for the host to say where the shell is.
                cx.background_executor().timer(Duration::from_millis(500)).await;
                let (_, hovered) = pane.update(cx, |pane, _| pane.visual_path(word)).unwrap();
                assert_eq!(hovered, expect, "{word}");
                assert_eq!(pane.update(cx, |p, _| p.visual_hint()), None, "{word}: a hint without ⌘");
                hover(gpui::Modifiers { platform: true, ..Default::default() }, cx);
                let hint = pane.update(cx, |p, _| p.visual_hint());
                let wants = match name {
                    "hover-relative.png" => Some("第 3 行"),
                    "hover-wrapped.png" => Some("⌘ 点击打开"),
                    "hover-script.png" => Some("不运行"),
                    "hover-missing.png" => None,
                    _ => Some("⌘ 点击在访达中打开"),
                };
                match wants {
                    Some(text) => assert!(hint.as_deref().is_some_and(|h| h.contains(text)), "{word}: hint {hint:?}"),
                    None => assert_eq!(hint, None, "{word}"),
                }
                eprintln!("{name}: hint {hint:?}");
                cx.update_window(handle.into(), |_, window, app| {
                    window.refresh(); window.draw(app).clear(app);
                    window.render_to_image().unwrap().save(directory.join(name)).unwrap();
                }).unwrap();
            }
            eprintln!("link native PASS: with ⌘ held, plain-text URLs and existing paths are underlined with a note on what a click does, a wrapped path is one link, a script is only shown; a missing file is not");
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
