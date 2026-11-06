//! MOORING_PINNED_ASK_EVIDENCE: the turn's ask pinned while its answer runs past the top: a
//! Claude-drawn pane gets the bar and a Codex one (which pins its own) none; an ask typed
//! under a paste is found and stays pinned while scrolled up in its answer; a click goes
//! back to the ask; reading the older turn floats its ask; the header button when chosen
//! instead.
use super::*;
use gpui::WindowHandle;
use std::time::Duration;
use workbench::Workbench;
#[path = "todo_demo_evidence.rs"]
mod todo_demo_evidence;

pub(super) fn start(
    handle: Option<WindowHandle<Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if todo_demo_evidence::start(handle, &directory, cx) {
        return true;
    }
    if std::env::var_os("MOORING_PINNED_ASK_EVIDENCE").is_none() {
        return false;
    }
    cx.spawn(async move |cx| {
        use crate::ui::pinned_ask::PinnedAsk;
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        wait(1500).await;
        std::fs::create_dir_all(&directory).unwrap();
        let socket = handle.update(cx, |v, _, _| v.visual_socket()).unwrap();
        let call = |r: Request| rpc(&socket, &r).unwrap();
        let w = match call(Request::Workspace) {
            Reply::Workspace(w) => w,
            x => panic!("{x:?}"),
        };
        let space = match w.spaces.first() {
            Some(s) => s.id.clone(),
            None => {
                call(Request::Edit {
                    request: None,
                    edit: Edit::AddSpace(Space {
                        id: "s".into(),
                        name: "Tests".into(),
                        cwd: "/tmp".into(),
                    }),
                });
                "s".into()
            }
        };
        // A one-character reply, as when answering a numbered menu, then an ask typed
// under a paste: the mark opens the paste and the transcript keeps only the words.
let asks = ["先看看架构页还漏了什么", "1", "这些能都加回来吗"];
        for (id, mark) in [("pc", "❯"), ("px", "›")] {
            let mut text = String::new();
            for (turn, ask) in asks.iter().enumerate() {
                if turn == 2 {
text.push_str(&format!(
"{mark} │ 前台标记 │ 删了 │\n\n    ├──────┼────┤\n\n    │ 红 ✕ │ 没做 │\n\n  {ask}\n\n"
));
} else {
text.push_str(&format!("{mark} {ask}\n\n"));
}
for n in 1..=80 {
text.push_str(&format!("  {}.{n} 这一轮的回答\n", turn + 1));
}
                text.push('\n');
            }
            let file = directory.join(format!("{id}.txt"));
            std::fs::write(&file, text).unwrap();
            call(Request::Launch {
                id: id.into(),
                space: space.clone(),
                argv: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    format!("cat '{}'; exec /bin/sh -i", file.display()),
                ],
                cwd: "/tmp".into(),
                cols: 90,
                rows: 24,
                scheme: Default::default(),
                colors: None,
            });
        }
        let layout = Layout::Split {
            axis: mooring_core::layout::Axis::Right,
            ratio: 5000,
            first: Box::new(Layout::Leaf("pc".into())),
            second: Box::new(Layout::Leaf("px".into())),
        };
        call(Request::Edit {
            request: None,
            edit: Edit::CreateTab {
                tab: Tab {
                    id: "tab-pinned".into(),
                    space,
                    default_task: None,
                    title: "pinned".into(),
                    layout: Some(layout),
                },
            },
        });
        wait(800).await;
        handle
            .update(cx, |v, _, a| v.visual_peek_show_tab("pinned", a))
            .unwrap();
        wait(800).await;
        handle
            .update(cx, |v, _, a| v.visual_pinned_ask_fixture(&asks, a))
            .unwrap();
        let shot = |name: &str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh();
                w.draw(a).clear(a);
                w.render_to_image()
                    .unwrap()
                    .save(directory.join(name))
                    .unwrap();
            })
            .unwrap();
        };
        let state = |id: &'static str, cx: &mut gpui::AsyncApp| {
            handle
                .update(cx, |v, _, a| v.visual_pinned_ask(id, a))
                .unwrap()
        };
        let choose = |c: PinnedAsk, cx: &mut gpui::AsyncApp| {
            cx.update(|a| {
                a.set_global(c);
                a.refresh_windows();
            });
        };
        let settle = |cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh();
                w.draw(a).clear(a);
            })
            .unwrap();
        };
        // A: the bar on the Claude pane, none on the Codex one.
        choose(
            PinnedAsk {
                bar: true,
                float: false,
                button: false,
            },
            cx,
        );
        let mut claude = state("pc", cx);
        for _ in 0..30 {
            settle(cx);
            wait(200).await;
            claude = state("pc", cx);
            if claude.1.is_some() {
                break;
            }
        }
        eprintln!("claude pane: {claude:?}");
shot("a-bar.png", cx);
// Scrolled up inside the newest answer (rows counted from the "1" ask: its 80
// answer rows, the paste, then well into the newest answer): the ask stays pinned.
// Pictured before the checks, so a build that loses it still shows how.
let reply = claude.0.get(1).copied().flatten().expect("the reply is found");
handle
.update(cx, |v, _, a| v.visual_scroll_row("pc", reply + 100, a))
.unwrap();
wait(300).await;
settle(cx);
wait(300).await;
settle(cx);
shot("a-scrolled.png", cx);
let scrolled = state("pc", cx);
eprintln!("scrolled up: {scrolled:?}");
assert_eq!(claude.0.len(), asks.len(), "every ask looked for");
assert!(
claude.0.iter().all(Option::is_some),
"every ask found: {claude:?}"
);
assert_eq!(
claude.1,
Some((asks[2].to_owned(), true)),
"the newest ask pinned with a way back"
);
assert_eq!(
scrolled.1,
Some((asks[2].to_owned(), true)),
"still pinned scrolled up in its answer"
);
let codex = state("px", cx);
assert_eq!(codex.1, None, "Codex pins its own ask: {codex:?}");
        // The bar's way back puts the ask on screen, so the bar goes.
        handle
            .update(cx, |v, _, a| v.visual_back_to_ask("pc", a))
            .unwrap();
        wait(300).await;
        settle(cx);
        wait(300).await;
        settle(cx);
        let back = state("pc", cx);
        assert_eq!(back.1, None, "the ask on screen is not pinned: {back:?}");
        shot("a-back.png", cx);
        // B: reading the older turn floats its ask.
        choose(
            PinnedAsk {
                bar: false,
                float: true,
                button: false,
            },
            cx,
        );
        let older = claude.0[0].unwrap();
        handle
            .update(cx, |v, _, a| v.visual_scroll_row("pc", older + 12, a))
            .unwrap();
        wait(100).await;
        settle(cx);
        wait(100).await;
        settle(cx);
        let reading = state("pc", cx);
        assert_eq!(
            reading.2.as_deref(),
            Some(asks[0]),
            "the older turn's ask floats: {reading:?}"
        );
        shot("b-float.png", cx);
        // C: the header button instead, back at the live bottom.
        choose(
            PinnedAsk {
                bar: false,
                float: false,
                button: true,
            },
            cx,
        );
        handle
            .update(cx, |v, _, a| v.visual_scroll_row("pc", usize::MAX / 2, a))
            .unwrap();
        wait(300).await;
        settle(cx);
        wait(300).await;
        settle(cx);
        assert!(
            state("pc", cx).1.is_some(),
            "the newest ask is out of sight again"
        );
        shot("c-button.png", cx);
        eprintln!(
            "pinned ask evidence PASS: bar on Claude also under a paste and scrolled up, none on Codex, back, float, button"
        );
        cx.update(|a| a.quit());
    })
    .detach();
    true
}
