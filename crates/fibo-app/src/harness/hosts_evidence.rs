//! MOORING_HOSTS_EVIDENCE: another machine in this window (one window, many hosts, step
//! 2). The outer script gives the window a machine `far` reached through a fake ssh, with a
//! space `far`. Picking that space draws far's own workbench here; ⌘T there opens in that
//! space on far; 「← 本机」 comes back with this machine's tabs as they were; picking far again
//! finds its tab still there; what was opened there is closed again. MOORING_HOSTS_HOST and
//! MOORING_HOSTS_SPACE name another machine and space (a real one) instead of far's.
use super::*;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_HOSTS_EVIDENCE").is_none() {
        return false;
    }
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        std::fs::create_dir_all(&directory).unwrap();
        let shoot = |name: &str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, window, app| {
                window.refresh();
                window.draw(app).clear(app);
                window
                    .render_to_image()
                    .unwrap()
                    .save(directory.join(name))
                    .unwrap();
            })
            .unwrap();
        };
        let host = std::env::var("MOORING_HOSTS_HOST").unwrap_or_else(|_| "far".into());
        let picked = std::env::var("MOORING_HOSTS_SPACE").unwrap_or_else(|_| "far".into());
        let home_tabs = handle.update(cx, |v, _, _| v.visual_tab_ids()).unwrap();
        // Picking far's space: its own workbench, reached in the background, drawn here.
        handle
            .update(cx, |v, window, cx| {
                v.visual_show_host(&host, Some(&picked), window, cx)
            })
            .unwrap();
        let mut shown = (None, None);
        for _ in 0..120 {
            wait(250).await;
            shown = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
            if shown.0.is_some() {
                break;
            }
        }
        assert_eq!(
            shown.0.as_ref(),
            Some(&host),
            "{host} was never drawn: {shown:?}"
        );
        let far = handle
            .update(cx, |v, _, _| v.visual_hosted())
            .unwrap()
            .expect("far's view");
        let far_state = |cx: &mut gpui::AsyncApp| far.read_with(cx, |w, _| w.visual_attach_state());
        let far_tabs = far.read_with(cx, |w, _| w.visual_tab_ids());
        assert!(far.read_with(cx, |w, _| w.visual_host().attached()));
        wait(800).await;
        shoot("hosts-1-far.png", cx);
        // ⌘T there: a tab on far, in the space picked.
        handle
            .update(cx, |_, window, cx| {
                far.update(cx, |w, cx| {
                    assert!(w.visual_shortcut("new_tab", window, cx))
                })
            })
            .unwrap();
        let mut opened = None;
        for _ in 0..80 {
            wait(250).await;
            let now = far.read_with(cx, |w, _| w.visual_tab_ids());
            opened = now.into_iter().find(|t| !far_tabs.contains(t));
            if opened.is_some() {
                break;
            }
        }
        let opened =
            opened.unwrap_or_else(|| panic!("⌘T on far opened nothing: {:?}", far_state(cx)));
        let space = far.read_with(cx, |w, _| w.visual_tab_space(&opened));
        assert_eq!(
            space.as_ref(),
            Some(&picked),
            "⌘T opened outside the space picked"
        );
        wait(1500).await;
        shoot("hosts-2-far-tab.png", cx);
        // 「← 本机」: this machine's own, as it was.
        handle
            .update(cx, |_, _, cx| far.update(cx, |w, cx| w.visual_go_home(cx)))
            .unwrap();
        wait(400).await;
        let shown = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
        assert_eq!(shown.0, None, "「← 本机」 left far drawn");
        let tabs = handle.update(cx, |v, _, _| v.visual_tab_ids()).unwrap();
        assert_eq!(tabs, home_tabs, "this machine's tabs changed");
        shoot("hosts-3-home.png", cx);
        // far again: the same workbench, its tab still there.
        handle
            .update(cx, |v, window, cx| {
                v.visual_show_host(&host, None, window, cx)
            })
            .unwrap();
        wait(400).await;
        let again = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
        assert_eq!(
            again,
            (Some(host.clone()), None),
            "{host} was reached again"
        );
        let kept = far.read_with(cx, |w, _| w.visual_tab_ids());
        assert!(kept.contains(&opened), "far lost its tab meanwhile");
        shoot("hosts-4-far-again.png", cx);
        // Leaves far as it was.
        handle
            .update(cx, |_, _, cx| {
                far.update(cx, |w, cx| w.visual_close_tab_id(&opened, cx))
            })
            .unwrap();
        for _ in 0..40 {
            wait(250).await;
            if !far
                .read_with(cx, |w, _| w.visual_tab_ids())
                .contains(&opened)
            {
                break;
            }
        }
        eprintln!("hosts PASS: {host} drawn here, ⌘T in {picked}, home kept, {host} kept its tab");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
