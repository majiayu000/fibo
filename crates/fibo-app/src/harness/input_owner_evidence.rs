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
        until!(pane.read_with(cx, |p, _| p.is_read_only()));
        shot("1-read-only-native.png", cx);
        // Made again, as a tab switch or a split synced from another machine makes it, the
        // pane still only looks: showing a terminal is not wanting its input.
        handle.update(cx, |v, _, a| v.visual_remake_shown_pane(a)).unwrap();
        pane = shown(cx);
        until!(pane.read_with(cx, |p, _| p.visual_vt_ready()));
        wait(500).await;
        assert!(writable(&other), "showing a pane took the input");
        assert!(pane.read_with(cx, |p, _| p.is_read_only()));
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
        until!(pane.read_with(cx, |p, _| p.is_read_only()));
        cx.update_window(handle.into(), |_, w, a| {
            let viewport = w.viewport_size();
            w.refresh(); w.draw(a).clear(a);
            native_click(w, a, point(viewport.width - px(40.), viewport.height - px(theme::Theme::STATUSBAR + theme::Theme::FOOTER / 2. + 2.)), gpui::MouseButton::Left);
        }).unwrap();
        until!(!writable(&other));
        pane = shown(cx);
        until!(pane.read_with(cx, |p, _| p.visual_vt_ready() && !p.is_read_only()));
        pane.update(cx, |p, a| p.visual_type("CLICKED-ONCE\r", a));
        until!(pane.read_with(cx, |p, _| p.visual_text().contains("CLICKED-ONCE")));
        drop(other);
        // The writer leaving hands the input to the one still looking.
        let other = competitor();
        until!(pane.read_with(cx, |p, _| p.is_read_only()));
        drop(other);
        until!(!pane.read_with(cx, |p, _| p.is_read_only()));
        pane.update(cx, |p, a| p.visual_type("PROMOTED-ONCE\r", a));
        until!(pane.read_with(cx, |p, _| p.visual_text().contains("PROMOTED-ONCE")));
        assert!(pane.read_with(cx, |p, _| p.visual_error()).is_none());
        let text = pane.read_with(cx, |p, _| p.visual_text());
        for word in ["TYPED-TAKES", "AFTER-TAKE", "CLICKED-ONCE", "PROMOTED-ONCE"] {
            assert_eq!(text.matches(word).count(), 1, "{word}: {text}");
        }
        shot("4-promoted-native.png", cx);
        // A formerly 127-column terminal on a narrower display retains readable text.
        let other = competitor();
        until!(writable(&other));
        other.send(fibo_core::protocol::Request::Resize {
            cols: 127, rows: 34, cell_width: 16, cell_height: 32,
        }).unwrap();
        // Enough fixed-width content to reach the right edge on the first row.
        let marker = format!("LEFT_EDGE{}RIGHT_EDGE", ".".repeat(107));
        other.send(fibo_core::protocol::Request::Input {
            bytes: format!("{marker}\r").into_bytes(), lossy: false,
        }).unwrap();
        until!(pane.read_with(cx, |p, _| p.visual_text_shown().contains(&marker)));
        handle.update(cx, |_, w, _| w.resize(size(px(860.), px(800.)))).unwrap();
        wait(350).await;
        shot("5-narrow-left-native.png", cx);
        let (view, last_cell, cell_width) = pane.read_with(cx, |p, _| {
            let row = p.visual_text_shown().lines().position(|line| line.contains(&marker)).unwrap() as u16;
            let at = |col| p.visual_cell(row, col).unwrap();
            (p.visual_bounds().unwrap(), at(125), at(1).x - at(0).x)
        });
        assert!(!view.contains(&last_cell), "the wide grid should overflow");
        cx.update_window(handle.into(), |_, w, a| {
            use gpui::InputEvent;
            w.dispatch_event(gpui::ScrollWheelEvent {
                position: view.center(),
                delta: gpui::ScrollDelta::Pixels(point(px(-2000.), px(0.))),
                modifiers: Default::default(),
                touch_phase: gpui::TouchPhase::Moved,
            }.to_platform_input(), a);
        }).unwrap();
        shot("6-narrow-right-native.png", cx);
        assert!(pane.read_with(cx, |p, _| {
            let row = p.visual_text_shown().lines().position(|line| line.contains(&marker)).unwrap() as u16;
            view.contains(&p.visual_cell(row, 125).unwrap()) && p.is_read_only()
        }), "the line end stayed outside the read-only view");
        assert!(writable(&other), "scrolling took the input");
        cx.update_window(handle.into(), |_, w, a| {
            native_click(w, a, point(view.left() + px(2.), view.bottom() - px(3.)), gpui::MouseButton::Left);
        }).unwrap();
        shot("7-narrow-scrollbar-native.png", cx);
        assert!(pane.read_with(cx, |p, _| {
            let row = p.visual_text_shown().lines().position(|line| line.contains(&marker)).unwrap() as u16;
            view.contains(&p.visual_cell(row, 0).unwrap())
        }), "clicking the left end of the scrollbar did not return to the first column");
        handle.update(cx, |_, w, _| w.resize(size(px(860.), px(460.)))).unwrap();
        wait(350).await;
        shot("8-short-bottom-native.png", cx);
        let short = pane.read_with(cx, |p, _| p.visual_bounds().unwrap());
        assert!(!pane.read_with(cx, |p, _| short.contains(&p.visual_cell(0, 0).unwrap())));
        cx.update_window(handle.into(), |_, w, a| {
            use gpui::InputEvent;
            w.dispatch_event(gpui::ScrollWheelEvent {
                position: short.center(),
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(1000.))),
                modifiers: Default::default(),
                touch_phase: gpui::TouchPhase::Moved,
            }.to_platform_input(), a);
        }).unwrap();
        shot("9-short-top-native.png", cx);
        assert!(pane.read_with(cx, |p, _| short.contains(&p.visual_cell(0, 0).unwrap())));
        assert!(writable(&other), "vertical scrolling took the input");
        handle.update(cx, |_, w, _| w.resize(size(px(860.), px(800.)))).unwrap();
        wait(350).await;
        drop(other);
        until!(pane.read_with(cx, |p, _| !p.is_read_only()));
        shot("10-narrow-after-takeover-native.png", cx);
        assert_eq!(pane.read_with(cx, |p, _| {
            p.visual_cell(0, 1).unwrap().x - p.visual_cell(0, 0).unwrap().x
        }), cell_width, "read-only text was smaller than the saved size");
        std::fs::write(directory.join("result.txt"), format!("PASS: the 127-column grid kept its saved cell width ({cell_width:?}); a sideways gesture brought RIGHT_EDGE into view and a click on the scrollbar returned to the start. After reducing the height, vertical scrolling reached the top of the grid. The other window remained the writer through scrolling. Typing, the footer and the writer leaving still handed input over.\n")).unwrap();
        cx.update(|cx| cx.quit());
    }).detach();
    true
}
