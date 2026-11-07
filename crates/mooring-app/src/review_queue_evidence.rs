//! MOORING_REVIEW_QUEUE_EVIDENCE=1, run by scripts/todo-demo.py: 逐个验收's table of states
//! with real keys and a real click, each checked against the host's copy of the workspace.
//! Simulated agents, no model, its own host and HOME.
use super::*;
use gpui::WindowHandle;
use std::path::Path;
use std::time::Duration;
use workbench::Workbench;

pub(super) fn start(handle: WindowHandle<Workbench>, directory: &Path, cx: &mut App) -> bool {
    if std::env::var_os("MOORING_REVIEW_QUEUE_EVIDENCE").is_none() {
        return false;
    }
    let directory = directory.to_owned();
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        wait(2500).await;
        std::fs::create_dir_all(&directory).unwrap();
        let mut fails = 0;
        macro_rules! frame {
            () => {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                })
                .unwrap()
            };
        }
        macro_rules! shot {
            ($name:expr) => {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                    w.render_to_image()
                        .unwrap()
                        .save(directory.join(format!("{}.png", $name)))
                        .unwrap();
                })
                .unwrap()
            };
        }
        macro_rules! state {
            ($step:expr) => {{
                handle
                    .update(cx, |v, _, c| v.visual_review($step, c))
                    .unwrap();
                wait(700).await;
                frame!();
                handle.update(cx, |v, _, c| v.visual_review("", c)).unwrap()
            }};
        }
        macro_rules! key {
            ($k:expr) => {{
                frame!();
                cx.update_window(handle.into(), |_, w, a| {
                    w.dispatch_keystroke(gpui::Keystroke::parse($k).unwrap(), a);
                })
                .unwrap();
                state!("")
            }};
        }
        macro_rules! check {
            ($name:expr, $ok:expr, $got:expr) => {{
                let ok: bool = $ok;
                eprintln!("{} {}", if ok { "PASS" } else { "FAIL" }, $name);
                if !ok {
                    eprintln!("  got {}", $got);
                    fails += 1;
                }
            }};
        }
        let has = |s: &str, title: &str, want: &str| {
            s.split('"')
                .any(|row| row.starts_with(&format!("{title}|")) && row.contains(want))
        };

        let s = state!("board");
        shot!("1-board");
        check!("the board offers 逐个验收", s.contains("button=true"), s);
        frame!();
        cx.update_window(handle.into(), |_, w, a| {
            native_click(w, a, gpui::point(px(934.), px(129.)), gpui::MouseButton::Left)
        })
        .unwrap();
        let s = state!("");
        shot!("2-queue");
        check!(
            "a real click on the button queues only the finished turns: 6, no question",
            s.contains("queue=6") && s.contains("review=true") && s.contains("index=0"),
            s
        );
        let first = s.split("current=Some(\"").nth(1).unwrap().split('"').next().unwrap().to_owned();
        let s = key!("enter");
        check!(
            "Enter marks it, moves on and closes its stopped terminal",
            s.contains("index=1")
                && has(&s, &first, "Completed")
                && has(&s, &first, "live=false"),
            s
        );
        let second = s.split("current=Some(\"").nth(1).unwrap().split('"').next().unwrap().to_owned();
        let s = key!("backspace");
        check!(
            "Backspace abandons, moves on, leaves the terminal",
            s.contains("index=2")
                && has(&s, &second, "Abandoned")
                && has(&s, &second, "live=true"),
            s
        );
        let third = s.split("current=Some(\"").nth(1).unwrap().split('"').next().unwrap().to_owned();
        let s = key!("l");
        check!("L asks when to look again", s.contains("defer_form=true"), s);
        let s = key!("escape");
        check!(
            "Esc in the form leaves the queue where it was",
            s.contains("index=2")
                && s.contains("defer_form=false")
                && s.contains("queue=6")
                && s.contains("overview=true"),
            s
        );
        key!("l");
        frame!();
        cx.update_window(handle.into(), |_, w, a| {
            native_click(w, a, gpui::point(px(436.), px(292.)), gpui::MouseButton::Left)
        })
        .unwrap();
        let s = state!("");
        check!(
            "取消 in the form does the same",
            s.contains("index=2") && s.contains("defer_form=false") && s.contains("overview=true"),
            s
        );
        // A turn starts behind the card: the host now reads 在跑.
        let log = handle.update(cx, |v, _, _| v.visual_review_log()).unwrap().unwrap();
        let mut text = std::fs::read_to_string(&log).unwrap();
        text.push_str(&format!(
            "{{\"timestamp\":{},\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\",\"turn_id\":\"evidence\"}}}}\n",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
        ));
        std::fs::write(&log, text).unwrap();
        let mut s = String::new();
        for _ in 0..40 {
            s = state!("");
            if s.contains("blocked=Some(\"running\")") {
                break;
            }
        }
        shot!("3-blocked");
        check!("a card that started a turn says so", s.contains("blocked=Some(\"running\")"), s);
        for k in ["enter", "backspace", "l"] {
            s = key!(k);
        }
        check!(
            "no key marks a card whose turn is no longer finished",
            s.contains("index=2") && has(&s, &third, "Open") && has(&s, &third, "live=true")
                && s.contains("defer_form=false"),
            s
        );
        let s = state!("skip");
        check!("skip moves past it unmarked", s.contains("index=3") && has(&s, &third, "Open"), s);
        let s = key!("escape");
        check!(
            "Esc goes back to the board and closes nothing more",
            s.contains("overview=true") && s.contains("queue=0") && has(&s, &third, "live=true"),
            s
        );
        let s = state!("start");
        check!(
            "the queue again holds what is left: the 3 not reached",
            s.contains("queue=3") && s.contains("index=0"),
            s
        );
        let s = key!("c");
        check!(
            "C opens the conversation and leaves the queue",
            s.contains("overview=false") && s.contains("queue=0"),
            s
        );
        state!("board");
        let s = state!("start");
        let fourth = s.split("current=Some(\"").nth(1).unwrap().split('"').next().unwrap().to_owned();
        key!("l");
        let s = state!("defersave");
        check!(
            "saving the form puts it off, moves on, and the keys work again",
            s.contains("index=1") && s.contains("overview=true") && has(&s, &fourth, "Deferred"),
            s
        );
        let mut last = String::new();
        for _ in 0..2 {
            last = key!("enter");
        }
        shot!("4-finished");
        check!(
            "the last one leaves 这一轮已处理完 and no button on the board",
            last.contains("index=3") && last.contains("current=None") && last.contains("button=false"),
            last
        );
        let s = key!("escape");
        for title in ["推送前确认 PR 标题", "确认迁移脚本权限"] {
            check!(
                format!("{title}: a question is never touched"),
                has(&s, title, "Open") && has(&s, title, "waiting") && has(&s, title, "live=true"),
                s
            );
        }
        eprintln!("review queue evidence fails={fails}");
        cx.update(|a| a.quit());
    })
    .detach();
    true
}
