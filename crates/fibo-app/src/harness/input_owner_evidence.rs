//! Two real connections to one PTY: a pane shown again only looks, typing takes the input,
//! and the takeover click and the writer leaving still hand it over.
use super::*;
use crate::connection::Connection;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_INPUT_OWNER_EVIDENCE").is_none()
        && std::env::var("MOORING_PAGE_LIST").as_deref() != Ok("InputOwner")
    {
        return false;
    }
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        std::fs::create_dir_all(&directory).unwrap();
        wait(1000).await;
        handle.update(cx, |v, w, a| {
            w.resize(size(px(1200.), px(800.)));
            v.visual_new_tab(a);
        }).unwrap();
        wait(1000).await;
        let (socket, id, mut pane) = handle.update(cx, |v, _, _| {
            (v.visual_socket(), v.visual_shown_terminal(), v.visual_shown_pane())
        }).unwrap();
        macro_rules! until {
            ($condition:expr) => {{
                let mut ready = false;
                for _ in 0..100 {
                    if $condition { ready = true; break; }
                    wait(50).await;
                }
                assert!(ready, "timed out: {}", stringify!($condition));
            }};
        }
        until!(pane.read_with(cx, |p, _| p.visual_vt_ready()));
        pane.update(cx, |p, a| p.visual_type("stty -echo; cat\r", a));
        wait(500).await;
        let competitor = || {
            let (tx, _rx) = futures::channel::mpsc::unbounded();
            Connection::new(&socket, &id, true, tx)
        };
        let shot = |name: &str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh(); w.draw(a).clear(a);
                w.render_to_image().unwrap().save(directory.join(name)).unwrap();
            }).unwrap();
        };
        let shown = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
        let writable = |c: &Connection| c.state.lock().unwrap().writable;
        // Another window or machine takes the terminal: this pane only looks.
        let other = competitor();
        until!(pane.read_with(cx, |p, _| p.has_other_owner()));
        shot("1-read-only-native.png", cx);
        // Made again, as a tab switch or a split synced from another machine makes it, the
        // pane still only looks: showing a terminal is not wanting its input.
        handle.update(cx, |v, _, a| v.visual_remake_shown_pane(a)).unwrap();
        pane = shown(cx);
        until!(pane.read_with(cx, |p, _| p.visual_vt_ready()));
        wait(500).await;
        assert!(writable(&other), "showing a pane took the input");
        assert!(pane.read_with(cx, |p, _| p.has_other_owner()));
        shot("2-shown-pane-only-looks.png", cx);
        // Typing in it is wanting it: it takes over and what was typed arrives once.
        pane.update(cx, |p, a| p.visual_type("TYPED-TAKES\r", a));
        until!(!writable(&other));
        until!(pane.read_with(cx, |p, _| p.visual_text().contains("TYPED-TAKES")));
        pane.update(cx, |p, a| p.visual_type("AFTER-TAKE\r", a));
        until!(pane.read_with(cx, |p, _| p.visual_text().contains("AFTER-TAKE")));
        assert!(pane.read_with(cx, |p, _| p.visual_error()).is_none());
        shot("3-typing-took-over.png", cx);
        drop(other);
        // The footer button still takes it, for a program used only with the mouse. Click
        // the real button rather than calling a test-only takeover method.
        let other = competitor();
        until!(pane.read_with(cx, |p, _| p.has_other_owner()));
        cx.update_window(handle.into(), |_, w, a| {
            let viewport = w.viewport_size();
            native_click(w, a, point(viewport.width - px(80.), viewport.height - px(theme::Theme::STATUSBAR + theme::Theme::FOOTER / 2. + 2.)), gpui::MouseButton::Left);
        }).unwrap();
        until!(!writable(&other));
        pane = shown(cx);
        until!(pane.read_with(cx, |p, _| p.visual_vt_ready() && !p.has_other_owner()));
        pane.update(cx, |p, a| p.visual_type("CLICKED-ONCE\r", a));
        until!(pane.read_with(cx, |p, _| p.visual_text().contains("CLICKED-ONCE")));
        drop(other);
        // The writer leaving hands the input to the one still looking.
        let other = competitor();
        until!(pane.read_with(cx, |p, _| p.has_other_owner()));
        drop(other);
        until!(!pane.read_with(cx, |p, _| p.has_other_owner()));
        pane.update(cx, |p, a| p.visual_type("PROMOTED-ONCE\r", a));
        until!(pane.read_with(cx, |p, _| p.visual_text().contains("PROMOTED-ONCE")));
        assert!(pane.read_with(cx, |p, _| p.visual_error()).is_none());
        let text = pane.read_with(cx, |p, _| p.visual_text());
        for word in ["TYPED-TAKES", "AFTER-TAKE", "CLICKED-ONCE", "PROMOTED-ONCE"] {
            assert_eq!(text.matches(word).count(), 1, "{word}: {text}");
        }
        shot("4-promoted-native.png", cx);
        std::fs::write(directory.join("result.txt"), format!("PASS: a pane made again only looked; typing took the input and the typed line arrived once; the footer click and the writer leaving each handed it over.\nScreen:\n{text}\n")).unwrap();
        cx.update(|cx| cx.quit());
    }).detach();
    true
}
