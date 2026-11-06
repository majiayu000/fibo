//! MOORING_TODO_DEMO_EVIDENCE, run by scripts/todo-demo.py: a todo written on the board, a
//! space made on the way, how to start asked on its card, the session bound to it, and
//! the todo staying through all of that until you close it.
use super::*;
use gpui::WindowHandle;
use std::path::Path;
use std::time::Duration;
use workbench::Workbench;

pub(super) fn start(handle: WindowHandle<Workbench>, directory: &Path, cx: &mut App) -> bool {
    if std::env::var_os("MOORING_TODO_DEMO_EVIDENCE").is_none() {
        return false;
    }
    let directory = directory.to_owned();
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        wait(1500).await;
        std::fs::create_dir_all(&directory).unwrap();
        let socket = handle.update(cx, |v, _, _| v.visual_socket()).unwrap();
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
        macro_rules! step {
            ($s:expr, $a:expr) => {{
                let out = handle
                    .update(cx, |v, w, c| v.visual_todo($s, $a, w, c))
                    .unwrap();
                wait(500).await;
                let out = if out.is_empty() {
                    out
                } else {
                    handle
                        .update(cx, |v, w, c| v.visual_todo("unhover", "-", w, c))
                        .unwrap()
                };
                eprintln!("{}({}) -> {out}", $s, $a);
                out
            }};
        }
        let mut fails = 0;
        let mut check = |name: &str, ok: bool, got: &str| {
            eprintln!("{} {name}", if ok { "PASS" } else { "FAIL" });
            if !ok {
                eprintln!("  got {got}");
                fails += 1;
            }
        };

        step!("board", "");
        shot("1-board", cx);
        // The card's whole table: each way in and out, with real keys and clicks.
        macro_rules! frame {
            () => {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                })
                .unwrap()
            };
        }
        macro_rules! report {
            () => {{
                wait(250).await;
                frame!();
                step!("unhover", "-")
            }};
        }
        macro_rules! key {
            ($k:expr) => {{
                frame!();
                cx.update_window(handle.into(), |_, w, a| {
                    w.dispatch_keystroke(gpui::Keystroke::parse($k).unwrap(), a);
                })
                .unwrap();
                report!()
            }};
        }
        macro_rules! click {
            ($x:expr, $y:expr) => {{
                frame!();
                cx.update_window(handle.into(), |_, w, a| {
                    native_click(w, a, gpui::point(px($x), px($y)), gpui::MouseButton::Left)
                })
                .unwrap();
                report!()
            }};
        }
        let row = |s: &str, all: &[&str]| all.iter().all(|want| s.contains(want));
        frame!();
        let s = key!("n");
        check(
            "N opens the card, keyboard in it",
            row(&s, &["quick=true", "focused=true"]),
            &s,
        );
        // The simulated key also types its own letter, which a real key press does not.
        step!("type", "");
        let s = key!("enter");
        check(
            "Enter with no title does nothing",
            row(&s, &["quick=true", "focused=true", "tabs=16"]) && !s.contains("fresh=true"),
            &s,
        );
        step!("type", "第一条");
        let s = key!("enter");
        check(
            "Enter saves, closes, keyboard on the board, new card first",
            row(
                &s,
                &[
                    "quick=false",
                    "board_focus=true",
                    "fresh=true",
                    "draft=\"\"",
                    "todos=[\"第一条:0\"",
                ],
            ),
            &s,
        );
        shot("0-after-enter-start", cx);
        wait(350).await;
        shot("0-after-enter", cx);
        let s = key!("n");
        check(
            "N again right after, nothing left over",
            row(&s, &["quick=true", "focused=true"]),
            &s,
        );
        step!("type", "半句");
        let s = key!("escape");
        check(
            "Esc closes, words kept, keyboard on the board",
            row(
                &s,
                &[
                    "quick=false",
                    "board_focus=true",
                    "draft=\"半句\"",
                    "overview=true",
                ],
            ),
            &s,
        );
        let s = key!("n");
        check("N brings the words back", s.contains("半句"), &s);
        step!("type", "半句");
        let s = click!(600., 760.);
        check(
            "a click on the board closes it, words kept",
            row(&s, &["quick=false", "draft=\"半句\""]),
            &s,
        );
        key!("n");
        step!("type", "半句");
        let s = click!(120., 730.);
        check(
            "a click in the sidebar closes it too",
            row(&s, &["quick=false", "draft=\"半句\""]),
            &s,
        );
        key!("n");
        step!("type", "");
        let s = click!(600., 760.);
        check(
            "an empty card closes the same way",
            row(&s, &["quick=false", "draft=\"\""]),
            &s,
        );
        key!("n");
        step!("type", "");
        frame!();
        let s = click!(350., 320.);
        check(
            "a click on a card closes this and opens that",
            row(&s, &["quick=false", "detail=Some"]),
            &s,
        );
        step!("closedetail", "");
        frame!();
        key!("n");
        step!("type", "走开再回来");
        step!("leave", "");
        let s = report!();
        check(
            "leaving 鸟瞰 closes it, words kept",
            row(
                &s,
                &["overview=false", "quick=false", "draft=\"走开再回来\""],
            ),
            &s,
        );
        step!("board", "");
        let s = report!();
        check(
            "back on the board it is closed",
            row(&s, &["overview=true", "quick=false"]),
            &s,
        );
        let s = key!("n");
        check(
            "N there has the keyboard in it",
            row(&s, &["quick=true", "focused=true"]),
            &s,
        );
        step!("type", "别的空间的");
        step!("spacemenu", "");
        frame!();
        let s = key!("escape");
        check(
            "Esc folds the space list first",
            row(&s, &["quick=true", "choosing=false", "focused=true"]),
            &s,
        );
        step!("space", "s2");
        let s = key!("escape");
        check("then closes", s.contains("quick=false"), &s);
        let s = key!("n");
        check(
            "the words come back with their space",
            s.contains(r#"space=Some("her")"#),
            &s,
        );
        step!("newspace", "");
        frame!();
        let s = key!("escape");
        check(
            "Esc drops a new space's name, back to the title",
            row(&s, &["quick=true", "naming=false", "focused=true"]),
            &s,
        );
        step!("type", "空间被筛掉也能开始");
        step!("filter", "s1");
        frame!();
        let s = key!("cmd-enter");
        check(
            "Cmd-Enter saves, closes, asks on a card the filter can't hide",
            row(
                &s,
                &[
                    "quick=false",
                    "board_focus=true",
                    "picking=Some",
                    "filter=None",
                    "overview=true",
                ],
            ),
            &s,
        );
        shot("0-picker", cx);
        let s = key!("escape");
        check(
            "Esc closes the question, 鸟瞰 stays",
            row(&s, &["picking=None", "overview=true"]),
            &s,
        );
        step!("start", "t1");
        let s = click!(600., 760.);
        check(
            "a click elsewhere closes the question",
            s.contains("picking=None"),
            &s,
        );
        step!("start", "t1");
        let s = key!("n");
        check(
            "N closes the question and opens the card",
            row(&s, &["picking=None", "quick=true"]),
            &s,
        );
        step!("type", "");
        key!("escape");
        step!("start", "t1");
        step!("leave", "");
        let s = report!();
        check(
            "leaving 鸟瞰 closes the question",
            s.contains("picking=None"),
            &s,
        );
        step!("board", "");
        frame!();
        step!("start", "t1");
        step!("done", "t1");
        let s = report!();
        check(
            "完成 closes the question on that todo",
            s.contains("picking=None"),
            &s,
        );
        step!("undo", "t1");
        step!("detail", "t0");
        frame!();
        step!("closedetail", "");
        frame!();
        // An input method composing owns Enter and Esc; what it holds isn't written yet.
        key!("n");
        step!("type", "");
        // What reaches the app while composing, sent as it is: no letter comes with it.
        macro_rules! raw_key {
            ($k:expr) => {{
                frame!();
                step!("compose", "ni hao");
                cx.update_window(handle.into(), |_, w, a| {
                    use gpui::InputEvent;
                    let keystroke = gpui::Keystroke::parse($k).unwrap();
                    w.dispatch_event(
                        gpui::KeyDownEvent {
                            keystroke,
                            is_held: false,
                            prefer_character_input: false,
                        }
                        .to_platform_input(),
                        a,
                    );
                })
                .unwrap();
                report!()
            }};
        }
        let s = raw_key!("enter");
        check(
            "composing, Enter does not save",
            row(&s, &["quick=true"]) && !s.contains("ni hao:0"),
            &s,
        );
        let s = raw_key!("cmd-enter");
        check(
            "composing, Cmd-Enter does not save",
            row(&s, &["quick=true", "picking=None"]) && !s.contains("ni hao:0"),
            &s,
        );
        let s = raw_key!("escape");
        check(
            "composing, Esc leaves the card and 鸟瞰 alone",
            row(&s, &["quick=true", "overview=true"]),
            &s,
        );
        step!("commit", "你好");
        let s = key!("enter");
        check(
            "composed, Enter saves the words",
            row(&s, &["quick=false"]) && s.contains("你好:0"),
            &s,
        );
        let s = step!("new", "");
        check(
            "the card opens on the board",
            s.contains("overview=true") && s.contains("quick=true") && s.contains("focused=true"),
            &s,
        );
        shot("2-card", cx);
        step!("type", "给每日账本加导出");
        let s = step!("enter", "");
        check(
            "Enter saves and closes",
            s.contains("给每日账本加导出:0")
                && s.contains("quick=false")
                && s.contains("fresh=true"),
            &s,
        );
        check(
            "the newest todo is first",
            s.contains(r#"todos=["给每日账本加导出:0""#),
            &s,
        );
        shot("3-saved", cx);
        wait(1700).await;
        let s = step!("unhover", "-");
        check(
            "the mark goes after a moment",
            s.contains("fresh=false"),
            &s,
        );
        shot("3b-settled", cx);
        for n in 0..12 {
            step!("new", "");
            step!("type", &format!("填满这一列 {n}"));
            step!("enter", "");
        }
        wait(1800).await;
        step!("scrolltodos", "6");
        shot("3c-scrolled", cx);
        step!("new", "");
        step!("type", "滚动之后新记的");
        step!("enter", "");
        shot("3d-revealed", cx);
        let s = step!("unhover", "-");
        check(
            "a new todo brings the list back to its top",
            s.contains("top=0"),
            &s,
        );
        wait(120).await;
        shot("3e-rising", cx);
        wait(300).await;
        shot("3f-outlined", cx);
        wait(1800).await;
        // Written to a space the filter hides: a note says where it went.
        step!("filter", "s1");
        step!("new", "");
        step!("space", "s0");
        step!("type", "记到别的空间");
        let s = step!("enter", "");
        check(
            "filtered away, a note says where the todo went",
            s.contains(r#"toast=Some("「记到别的空间」记到了「mooring」"#),
            &s,
        );
        shot("3g-filtered0", cx);
        wait(500).await;
        shot("3g-filtered", cx);
        step!("filter", "");
        step!("new", "");
        step!("type", "半句");
        let s = step!("esc", "");
        check(
            "Esc folds it on the board, words kept",
            s.contains("overview=true")
                && s.contains("quick=false")
                && s.contains(r#"draft="半句""#),
            &s,
        );
        shot("4-draft", cx);
        let s = step!("new", "");
        check("they come back", s.contains(r#"text=Some("半句")"#), &s);
        step!("spacemenu", "");
        shot("5-spaces", cx);
        step!("newspace", "");
        shot("6-newspace", cx);
        let s = step!("savespace", "家里的事");
        check(
            "a new space, title kept and focused",
            s.contains(r#"space=Some("家里的事")"#)
                && s.contains(r#"text=Some("半句")"#)
                && s.contains("focused=true"),
            &s,
        );
        step!("type", "接新的活");
        let s = step!("cmdenter", "");
        check(
            "cmd-enter saves and asks how to start, still on the board",
            s.contains("overview=true")
                && s.contains("quick=false")
                && s.contains("picking=Some")
                && s.contains("接新的活:0"),
            &s,
        );
        shot("7-picker", cx);
        let before = match rpc(&socket, &Request::Workspace).unwrap() {
            Reply::Workspace(w) => w.terminals.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
            x => panic!("{x:?}"),
        };
        let s = step!("pick", "");
        wait(1500).await;
        check(
            "choosing leaves for the terminal, the todo stays",
            s.contains("overview=false") && s.contains("接新的活:0"),
            &s,
        );
        shot("8-terminal", cx);
        wait(3000).await;
        shot("8b-agent", cx);
        let w = match rpc(&socket, &Request::Workspace).unwrap() {
            Reply::Workspace(w) => w,
            x => panic!("{x:?}"),
        };
        let terminal = (w.terminals.iter())
            .find(|t| !before.contains(&t.id))
            .expect("a new terminal");
        // You press Enter in the Agent's input box.
        rpc(
            &socket,
            &Request::SendInput {
                id: terminal.id.clone(),
                bytes: b"\r".to_vec(),
            },
        )
        .unwrap();
        wait(2500).await;
        shot("8c-running", cx);
        let s = step!("board", "");
        check(
            "the todo stays with its session",
            s.contains("接新的活:1"),
            &s,
        );
        shot("9-linked", cx);
        // 完成 with its Agent working: the terminal stays, with a note.
        let tabs = |s: &str| {
            s.split("tabs=")
                .nth(1)
                .and_then(|r| r.split(' ').next())
                .unwrap_or("")
                .to_owned()
        };
        let working = tabs(&s);
        let started = (mooring_core::status::unfinished_todos(
            &match rpc(&socket, &Request::Workspace).unwrap() {
                Reply::Workspace(w) => w,
                x => panic!("{x:?}"),
            },
            0,
        )
        .iter())
        .find(|t| t.title == "接新的活")
        .map(|t| t.id.clone())
        .expect("the todo");
        let s = step!("done", &started);
        wait(800).await;
        check(
            "a working Agent keeps its terminal",
            tabs(&s) == working,
            &s,
        );
        shot("9b-kept0", cx);
        wait(500).await;
        shot("9b-kept", cx);
        step!("undo", &started);
        // Its turn over, 完成 closes the terminal the todo left open.
        wait(21000).await;
        step!("done", &started);
        wait(1200).await;
        let s = step!("unhover", "-");
        let left = tabs(&s);
        check(
            "a stopped Agent's terminal closes with the todo",
            left.parse::<usize>().unwrap() + 1 == working.parse::<usize>().unwrap(),
            &s,
        );
        shot("9c-closed0", cx);
        wait(500).await;
        shot("9c-closed1", cx);
        wait(1000).await;
        shot("9c-closed", cx);
        step!("drawertask", "t0");
        wait(700).await;
        shot("9d-drawer", cx);
        let s = click!(840., 146.);
        check(
            "the drawer's link opens the todo",
            s.contains("drawer=false") && s.contains("detail=Some(\"t0\")"),
            &s,
        );
        step!("closedetail", "");
        step!("hover", "t0");
        handle
            .update(cx, |v, w, c| v.visual_todo("hover", "t0", w, c))
            .unwrap();
        shot("10-hover", cx);
        let s = step!("detail", "t0");
        check(
            "a todo opens inside 鸟瞰",
            s.contains("overview=true") && s.contains("detail=Some"),
            &s,
        );
        shot("11-detail", cx);
        step!("closedetail", "");
        let s = step!("done", "t2");
        check(
            "done with no session leaves the unfinished",
            !s.contains("周五前交报销单"),
            &s,
        );
        step!("done", "t0");
        wait(500).await;
        shot("12-done", cx);
        let s = step!("undo", "t0");
        check("undo brings it back", s.contains("键盘操作补齐:2"), &s);
        step!("leave", "");
        let s = step!("new", "");
        check(
            "in the workspace ＋ stays in the column",
            s.contains("overview=false") && s.contains("quick=false"),
            &s,
        );
        wait(300).await;
        shot("13-side-quick", cx);
        // Hundreds of random steps, and after each what must always hold: words are never
        // lost, an open card has the keyboard, a closed one leaves it on the board.
        let mut seed: u64 = std::env::var("MOORING_TODO_FUZZ_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20261106);
        eprintln!("fuzz seed {seed}");
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        step!("board", "");
        frame!();
        let mut trail: Vec<String> = vec![];
        let mut written: Vec<String> = vec![];
        let mut broken = 0;
        for i in 0..400 {
            let event = next(16);
            let word = format!("随机{i}");
            let name = match event {
                0 | 1 => {
                    cx.update_window(handle.into(), |_, w, a| {
                        w.dispatch_keystroke(gpui::Keystroke::parse("n").unwrap(), a);
                    })
                    .unwrap();
                    "key n".to_owned()
                }
                2 | 3 => {
                    cx.update_window(handle.into(), |_, w, a| {
                        w.dispatch_keystroke(gpui::Keystroke::parse("enter").unwrap(), a);
                    })
                    .unwrap();
                    "key enter".to_owned()
                }
                4 => {
                    cx.update_window(handle.into(), |_, w, a| {
                        w.dispatch_keystroke(gpui::Keystroke::parse("cmd-enter").unwrap(), a);
                    })
                    .unwrap();
                    "key cmd-enter".to_owned()
                }
                5 | 6 => {
                    cx.update_window(handle.into(), |_, w, a| {
                        w.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), a);
                    })
                    .unwrap();
                    "key escape".to_owned()
                }
                7 | 8 => {
                    let s = handle
                        .update(cx, |v, w, c| v.visual_todo("unhover", "-", w, c))
                        .unwrap();
                    if s.contains("quick=true") && s.contains("naming=false") {
                        handle
                            .update(cx, |v, w, c| v.visual_todo("type", &word, w, c))
                            .unwrap();
                        // Typing over the last words is you changing them, not a loss.
                        written = vec![word.clone()];
                    }
                    format!("type {word}")
                }
                9 => {
                    cx.update_window(handle.into(), |_, w, a| {
                        native_click(
                            w,
                            a,
                            gpui::point(px(600.), px(760.)),
                            gpui::MouseButton::Left,
                        )
                    })
                    .unwrap();
                    "click board".to_owned()
                }
                10 => {
                    cx.update_window(handle.into(), |_, w, a| {
                        native_click(
                            w,
                            a,
                            gpui::point(px(120.), px(730.)),
                            gpui::MouseButton::Left,
                        )
                    })
                    .unwrap();
                    "click sidebar".to_owned()
                }
                11 => {
                    cx.update_window(handle.into(), |_, w, a| {
                        native_click(
                            w,
                            a,
                            gpui::point(px(350.), px(320.)),
                            gpui::MouseButton::Left,
                        )
                    })
                    .unwrap();
                    "click column".to_owned()
                }
                12 => {
                    handle
                        .update(cx, |v, w, c| v.visual_todo("leave", "", w, c))
                        .unwrap();
                    "leave".to_owned()
                }
                13 => {
                    handle
                        .update(cx, |v, w, c| v.visual_todo("board", "", w, c))
                        .unwrap();
                    "board".to_owned()
                }
                14 => {
                    let s = handle
                        .update(cx, |v, w, c| v.visual_todo("unhover", "-", w, c))
                        .unwrap();
                    if s.contains("overview=true") && s.contains("detail=None") {
                        handle
                            .update(cx, |v, w, c| v.visual_todo("start", "t1", w, c))
                            .unwrap();
                    }
                    "ask t1".to_owned()
                }
                _ => {
                    handle
                        .update(cx, |v, w, c| v.visual_todo("closedetail", "", w, c))
                        .unwrap();
                    "close detail".to_owned()
                }
            };
            trail.push(name);
            wait(40).await;
            frame!();
            wait(40).await;
            frame!();
            let s = handle
                .update(cx, |v, w, c| v.visual_todo("unhover", "-", w, c))
                .unwrap();
            let has = |want: &str| s.contains(want);
            let mut fail = vec![];
            if has("overview=false") && has("quick=true") {
                fail.push("left 鸟瞰 with the card open");
            }
            if has("quick=true") && has("naming=false") && has("focused=false") {
                fail.push("the card is open without the keyboard");
            }
            if has("overview=true")
                && has("detail=None")
                && has("quick=false")
                && !has("board_focus=true")
            {
                fail.push("the card is closed and N has no keyboard");
            }
            if has("overview=false") && has("picking=Some") {
                fail.push("a board question outlived 鸟瞰");
            }
            if let Some(w) = written.iter().find(|w| !s.contains(w.as_str())) {
                eprintln!("  lost {w}");
                fail.push("words were lost");
            }
            if !fail.is_empty() && broken < 6 {
                broken += 1;
                eprintln!("FUZZ step {i}: {fail:?}");
                eprintln!("  trail {:?}", &trail[trail.len().saturating_sub(6)..]);
                eprintln!("  state {}", s.chars().take(200).collect::<String>());
            }
        }
        check(
            "400 random steps never broke what must hold",
            broken == 0,
            &format!("{broken} broke"),
        );
        eprintln!("todo demo evidence fails={fails}");
        cx.update(|a| a.quit());
    })
    .detach();
    true
}
