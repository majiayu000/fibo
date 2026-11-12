use super::*;

// 「＋ 空间」 starts a space without asking for a folder: its name is in a field at
// once, it is not marked 目录不在了, and its first terminal starts in $HOME.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_FOLDERLESS_SPACE_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            use gpui::InputEvent;
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
            let state = |cx: &mut gpui::AsyncApp| {
                handle
                    .update(cx, |v, w, a| v.visual_space_state(w, a))
                    .unwrap()
            };
            // A folder used elsewhere, for the empty tab's 「选一个目录」 list to offer.
            let project = directory.join("proj-a");
            std::fs::create_dir_all(&project).unwrap();
            let project = project.to_string_lossy().into_owned();
            handle
                .update(cx, |v, _, cx| v.visual_add_space("proj-a", &project, cx))
                .unwrap();
            wait(500).await;
            let before = state(cx).2.len();
            handle
                .update(cx, |v, w, cx| v.visual_new_space(w, cx))
                .unwrap();
            wait(800).await;
            let (_, renaming, spaces) = state(cx);
            eprintln!("after 新建: field={renaming:?} spaces={spaces:?}");
            shot("0-folderless-naming", cx);
            assert_eq!(
                spaces.len(),
                before + 1,
                "a space is added with no folder dialog"
            );
            assert_eq!(spaces[before], ("新空间".to_owned(), String::new()));
            assert_eq!(
                renaming,
                Some(("新空间".to_owned(), true)),
                "its name is in a focused field"
            );
            handle
                .update(cx, |v, _, cx| v.visual_space_rename_type("草稿", cx))
                .unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                w.dispatch_event(
                    gpui::KeyDownEvent {
                        keystroke: gpui::Keystroke::parse("enter").unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    }
                    .to_platform_input(),
                    a,
                );
            })
            .unwrap();
            wait(1000).await;
            let (_, renaming, spaces) = state(cx);
            assert!(renaming.is_none(), "Enter keeps the name");
            assert_eq!(spaces[before], ("草稿".to_owned(), String::new()));
            shot("1-folderless-named", cx);
            handle.update(cx, |v, _, cx| v.visual_new_tab(cx)).unwrap();
            wait(1500).await;
            let cwd = handle.update(cx, |v, _, _| v.visual_shown_cwd()).unwrap();
            let home = std::env::var("HOME").unwrap();
            eprintln!("first terminal cwd: {cwd} (HOME {home})");
            assert_eq!(cwd, home, "a space with no folder starts in $HOME");
            shot("2-folderless-terminal", cx);
            eprintln!("folderless space evidence PASS");
            cx.update(|cx| cx.quit());
        })
        .detach();
        return true;
    }
    false
}
