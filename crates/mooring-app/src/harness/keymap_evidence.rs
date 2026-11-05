use super::*;

// Two runs over one config directory: 1 rebinds 新建标签 in the settings panel,
// 2 is the restart that must still have it, then restores the defaults.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && let Ok(phase) = std::env::var("MOORING_KEYMAP_EVIDENCE")
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
            let executor = cx.background_executor().clone();
            let wait = move |s: u64| executor.timer(Duration::from_millis(s));
            std::fs::create_dir_all(&directory).unwrap();
            let file = crate::keymap::path().unwrap();
            assert!(
                std::env::var_os("XDG_CONFIG_HOME").is_some(),
                "runs against a scratch config, never ~/.config"
            );
            let ready = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, w, a| v.visual_pane_focused(w, a)).unwrap();
            wait(2000).await;
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            // A launch under load can outlast any fixed wait: poll for the shell's focus.
            for _ in 0..100 {
                if ready(cx) {
                    break;
                }
                wait(100).await;
            }
            let press = |keys: &str, cx: &mut gpui::AsyncApp| {
                let keystroke = gpui::Keystroke::parse(keys).unwrap();
                cx.update_window(handle.into(), |_, w, a| {
                    w.dispatch_event(gpui::KeyDownEvent { keystroke, is_held: false, prefer_character_input: false }.to_platform_input(), a);
                })
                .unwrap();
            };
            let in_pane = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, w, a| v.visual_pane_focused(w, a)).unwrap();
            let tabs = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_tab_row().0.len()).unwrap();
            let settings = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, w, a| v.visual_settings(w, a)).unwrap();
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                let path = directory.join(format!("{name}.png"));
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                    w.render_to_image().unwrap().save(path).unwrap();
                })
                .unwrap();
            };
            // A ⌘ chord typed into the terminal opens a tab, or provably does not.
            let opens_tab = async |keys: &str, cx: &mut gpui::AsyncApp| {
                assert!(in_pane(cx), "{keys} is pressed while typing in a terminal");
                let before = tabs(cx);
                press(keys, cx);
                // Opened means the new tab's shell holds input; a chord that opens
                // nothing is given the same 6s to prove it.
                for _ in 0..60 {
                    wait(100).await;
                    if tabs(cx) == before + 1 && in_pane(cx) {
                        return true;
                    }
                }
                false
            };
            let open_settings = async |cx: &mut gpui::AsyncApp| {
                press("cmd-,", cx);
                wait(400).await;
                let (recording, taken, focused) = settings(cx).expect("⌘, opens 设置");
                assert!(!recording && !taken && focused, "the panel takes the keys");
            };
            let close_settings = async |cx: &mut gpui::AsyncApp| {
                press("escape", cx);
                wait(500).await;
                assert!(settings(cx).is_none(), "Esc closes 设置");
                assert!(in_pane(cx), "input goes back to the terminal");
            };
            if phase == "1" {
                assert!(!file.exists(), "a fresh config starts on the defaults");
                assert!(opens_tab("cmd-t", cx).await, "default ⌘T opens a tab");
                open_settings(cx).await;
                cx.update(|a| crate::ui::appearance::set_mode(crate::ui::appearance::Mode::Dark, a).unwrap());
                wait(300).await;
                shot("settings-dark", cx);
                cx.update(|a| crate::ui::appearance::set_mode(crate::ui::appearance::Mode::Light, a).unwrap());
                wait(300).await;
                shot("settings-light", cx);
                // A taken chord waits for 仍然使用 instead of moving silently.
                handle.update(cx, |v, _, cx| v.visual_settings_record("split_down", cx)).unwrap();
                press("cmd-d", cx);
                wait(300).await;
                assert_eq!(settings(cx), Some((false, true, true)), "⌘D is 向右分屏's");
                shot("settings-conflict-light", cx);
                handle.update(cx, |v, _, cx| v.visual_settings_press("取消", cx)).unwrap();
                // A chord without ⌘ belongs to the program and is refused.
                handle.update(cx, |v, _, cx| v.visual_settings_record("new_tab", cx)).unwrap();
                press("ctrl-t", cx);
                wait(300).await;
                assert!(!file.exists(), "a refused chord changes nothing");
                handle.update(cx, |v, _, cx| v.visual_settings_record("new_tab", cx)).unwrap();
                wait(200).await;
                shot("settings-recording-light", cx);
                // A modifier alone does not end the recording; the full chord does.
                press("cmd-shift", cx);
                assert_eq!(settings(cx).map(|s| s.0), Some(true));
                press("cmd-shift-t", cx);
                wait(300).await;
                let saved = std::fs::read_to_string(&file).expect("keys.json written");
                eprintln!("keys.json after rebind: {}", saved.replace('\n', " "));
                assert!(saved.contains("new_tab") && saved.contains("cmd-shift-t"));
                cx.update(|a| crate::ui::appearance::set_mode(crate::ui::appearance::Mode::Dark, a).unwrap());
                wait(300).await;
                shot("settings-rebound-dark", cx);
                close_settings(cx).await;
                assert!(!opens_tab("cmd-t", cx).await, "the old ⌘T no longer opens a tab");
                assert!(opens_tab("cmd-shift-t", cx).await, "the new ⌘⇧T does");
                eprintln!("keymap native PASS phase 1: default ⌘T, conflict held, ctrl refused, rebound to ⌘⇧T in 设置, old key off");
            } else {
                assert!(file.exists(), "phase 1 left keys.json");
                assert!(opens_tab("cmd-shift-t", cx).await, "after a restart ⌘⇧T still opens a tab");
                assert!(!opens_tab("cmd-t", cx).await, "and ⌘T still does not");
                open_settings(cx).await;
                handle.update(cx, |v, _, cx| v.visual_settings_press("全部恢复默认", cx)).unwrap();
                wait(300).await;
                assert!(!file.exists(), "全部恢复默认 removes keys.json");
                close_settings(cx).await;
                assert!(opens_tab("cmd-t", cx).await, "⌘T is back");
                assert!(!opens_tab("cmd-shift-t", cx).await, "⌘⇧T is free again");
                eprintln!("keymap native PASS phase 2: ⌘⇧T kept across restart, 全部恢复默认 restores ⌘T");
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
