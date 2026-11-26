//! Two real connections to one PTY: refusal, native takeover click and owner disconnect.
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
        let other = competitor();
        until!(pane.read_with(cx, |p, _| p.has_other_owner()));
        pane.update(cx, |p, a| p.visual_type("BLOCKED-ONE\r", a));
        let error = pane.read_with(cx, |p, _| p.visual_error()).unwrap();
        eprintln!("read-only refusal: {error}");
        assert!(!pane.read_with(cx, |p, _| p.visual_text()).contains("BLOCKED-ONE"));
        shot("1-read-only-native.png", cx);
        // Click the real footer button rather than calling a test-only takeover method.
        cx.update_window(handle.into(), |_, w, a| {
            let viewport = w.viewport_size();
            native_click(w, a, point(viewport.width - px(80.), viewport.height - px(theme::Theme::STATUSBAR + theme::Theme::FOOTER / 2. + 2.)), gpui::MouseButton::Left);
        }).unwrap();
        until!(!other.state.lock().unwrap().writable);
        pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
        until!(pane.read_with(cx, |p, _| p.visual_vt_ready() && !p.has_other_owner()));
        pane.update(cx, |p, a| p.visual_type("TAKEN-ONCE\r", a));
        until!(pane.read_with(cx, |p, _| p.visual_text().contains("TAKEN-ONCE")));
        assert!(pane.read_with(cx, |p, _| p.visual_error()).is_none());
        shot("2-taken-native.png", cx);
        drop(other);
        let other = competitor();
        until!(pane.read_with(cx, |p, _| p.has_other_owner()));
        pane.update(cx, |p, a| p.visual_type("BLOCKED-TWO\r", a));
        drop(other);
        until!(!pane.read_with(cx, |p, _| p.has_other_owner()));
        pane.update(cx, |p, a| p.visual_type("PROMOTED-ONCE\r", a));
        until!(pane.read_with(cx, |p, _| p.visual_text().contains("PROMOTED-ONCE")));
        assert!(pane.read_with(cx, |p, _| p.visual_error()).is_none());
        let text = pane.read_with(cx, |p, _| p.visual_text());
        assert!(!text.contains("BLOCKED-"), "refused input was replayed: {text}");
        assert_eq!(text.matches("TAKEN-ONCE").count(), 1, "{text}");
        assert_eq!(text.matches("PROMOTED-ONCE").count(), 1, "{text}");
        shot("3-promoted-native.png", cx);
        // A split opened on another machine reaches this window while nobody is at it: the
        // pane made for it only looks, and the one who opened it keeps typing.
        let other = competitor();
        until!(pane.read_with(cx, |p, _| p.has_other_owner()));
        handle.update(cx, |v, _, a| {
            v.visual_set_window_away(true);
            v.visual_remake_shown_pane(a);
        }).unwrap();
        pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
        until!(pane.read_with(cx, |p, _| p.visual_vt_ready()));
        wait(500).await;
        assert!(other.state.lock().unwrap().writable, "a window not in use took the input");
        assert!(pane.read_with(cx, |p, _| p.has_other_owner()));
        shot("4-away-window-only-looks.png", cx);
        // In use again, a pane it makes takes over as before.
        handle.update(cx, |v, _, a| {
            v.visual_set_window_away(false);
            v.visual_remake_shown_pane(a);
        }).unwrap();
        until!(!other.state.lock().unwrap().writable);
        pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
        until!(pane.read_with(cx, |p, _| p.visual_vt_ready() && !p.has_other_owner()));
        shot("5-in-use-window-takes-over.png", cx);
        drop(other);
        std::fs::write(directory.join("result.txt"), format!("PASS: refused input never reached PTY; native takeover click and disconnect promotion each delivered once; subsequent input cleared refusal; a pane made while the window was away only looked, one made in use took over.\nRefusal: {error}\nScreen:\n{text}\n")).unwrap();
        cx.update(|cx| cx.quit());
    }).detach();
    true
}
