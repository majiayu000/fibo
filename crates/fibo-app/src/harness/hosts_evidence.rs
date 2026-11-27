//! MOORING_HOSTS_EVIDENCE: another machine in this window (one window, many hosts, step
//! 2; one UI, step 1). The outer script gives the window a machine `far` reached through a
//! fake ssh, with a space `far`. far is reached as the window opens, before any click and
//! without being drawn. Picking that space draws far's own main area beside this
//! window's own sidebar, which stays; ⌘T there opens in that space on far; 「← 本机」 comes
//! back with this machine's tabs as they were; picking far again finds its tab still there,
//! and one of this machine's spaces picked in the sidebar comes back too; what was opened
//! there is closed again. Until far is drawn its workbench holds no pane (a pane takes the
//! terminal's input from far's own window) and opens no terminal there; put away again, it
//! is back to that. MOORING_HOSTS_HOST and
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
        // A real machine's every change comes back as its whole workspace over its link,
        // seconds on a slow one: each wait stretches four times (none against far).
        let slow = if std::env::var_os("MOORING_HOSTS_HOST").is_some() { 4 } else { 1 };
        let home_tabs = handle.update(cx, |v, _, _| v.visual_tab_ids()).unwrap();
        // Reached as the window opens, before any click, and not drawn. A real machine on a
        // slow link takes two ssh setups and its whole workspace: up to two minutes.
        let mut reached = false;
        for _ in 0..120 * slow {
            wait(250).await;
            reached = handle.update(cx, |v, _, _| v.visual_reached(&host)).unwrap();
            if reached {
                break;
            }
        }
        assert!(reached, "{host} was not reached as the window opened");
        // Its rows in this sidebar come from its own workspace, its spaces included.
        let rows = handle.update(cx, |v, _, _| v.visual_machine_rows(&host)).unwrap();
        assert!(
            rows.as_ref().is_some_and(|(native, spaces)| *native && spaces.contains(&picked)),
            "{host}'s rows are not from its own workspace: {rows:?}"
        );
        let shown = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
        assert_eq!(shown.0, None, "{host} was drawn without being picked");
        // Not drawn, it does nothing there on its own. Waited, so a shell it opened for an
        // empty far, or a pane it attached, would have come back by now.
        let fake = std::env::var_os("MOORING_HOSTS_HOST").is_none();
        wait(1500 * slow).await;
        let far = handle
            .update(cx, |v, _, _| v.visual_host_view(&host))
            .unwrap()
            .expect("far's view");
        let (drawn, panes, terminals) = far.read_with(cx, |w, _| w.visual_embedded_state());
        assert!(
            !drawn && panes == 0,
            "{host} holds {panes} panes before it was picked: its own window lost their input"
        );
        assert!(
            !fake || terminals == 0,
            "{terminals} terminals were opened on {host} before it was picked"
        );
        // Picking far's space chip: its own workbench, reached in the background, drawn here,
        // and the list narrowed to that space, as a click on one of this machine's does.
        handle
            .update(cx, |v, window, cx| {
                v.visual_pick_machine_space(&host, &picked, window, cx)
            })
            .unwrap();
        let narrowed = handle.update(cx, |v, _, _| v.visual_machine_space_filter()).unwrap();
        assert_eq!(
            narrowed.as_ref(),
            Some(&picked),
            "picking {host}'s space left the sidebar listing every space"
        );
        // Already reached: drawn at the click, nothing to wait for.
        let shown = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
        assert_eq!(
            shown.0.as_ref(),
            Some(&host),
            "{host} was never drawn: {shown:?}"
        );
        // What is drawn is the workbench reached as the window opened, not a second one.
        let hosted = handle.update(cx, |v, _, _| v.visual_hosted()).unwrap();
        assert!(
            hosted.as_ref() == Some(&far),
            "the workbench drawn is not the one reached for {host}"
        );
        let far_state = |cx: &mut gpui::AsyncApp| far.read_with(cx, |w, _| w.visual_attach_state());
        let far_tabs = far.read_with(cx, |w, _| w.visual_tab_ids());
        assert!(far.read_with(cx, |w, _| w.visual_host().attached()));
        wait(800).await;
        shoot("hosts-1-far.png", cx);
        assert!(
            far.read_with(cx, |w, _| w.visual_embedded_state().0),
            "{host} is drawn but still counts as put away"
        );
        // The sidebar is the window's: far's main area is drawn beside it, far's own is not.
        let beside = far.read_with(cx, |w, _| w.visual_beside());
        assert!(
            beside.is_some(),
            "{host} was drawn without this window's sidebar"
        );
        // Its title row counts what needs you as this window does, on every machine.
        let counted = far.read_with(cx, |w, _| w.visual_board_and_wants().1);
        assert!(counted.is_some(), "{host}'s title row counts only its own");
        // Its main area shows the space picked, and the chip in this sidebar is lit.
        let lit = handle.update(cx, |v, _, cx| v.visual_shown_space(cx)).unwrap();
        assert_eq!(
            lit,
            Some((host.clone(), picked.clone())),
            "the main area does not show the space picked"
        );
        assert!(
            handle.update(cx, |v, _, _| v.visual_machine_chip_lit(&picked)).unwrap(),
            "the space picked is not lit"
        );
        // Picked again: every space is listed again and its chip goes out, while far's main
        // area stays on it; then picked once more for what follows.
        for again in [false, true] {
            handle
                .update(cx, |v, window, cx| {
                    v.visual_pick_machine_space(&host, &picked, window, cx)
                })
                .unwrap();
            let (filter, lit) = handle
                .update(cx, |v, _, _| {
                    (v.visual_machine_space_filter(), v.visual_machine_chip_lit(&picked))
                })
                .unwrap();
            assert_eq!(filter.is_some(), again, "picking it again did not toggle the list");
            assert_eq!(lit, again, "picking {host}'s space again left its chip lit: {lit}");
        }
        let still = handle.update(cx, |v, _, cx| v.visual_shown_space(cx)).unwrap();
        assert_eq!(still, lit, "picking it again moved {host}'s main area");
        // ⌘T there: a tab on far, in the space picked.
        handle
            .update(cx, |_, window, cx| {
                far.update(cx, |w, cx| {
                    assert!(w.visual_shortcut("new_tab", window, cx))
                })
            })
            .unwrap();
        let mut opened = None;
        for _ in 0..80 * slow {
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
        // Drawn, the tab opened there has its pane here, running far's own shell.
        let panes = far.read_with(cx, |w, _| w.visual_embedded_state().1);
        assert!(panes > 0, "the tab opened on {host} has no pane here");
        let programs = far.read_with(cx, |w, _| w.visual_tab_programs(&opened));
        assert!(
            !fake || programs == ["/bin/bash"],
            "⌘T on far ran {programs:?}, not far's own shell"
        );
        if fake {
            let terminal = far
                .read_with(cx, |w, _| w.visual_tab_terminal(&opened))
                .expect("remote terminal");
            let pane = far.read_with(cx, |w, _| w.visual_shown_pane());
            assert!(pane.read_with(cx, |p, _| p.visual_vt_ready() && !p.is_read_only()),
                "new work on an SSH host must be writable in the window that launched it");
            let socket = far.read_with(cx, |w, _| w.visual_socket()).canonicalize().unwrap();
            let local_window = |takeover| {
                let (tx, _) = futures::channel::mpsc::unbounded();
                crate::connection::Connection::new(&socket, &terminal, takeover, tx)
            };
            drop(pane);
            let local = local_window(true);
            wait(350).await;
            far.update(cx, |w, a| w.visual_remake_shown_pane(a));
            let pane = far.read_with(cx, |w, _| w.visual_shown_pane());
            wait(500).await;
            assert!(pane.read_with(cx, |p, _| p.is_read_only()), "reopening the SSH pane took input");
            drop(local);
            wait(350).await;
            assert!(pane.read_with(cx, |p, _| p.is_read_only()), "a passive SSH pane inherited input");
            let local = local_window(false);
            wait(350).await;
            assert!(local.state.lock().unwrap().writable, "the host's local window returned read-only");
            let before = handle.update(cx, |v, _, _| v.visual_wants()).unwrap();
            let (mut original, revision) = far.read_with(cx, |w, _| w.visual_recognition());
            original.workspace_revision = revision;
            shoot("hosts-trust-0-before.png", cx);
            let mut pending = original.clone();
            pending.terminal_trust.insert(
                terminal,
                fibo_core::protocol::TrustPrompt {
                    agent: Default::default(),
                    question: "Trust this folder?".into(),
                    folder: "/fixture".into(),
                },
            );
            far.update(cx, |w, cx| w.receive_recognition(pending, cx));
            let waiting = handle.update(cx, |v, _, _| v.visual_wants()).unwrap();
            shoot("hosts-trust-1-waiting.png", cx);
            assert_eq!(waiting, before + 1, "the sidebar missed the remote trust request");
            far.update(cx, |w, cx| w.receive_recognition(original, cx));
            let resolved = handle.update(cx, |v, _, _| v.visual_wants()).unwrap();
            shoot("hosts-trust-2-resolved.png", cx);
            assert_eq!(resolved, before, "the sidebar kept the answered remote trust request");
        }
        // 「← 本机」: this machine's own, as it was.
        handle
            .update(cx, |_, _, cx| far.update(cx, |w, cx| w.visual_go_home(cx)))
            .unwrap();
        wait(400).await;
        let shown = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
        assert_eq!(shown.0, None, "「← 本机」 left far drawn");
        assert!(
            !far.read_with(cx, |w, _| w.visual_embedded_state().0),
            "{host} was put away but still counts as drawn"
        );
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
        // One of this machine's spaces in the sidebar: this machine's main area again.
        handle
            .update(cx, |v, _, cx| {
                let home = v.visual_spaces().into_iter().next().expect("a space here");
                v.visual_pick_space(&home, cx)
            })
            .unwrap();
        wait(400).await;
        let shown = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
        assert_eq!(shown.0, None, "this machine's space left {host} drawn");
        shoot("hosts-5-home-space.png", cx);
        // ⌘J in far's main area, with something waiting: the window's next, so input moves.
        if counted.is_some_and(|n| n > 1) {
            let before = far.read_with(cx, |w, _| w.visual_focused());
            handle
                .update(cx, |v, window, cx| {
                    v.visual_show_host(&host, None, window, cx);
                    far.update(cx, |w, cx| {
                        assert!(w.visual_shortcut("next_waiting", window, cx))
                    })
                })
                .unwrap();
            let mut moved = false;
            for _ in 0..6 * slow {
                wait(250).await;
                // That machine's workbench goes where it was sent when it next draws; this
                // window draws only when shot.
                shoot("hosts-5b-next.png", cx);
                let after = far.read_with(cx, |w, _| w.visual_focused());
                let shown = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
                moved = after != before || shown.0.is_none();
                if moved {
                    break;
                }
            }
            assert!(moved, "⌘J in {host} went nowhere: {before:?}");
            shoot("hosts-5b-next.png", cx);
        }
        // With peek on, a peek opened while far's main area is drawn floats over it. Only
        // against the outer script's far: peek is a setting of the HOME it runs in.
        let shell = far.read_with(cx, |w, _| w.visual_focused()).filter(|_| fake);
        if let Some(shell) = shell {
            handle
                .update(cx, |v, window, cx| {
                    crate::ui::peek::choose(true, cx).unwrap();
                    v.visual_show_host(&host, None, window, cx);
                    v.visual_peek_remote(0, &shell, window, cx)
                })
                .unwrap();
            let mut drawn = None;
            for _ in 0..40 {
                wait(250).await;
                shoot("hosts-5c-peek.png", cx);
                drawn = handle.update(cx, |v, _, cx| v.visual_peek_bounds(cx)).unwrap();
                if drawn.is_some() {
                    break;
                }
            }
            assert!(drawn.is_some(), "a peek opened over {host} was never drawn");
            handle
                .update(cx, |v, _, cx| {
                    v.visual_close_peek(cx);
                    crate::ui::peek::choose(false, cx).unwrap();
                })
                .unwrap();
        }
        // 鸟瞰 pressed in far's main area: this window's board, which holds every machine.
        handle
            .update(cx, |v, window, cx| {
                v.visual_show_host(&host, None, window, cx)
            })
            .unwrap();
        wait(400).await;
        handle
            .update(cx, |_, _, cx| far.update(cx, |w, cx| w.visual_press_board(cx)))
            .unwrap();
        wait(400).await;
        let shown = handle.update(cx, |v, _, _| v.visual_hosts()).unwrap();
        let board = handle.update(cx, |v, _, _| v.visual_board_and_wants().0).unwrap();
        assert!(
            shown.0.is_none() && board,
            "鸟瞰 in {host} did not open this window's board: {shown:?}"
        );
        shoot("hosts-6-board.png", cx);
        if fake {
            handle.update(cx, |_, _, cx| far.update(cx, |w, cx| w.visual_failed_host_request(cx))).unwrap();
            let mut error = String::new();
            for _ in 0..40 {
                wait(100).await;
                error = handle.update(cx, |v, _, _| v.visual_attach_error()).unwrap();
                if error.contains("terminal not found") { break; }
            }
            shoot("hosts-6b-failure.png", cx);
            assert!(error.contains("terminal not found"), "hidden host failure was not shown: {error:?}");
        }
        // 置顶 is far's host's: pinned from this window's menu on far's row, far's host
        // keeps it, so far's own window lists it too; changed by another window on far,
        // this sidebar follows.
        let terminal = far
            .read_with(cx, |w, _| w.visual_tab_terminal(&opened))
            .expect("the tab opened on far has a terminal");
        let said = handle
            .update(cx, |v, _, cx| v.visual_remote_menu_pin(0, &terminal, cx))
            .unwrap();
        assert_eq!(said, Some(t!("置顶")), "far's row offered no 置顶");
        let pinned = |pins: &Option<Vec<String>>| pins.iter().flatten().any(|p| *p == terminal);
        let mut kept = None;
        for _ in 0..40 * slow {
            wait(250).await;
            kept = far.read_with(cx, |w, _| w.visual_host_pins());
            if pinned(&kept) {
                break;
            }
        }
        assert!(pinned(&kept), "pinned here, {host}'s host did not keep it: {kept:?}");
        let follows = async |want: bool, cx: &mut gpui::AsyncApp| {
            for _ in 0..40 * slow {
                wait(250).await;
                let seen = handle.update(cx, |v, _, _| v.visual_machine_pins(0)).unwrap();
                if pinned(&seen) == want {
                    return true;
                }
            }
            false
        };
        assert!(follows(true, cx).await, "this sidebar did not list far's pin");
        shoot("hosts-6c-pinned.png", cx);
        far.read_with(cx, |w, _| w.visual_host_pin(&terminal, false));
        assert!(follows(false, cx).await, "unpinned on far, this sidebar kept it");
        far.read_with(cx, |w, _| w.visual_host_pin(&terminal, true));
        assert!(follows(true, cx).await, "pinned on far, this sidebar did not list it");
        let said = handle
            .update(cx, |v, _, cx| v.visual_remote_menu_pin(0, &terminal, cx))
            .unwrap();
        assert_eq!(said, Some(t!("取消置顶")), "far's pinned row offered no 取消置顶");
        assert!(follows(false, cx).await, "unpinned here, this sidebar kept it");
        let kept = far.read_with(cx, |w, _| w.visual_host_pins());
        assert!(!pinned(&kept), "unpinned here, {host}'s host kept it: {kept:?}");
        eprintln!("hosts pins PASS: pinned here kept by {host}'s host, pinned or unpinned there listed here");
        // Leaves far as it was.
        handle
            .update(cx, |_, _, cx| {
                far.update(cx, |w, cx| w.visual_close_tab_id(&opened, cx))
            })
            .unwrap();
        // Waited out, so a close still on its way over a slow link is not cut by quitting.
        let mut left = true;
        for _ in 0..40 * slow {
            wait(250).await;
            left = far
                .read_with(cx, |w, _| w.visual_tab_ids())
                .contains(&opened);
            if !left {
                break;
            }
        }
        assert!(!left, "the tab opened on {host} was left there");
        // 「换目录」 on far: a path there typed, its folders listed by far's host, and a new
        // folder made there and taken as the space's.
        if fake {
            let farhome = std::path::PathBuf::from(std::env::var("MOORING_HOSTS_FARHOME").unwrap());
            let typed = |text: &'static str, cx: &mut gpui::AsyncApp| {
                let (far, picked) = (far.clone(), picked.clone());
                cx.update_window(handle.into(), move |_, window, app| {
                    far.update(app, |w, cx| w.visual_type_remote_folder(&picked, text, window, cx))
                })
                .unwrap();
            };
            let offered = |cx: &mut gpui::AsyncApp| far.read_with(cx, |w, _| w.visual_remote_folder());
            // Drawn, in that space, so the shots show its empty tab and the field.
            handle
                .update(cx, |v, window, cx| v.visual_show_host(&host, Some(&picked), window, cx))
                .unwrap();
            wait(800).await;
            typed("~/co", cx);
            let code = farhome.join("code").to_string_lossy().into_owned();
            let mut seen = None;
            for _ in 0..40 {
                wait(100).await;
                seen = offered(cx);
                if seen.as_ref().is_some_and(|s| s.0.contains(&code)) {
                    break;
                }
            }
            assert!(seen.as_ref().is_some_and(|s| s.0.contains(&code)), "far listed {seen:?}");
            typed("~/code/new-proj", cx);
            wait(800).await;
            let seen = offered(cx);
            let make = seen.as_ref().and_then(|s| s.1.clone());
            assert_eq!(make.as_deref(), Some("~/code/new-proj"), "offered {seen:?}");
            shoot("far-folder-make.png", cx);
            handle
                .update(cx, |_, _, cx| far.update(cx, |w, cx| w.visual_choose_remote_folder(true, cx)))
                .unwrap();
            let made = farhome.join("code/new-proj");
            let want = made.to_string_lossy().into_owned();
            let mut set = None;
            for _ in 0..40 {
                wait(100).await;
                set = far.read_with(cx, |w, _| w.visual_space_folder(&picked));
                if set.as_deref() == Some(want.as_str()) {
                    break;
                }
            }
            assert!(made.is_dir(), "no folder made on {host}");
            assert_eq!(set.as_deref(), Some(want.as_str()), "{picked}'s folder on {host}");
            shoot("far-folder-set.png", cx);
            eprintln!("hosts folder PASS: {host}'s folders listed, one made there and set");
        }
        eprintln!(
            "hosts PASS: {host} beside this sidebar, ⌘T in {picked}, home kept, {host} kept its tab, a space here came back"
        );
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
