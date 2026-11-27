//! One row lit, wherever the view went: 「选中另一台的对话，侧栏的光标不跟过去」. With
//! peek off a click on another machine's row draws that machine here, with it on the row
//! floats up; either way, and after ⌘J, 回到刚才 and coming back, the sidebar lights the
//! row of what is in front and no other.
//! MOORING_REAL_SELECTION_EVIDENCE walks the same on this machine's own host and the
//! machines in its machines.json, with rows picked from what they list, saving each step.
use super::*;

/// Where the walk goes: a row of this machine's (its CLI id and session id) and one of
/// machine 0's (its terminal and row key).
struct Walk {
    here_cli: String,
    here: String,
    far_terminal: String,
    far: String,
    /// The fixture's: ⌘J has one place to go (this machine's row) and coming back lands
    /// there. With real rows ⌘J goes wherever the queue says: one row lit is what holds.
    strict: bool,
    shots: Option<PathBuf>,
}

pub(super) async fn one_row_lit(
    handle: WindowHandle<workbench::Workbench>,
    cx: &mut gpui::AsyncApp,
) {
    let here = handle
        .update(cx, |v, _, _| v.visual_session_id("here-codex"))
        .unwrap();
    let far = handle
        .update(cx, |v, _, cx| v.visual_remote_row_key(0, "far", cx))
        .unwrap();
    let walk = Walk {
        here_cli: "here-codex".into(),
        here,
        far_terminal: "far".into(),
        far,
        strict: true,
        shots: None,
    };
    walk.run(handle, cx).await;
    cx.update(|cx| cx.set_global(crate::ui::peek::PeekChoice(true)));
    eprintln!("one row lit PASS");
}

/// Against the real hosts: waits for machine 0's rows, picks the first open session on
/// each side, walks, and puts peek back as it was chosen.
pub(super) fn real(
    handle: WindowHandle<workbench::Workbench>,
    directory: PathBuf,
    cx: &mut App,
) -> bool {
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        std::fs::create_dir_all(&directory).unwrap();
        let mut picked = None;
        // Two ssh setups and a whole workspace on a slow link: up to two minutes.
        for _ in 0..240 {
            executor.timer(Duration::from_millis(500)).await;
            picked = handle
                .update(cx, |v, _, cx| v.visual_pick_rows(cx))
                .unwrap();
            if picked.is_some() {
                break;
            }
        }
        let (here_cli, here, far_terminal, far) =
            picked.expect("no open session here and on machine 0 to walk between");
        eprintln!("walking {here} here and {far} there");
        let chosen = cx.update(|cx| crate::ui::peek::enabled(cx));
        let walk = Walk {
            here_cli,
            here,
            far_terminal,
            far,
            strict: false,
            shots: Some(directory),
        };
        walk.run(handle, cx).await;
        cx.update(|cx| cx.set_global(crate::ui::peek::PeekChoice(chosen)));
        eprintln!("real one row lit PASS");
        cx.update(|cx| cx.quit());
    })
    .detach();
    true
}

impl Walk {
    async fn run(&self, handle: WindowHandle<workbench::Workbench>, cx: &mut gpui::AsyncApp) {
        let executor = cx.background_executor().clone();
        let lit =
            |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, cx| v.visual_lit_rows(cx)).unwrap();
        let mut step = 0;
        // A machine drawn here the first time is reached first: up to a few seconds.
        let mut settle = async |want: &dyn Fn(&[String]) -> bool,
                                what: &str,
                                cx: &mut gpui::AsyncApp| {
            let mut now = Vec::new();
            for _ in 0..80 {
                executor.timer(Duration::from_millis(250)).await;
                // Another machine drawn here goes where it was asked to on its next frame.
                cx.update_window(handle.into(), |_, window, app| {
                    window.refresh();
                    window.draw(app).clear(app);
                })
                .unwrap();
                now = lit(cx);
                if want(&now) {
                    break;
                }
            }
            step += 1;
            if let Some(dir) = &self.shots {
                let name = format!("{step:02}-{}.png", what.replace([' ', ','], "-"));
                cx.update_window(handle.into(), |_, window, _| {
                    window
                        .render_to_image()
                        .unwrap()
                        .save(dir.join(&name))
                        .unwrap();
                })
                .unwrap();
                eprintln!("{name}: lit {now:?}");
            }
            if want(&now) {
                return;
            }
            let (hosts, focus) = handle
                .update(cx, |v, _, cx| {
                    let hosted = v.visual_hosted().map(|h| h.read(cx).visual_focused());
                    (v.visual_hosts(), hosted)
                })
                .unwrap();
            panic!("{what}: lit {now:?}, drawn and reaching {hosts:?}, focused there {focus:?}");
        };
        let only = |key: &str| {
            let key = key.to_owned();
            move |now: &[String]| now == [key.clone()]
        };
        let one = |now: &[String]| now.len() == 1;
        let strict = self.strict;
        let after_jump = |now: &[String]| {
            if strict {
                only(&self.here)(now)
            } else {
                one(now)
            }
        };
        for peek in [true, false] {
            let mode = if peek { "peek on" } else { "peek off" };
            cx.update(|cx| cx.set_global(crate::ui::peek::PeekChoice(peek)));
            handle
                .update(cx, |v, window, cx| {
                    v.visual_click_row(&self.here_cli, window, cx)
                })
                .unwrap();
            settle(
                &only(&self.here),
                &format!("{mode}, this machine's row clicked"),
                cx,
            )
            .await;
            handle
                .update(cx, |v, window, cx| {
                    v.visual_click_remote_row(0, &self.far_terminal, window, cx)
                })
                .unwrap();
            settle(
                &only(&self.far),
                &format!("{mode}, the other machine's row clicked"),
                cx,
            )
            .await;
            // In the fixture ⌘J goes to this machine's session, the only other one waiting.
            handle
                .update(cx, |v, window, cx| v.visual_next_waiting(window, cx))
                .unwrap();
            let what = format!("{mode}, the next-waiting key from the other machine");
            settle(&after_jump, &what, cx).await;
            handle
                .update(cx, |v, window, cx| v.visual_go_back(window, cx))
                .unwrap();
            settle(
                &only(&self.far),
                &format!("{mode}, going back to the other machine"),
                cx,
            )
            .await;
            handle
                .update(cx, |v, window, cx| {
                    v.visual_close_peek(cx);
                    v.visual_show_home(window, cx);
                })
                .unwrap();
            settle(&after_jump, &format!("{mode}, back on this machine"), cx).await;
        }
    }
}
