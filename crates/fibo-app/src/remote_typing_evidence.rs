//! MOORING_REMOTE_TYPING_EVIDENCE: typing into another machine's terminal from its peek
//! (multi-machine step 2). The outer script sets up that machine: its own host, a
//! terminal `far` appending what it reads to MOORING_FAR_TYPED, and a machines.json whose
//! connection reaches it. Saves the peek and checks the line arrived once.
use super::*;
use gpui::WindowHandle;
use std::time::Duration;
#[path = "remote_board_evidence.rs"]
mod board;
#[path = "remote_sidebar_evidence.rs"]
mod sidebar;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_REMOTE_BOARD_EVIDENCE").is_some() {
        return board::start(handle, directory, cx);
    }
    if std::env::var_os("MOORING_REMOTE_SIDEBAR_EVIDENCE").is_some() {
        return sidebar::start(handle, directory, cx);
    }
    if std::env::var_os("MOORING_REMOTE_TYPING_EVIDENCE").is_none() {
        return false;
    }
    let typed = PathBuf::from(std::env::var("MOORING_FAR_TYPED").expect("MOORING_FAR_TYPED"));
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        std::fs::create_dir_all(&directory).unwrap();
        let mut opened = false;
        for _ in 0..60 {
            wait(250).await;
            opened = handle
                .update(cx, |v, window, cx| {
                    v.visual_peek_remote(0, "far", window, cx)
                })
                .unwrap();
            if opened {
                break;
            }
        }
        assert!(opened, "the other machine never listed its terminal");
        wait(1500).await;
        let (pane, read_only) = handle
            .update(cx, |v, _, _| v.visual_peek_pane())
            .unwrap()
            .unwrap();
        assert!(
            !read_only,
            "a machine that allows input is still marked 只读"
        );
        pane.update(cx, |p, cx| p.visual_type("hello-far\r", cx));
        wait(1500).await;
        let text = std::fs::read_to_string(&typed).unwrap_or_default();
        assert_eq!(text.matches("hello-far").count(), 1, "{text:?}");
        cx.update_window(handle.into(), |_, window, app| {
            window.refresh();
            window.draw(app).clear(app);
            let shot = window.render_to_image().unwrap();
            shot.save(directory.join("remote-typing.png")).unwrap();
        })
        .unwrap();
        eprintln!("remote typing PASS: peek writable, line typed once on the other machine");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}
