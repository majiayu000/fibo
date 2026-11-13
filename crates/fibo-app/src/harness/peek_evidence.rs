use super::*;

// Peek: a session waiting at an approval in a hidden tab floats over the shown one,
// is answered from the peek's title bar, and its terminal keeps its own size until
// the peek is typed in.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_PEEK_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |ms| executor.timer(Duration::from_millis(ms));
            // Peek is off until chosen in settings; this run turns it on, then off again.
            cx.update(|cx| cx.set_global(crate::ui::peek::PeekChoice(true)));
            wait(1500).await;
            std::fs::create_dir_all(&directory).unwrap();
            let socket = handle.update(cx, |v, _, _| v.visual_socket()).unwrap();
            let root = socket.parent().unwrap().to_owned();
            let call = |r: Request| rpc(&socket, &r).unwrap();
            let workspace = || match call(Request::Workspace) { Reply::Workspace(w) => w, x => panic!("{x:?}") };
            let w = workspace();
            let space = match w.spaces.first() {
                Some(s) => s.id.clone(),
                None => {
                    call(Request::Edit { request: None, edit: Edit::AddSpace(Space { id: "s".into(), name: "Tests".into(), cwd: "/tmp".into() }) });
                    "s".into()
                }
            };
            // A Claude stand-in: its transcript names the session, then it asks.
            let logs = PathBuf::from(std::env::var_os("CLAUDE_CONFIG_DIR").expect("scratch CLAUDE_CONFIG_DIR")).join("projects/-tmp");
            std::fs::create_dir_all(&logs).unwrap();
            let claude = root.join("claude");
            let _ = std::os::unix::fs::symlink("/bin/sh", &claude);
            let id = format!("{:08x}-0000-4000-8000-000000000001", std::process::id());
            let seed = root.join("seed");
            let row = serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/tmp","entrypoint":"cli","userType":"external","timestamp":fibo_core::content::now(),"message":{"content":"发布前检查"}});
            std::fs::write(&seed, format!("{row}\n")).unwrap();
            let script = format!(
                "cat '{}' > '{}'; printf '%s\\n' '⏺ 测试都过了，准备推送到远端。' '' '⏺ Bash(git push origin feat/release)' '' 'Bash command' '  git push origin feat/release' 'Do you want to proceed?' '❯ 1. Yes' \"  2. Yes, and don't ask again for git push\" '  3. No, and tell Claude what to do differently (esc)'; read -r answer; printf '\\033[2J\\033[Hanswer=[%s] size=%s\\n' \"$answer\" \"$(stty size)\"; read -r end; echo \"end=[$end]\"; read -r typed; echo \"typed=[$typed] size=$(stty size)\"; sleep 600",
                seed.display(), logs.join(format!("{id}.jsonl")).display()
            );
            let launch = |tid: &str, argv: Vec<String>| {
                call(Request::Launch { colors: None, id: tid.into(), space: space.clone(), argv, cwd: "/tmp".into(), cols: 100, rows: 30, scheme: Default::default() });
                call(Request::Edit { request: None, edit: Edit::CreateTab { tab: Tab { id: format!("tab-{tid}"), space: space.clone(), default_task: None, title: tid.into(), layout: Some(Layout::Leaf(tid.into())) } } });
            };
            launch("agent", vec!["/bin/sh".into(), "-c".into(), format!("exec '{}' -c \"$0\" fixture --session-id {id}", claude.display()), script]);
            launch("shell", vec!["/bin/sh".into(), "-c".into(), "printf '~/mooring $ cargo test\\n'; sleep 600".into()]);
            wait(500).await;
            handle.update(cx, |v, _, a| v.visual_peek_show_tab("shell", a)).unwrap();
            // The session binds and the screen rule sees the approval within a few seconds.
            let mut seen = None;
            for _ in 0..120 {
                wait(500).await;
                let opened = handle.update(cx, |v, w, a| v.visual_peek_row("agent", w, a)).unwrap();
                seen = handle.update(cx, |v, _, _| v.visual_peek_state()).unwrap();
                if opened && seen.as_ref().is_some_and(|(_, a, _)| a.is_some()) { break; }
            }
            let (terminal, approval, _) = seen.expect("peek never opened");
            assert_eq!(terminal, "agent");
            assert_eq!(approval.as_deref(), Some("Do you want to proceed?"), "approval not offered");
            assert!(handle.update(cx, |v, w, a| v.visual_peek_focused(w, a)).unwrap(), "peek not focused");
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh(); w.draw(a).clear(a);
                    w.render_to_image().unwrap().save(directory.join(name)).unwrap();
                }).unwrap();
            };
            wait(300).await;
            shot("1-peek-light.png", cx);
            cx.update(|a| crate::ui::appearance::set_mode(crate::ui::appearance::Mode::Dark, a).unwrap());
            wait(300).await;
            shot("1-peek-dark.png", cx);
            cx.update(|a| crate::ui::appearance::set_mode(crate::ui::appearance::Mode::Light, a).unwrap());
            let screen = || match call(Request::ReadScreen { id: "agent".into(), rows: 30 }) { Reply::Text(t) => t, x => panic!("{x:?}") };
            handle.update(cx, |v, _, a| v.visual_peek_answer(true, a)).unwrap();
            let mut answered = false;
            for _ in 0..50 {
                wait(100).await;
                if screen().contains("answer=[]") { answered = true; break; }
            }
            assert!(answered, "允许一次 never reached the Agent: {}", screen());
            // The peek never resized the terminal it looked at.
            assert!(screen().contains("size=30 100"), "peek resized the terminal: {}", screen());
            wait(500).await;
            shot("2-answered.png", cx);
            // The same answer again finds no approval and types nothing.
            let w = workspace();
            let stale = rpc(&socket, &Request::AnswerApproval { id: "agent".into(), question: "Do you want to proceed?".into(), allow: true });
            assert!(stale.is_err(), "answered twice: {stale:?} rev {}", w.revision);
            // Esc closes the peek and is not typed into the Agent.
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), a);
            }).unwrap();
            wait(300).await;
            assert!(handle.update(cx, |v, _, _| v.visual_peek_state()).unwrap().is_none(), "Esc did not close the peek");
            call(Request::SendText { session: None, id: "agent".into(), text: "done".into(), submit: true });
            let mut ended = String::new();
            for _ in 0..50 {
                wait(100).await;
                ended = screen();
                if ended.contains("end=[") { break; }
            }
            assert!(ended.contains("end=[done]"), "Esc reached the Agent: {ended}");
            // Opening the peeked terminal's own tab ends the peek, so the tab's pane owns it.
            handle.update(cx, |v, w, a| v.visual_peek_row("agent", w, a)).unwrap();
            assert!(handle.update(cx, |v, _, _| v.visual_peek_state()).unwrap().is_some());
            // Typing in the peek makes it the terminal's user: the terminal takes its size.
            wait(300).await;
            cx.update_window(handle.into(), |_, w, a| {
                for key in ["x", "enter"] {
                    w.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), a);
                }
            }).unwrap();
            let mut typed = String::new();
            for _ in 0..50 {
                wait(100).await;
                // The taller terminal's text may sit below the first 30 rows.
                typed = match call(Request::ReadScreen { id: "agent".into(), rows: 200 }) { Reply::Text(t) => t, x => panic!("{x:?}") };
                if typed.contains("typed=[") { break; }
            }
            assert!(typed.contains("typed=[x]"), "typing never reached the peeked Agent: {typed}");
            assert!(!typed.contains("typed=[x] size=30 100"), "a used peek kept the old size: {typed}");
            wait(300).await;
            shot("3-peek-typed.png", cx);
            handle.update(cx, |v, _, a| v.visual_peek_show_tab("agent", a)).unwrap();
            assert!(handle.update(cx, |v, _, _| v.visual_peek_state()).unwrap().is_none(), "peek kept over its own tab");
            wait(800).await;
            shot("4-own-tab.png", cx);
            // Turned off, the same sidebar click switches to the session instead.
            cx.update(|cx| cx.set_global(crate::ui::peek::PeekChoice(false)));
            handle.update(cx, |v, _, a| v.visual_peek_show_tab("shell", a)).unwrap();
            wait(300).await;
            handle.update(cx, |v, w, a| v.visual_peek_row("agent", w, a)).unwrap();
            assert!(handle.update(cx, |v, _, _| v.visual_peek_state()).unwrap().is_none(), "peek opened while turned off");
            wait(800).await;
            shot("5-peek-off.png", cx);
            eprintln!("peek native PASS: hidden approval peeked, answered once, size kept until typed in, Esc closes without reaching the Agent, off opens the session instead");
            // 置顶: right-clicking the row opens its menu, 置顶 there pins the terminal, the
            // pin is saved, and it comes off once the terminal ends. The row is the only
            // session, under 会话 and its 在跑 heading.
            let row = gpui::point(px(100.), px(197.));
            cx.update_window(handle.into(), |_, w, a| { w.refresh(); w.draw(a).clear(a); native_click(w, a, row, gpui::MouseButton::Right) }).unwrap();
            wait(300).await;
            shot("4-session-menu.png", cx);
            let item = handle.update(cx, |v, _, _| v.visual_session_menu_point("置顶")).unwrap();
            cx.update_window(handle.into(), |_, w, a| native_click(w, a, item, gpui::MouseButton::Left)).unwrap();
            wait(300).await;
            assert_eq!(handle.update(cx, |v, _, _| v.visual_pins()).unwrap(), vec!["agent".to_owned()], "the menu did not pin");
            assert!(handle.update(cx, |v, _, _| v.visual_peek_state()).unwrap().is_none(), "the menu's click also reached the row");
            let saved = std::fs::read_to_string(crate::ui::status_colors::config_dir().unwrap().join("sidebar")).unwrap();
            assert!(saved.lines().any(|l| l == "pin agent"), "pin not saved: {saved:?}");
            shot("5-pinned.png", cx);
            // 放弃 sits last in the same menu, and gives the session up.
            let w = workspace();
            let given = w.agent_sessions.iter().find(|s| s.terminal.as_deref() == Some("agent")).expect("agent's session").id.clone();
            handle.update(cx, |v, _, a| v.visual_open_session_menu(&given, row, a)).unwrap();
            wait(300).await;
            shot("5-session-menu-endings.png", cx);
            let item = handle.update(cx, |v, _, _| v.visual_session_menu_point("放弃")).unwrap();
            cx.update_window(handle.into(), |_, w, a| native_click(w, a, item, gpui::MouseButton::Left)).unwrap();
            wait(300).await;
            let w = workspace();
            let s = w.agent_sessions.iter().find(|s| s.id == given).unwrap();
            assert_eq!(s.outcome, Outcome::Abandoned, "the menu's 放弃 did not give the session up");
            shot("5-abandoned.png", cx);
            eprintln!("abandon native PASS: the row menu's 放弃 gives the session up");
            call(Request::Stop { id: "agent".into() });
            let mut gone = false;
            for _ in 0..50 {
                wait(200).await;
                if handle.update(cx, |v, _, _| v.visual_pins()).unwrap().is_empty() { gone = true; break; }
            }
            assert!(gone, "pin outlived its terminal");
            let saved = std::fs::read_to_string(crate::ui::status_colors::config_dir().unwrap().join("sidebar")).unwrap();
            assert!(!saved.contains("pin "), "ended pin still saved: {saved:?}");
            shot("6-unpinned.png", cx);
            eprintln!("pin native PASS: row menu pins and saves, ended terminal unpins");
            // 在编辑器里看滚动内容: the pane menu opens the terminal's text with $EDITOR in a
            // new tab; the stand-in editor (MOORING_TEST_EDITOR) copies what it was given.
            if let Some(edited) = std::env::var_os("MOORING_EDITED") {
                let edited = PathBuf::from(edited);
                handle.update(cx, |v, _, a| v.visual_peek_show_tab("shell", a)).unwrap();
                wait(500).await;
                let at = gpui::point(px(theme::Theme::SIDEBAR + 200.), px(theme::Theme::TOP + theme::Theme::TABS + theme::Theme::PANE_HEADER + 120.));
                cx.update_window(handle.into(), |_, w, a| { w.refresh(); w.draw(a).clear(a); native_click(w, a, at, gpui::MouseButton::Right) }).unwrap();
                wait(300).await;
                shot("7-menu.png", cx);
                let item = handle.update(cx, |v, _, _| v.visual_menu_point("在编辑器里看滚动内容")).unwrap();
                cx.update_window(handle.into(), |_, w, a| native_click(w, a, item, gpui::MouseButton::Left)).unwrap();
                let mut text = String::new();
                for _ in 0..100 {
                    wait(200).await;
                    if let Ok(t) = std::fs::read_to_string(&edited) { text = t; break; }
                }
                if text.is_empty() { shot("8-editor-failed.png", cx); }
                assert!(text.contains("~/mooring $ cargo test"), "editor got: {text:?}");
                let saved = root.join("scrollback/shell.txt");
                let mode = std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&saved).unwrap().permissions()) & 0o777;
                assert_eq!(mode, 0o600, "scrollback file readable by others");
                wait(500).await;
                shot("8-editor.png", cx);
                eprintln!("scrollback native PASS: menu opens the terminal text with $EDITOR, file private");
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
