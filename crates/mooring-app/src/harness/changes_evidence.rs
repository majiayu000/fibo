use super::*;
use mooring_core::turn_diff::Disk;
use serde_json::json;

type T = crate::ui::theme::Theme;

// 看改动, on a real host reading a real transcript's shape, by real clicks and keys: every row
// of docs/turn-diff's state table that a window can show. The terminal runs `cat`, so what
// reaches it comes back on its screen.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    if std::env::var_os("MOORING_CHANGES_EVIDENCE").is_none() {
        return false;
    }
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        wait(1500).await;
        std::fs::create_dir_all(&directory).unwrap();
        let socket = handle.update(cx, |v, _, _| v.visual_socket()).unwrap();
        let root = socket.parent().unwrap().to_owned();
        let call = |r: Request| rpc(&socket, &r).unwrap();
        let w = match call(Request::Workspace) {
            Reply::Workspace(w) => w,
            x => panic!("{x:?}"),
        };
        let space = match w.spaces.first() {
            Some(s) => s.id.clone(),
            None => {
                call(Request::Edit { request: None, edit: Edit::AddSpace(Space { id: "s".into(), name: "Tests".into(), cwd: "/tmp".into() }) });
                "s".into()
            }
        };
        let logs = PathBuf::from(std::env::var_os("CLAUDE_CONFIG_DIR").expect("scratch CLAUDE_CONFIG_DIR")).join("projects/-tmp");
        std::fs::create_dir_all(&logs).unwrap();
        let claude = root.join("claude");
        let _ = std::os::unix::fs::symlink("/bin/sh", &claude);
        let work = root.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let body = |n: usize, changes: &[(usize, &str)]| -> String {
            (1..=n)
                .map(|i| match changes.iter().find(|(at, _)| *at == i) {
                    Some((_, text)) => format!("{text}\n"),
                    None => format!("fn line_{i}() {{}}\n"),
                })
                .collect()
        };
        let was = |i: usize| format!("fn line_{i}() {{}}");
        let at = mooring_core::content::now();
        let id = |n: u32| format!("{:08x}-0000-4000-8000-00000000000{n}", std::process::id());
        // A call and the result the CLI logged for it.
        let mut seq = 0;
        let mut logged = |sid: &str, path: &PathBuf, name: &str, input: serde_json::Value, result: serde_json::Value, when: u64| {
            seq += 1;
            let call = json!({"type":"assistant","sessionId":sid,"uuid":format!("c{seq}"),"cwd":"/tmp","timestamp":when,"message":{"role":"assistant","model":"claude-test","content":[{"type":"tool_use","id":format!("t{seq}"),"name":name,"input":input}]}});
            let mut full = result;
            full["filePath"] = json!(path);
            let done = json!({"type":"user","sessionId":sid,"uuid":format!("r{seq}"),"cwd":"/tmp","timestamp":when,"toolUseResult":full,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":format!("t{seq}"),"content":"ok"}]}});
            format!("{call}\n{done}\n")
        };
        let edit = |line: usize, old: &str, new: &str| -> serde_json::Value {
            json!({"originalFile":null,"structuredPatch":[{"oldStart":line,"oldLines":1,"newStart":line,"newLines":1,"lines":[format!("-{old}"),format!("+{new}")]}]})
        };
        let input = |path: &PathBuf, old: &str, new: &str| json!({"file_path":path,"old_string":old,"new_string":new});
        let open = |sid: &str, text: &str, when: u64| json!({"type":"user","sessionId":sid,"uuid":format!("u{when}"),"cwd":"/tmp","entrypoint":"cli","userType":"external","timestamp":when,"message":{"content":text}}).to_string() + "\n";
        let close = |sid: &str, when: u64| {
            let reply = json!({"type":"assistant","sessionId":sid,"uuid":format!("a{when}"),"cwd":"/tmp","timestamp":when,"message":{"role":"assistant","model":"claude-test","stop_reason":"end_turn","content":[{"type":"text","text":"改好了。"}]}});
            let end = json!({"type":"system","subtype":"turn_duration","sessionId":sid,"uuid":format!("s{when}"),"cwd":"/tmp","timestamp":when});
            format!("{reply}\n{end}\n")
        };

        // Session a: a line changed twice; a new file; a change undone; a file changed since.
        let (sid_a, sid_b) = (id(1), id(2));
        let twice = work.join("twice.rs");
        let fresh = work.join("page.html");
        let undone = work.join("undone.rs");
        let stale = work.join("stale.rs");
        let page: String = (1..=104).map(|i| format!("<p>第 {i} 行，这一行故意写得比较长，用来看面板里的长句子怎样折行，不会把别的东西挤走，也不会被截断。</p>\n")).collect();
        std::fs::write(&twice, body(40, &[(10, "fn second_try() {}")])).unwrap();
        std::fs::write(&fresh, &page).unwrap();
        std::fs::write(&undone, body(20, &[])).unwrap();
        std::fs::write(&stale, body(20, &[(5, "changed since")])).unwrap();
        let mut seed = open(&sid_a, "把这几个文件改好", at);
        seed += &logged(&sid_a, &twice, "Edit", input(&twice, &was(10), "fn first_try() {}"), edit(10, &was(10), "fn first_try() {}"), at);
        seed += &logged(&sid_a, &twice, "Edit", input(&twice, "fn first_try() {}", "fn second_try() {}"), edit(10, "fn first_try() {}", "fn second_try() {}"), at);
        seed += &logged(&sid_a, &fresh, "Write", json!({"file_path":fresh,"content":page}), json!({"type":"create","content":page,"structuredPatch":[],"originalFile":null}), at);
        seed += &logged(&sid_a, &undone, "Edit", input(&undone, &was(3), "oops"), edit(3, &was(3), "oops"), at);
        seed += &logged(&sid_a, &undone, "Edit", input(&undone, "oops", &was(3)), edit(3, "oops", &was(3)), at);
        seed += &logged(&sid_a, &stale, "Edit", input(&stale, &was(5), "fn five() {}"), edit(5, &was(5), "fn five() {}"), at);
        seed += &close(&sid_a, at);
        let other = work.join("other.rs");
        std::fs::write(&other, body(10, &[(2, "fn two() {}")])).unwrap();
        let mut seed_b = open(&sid_b, "再改一个", at);
        seed_b += &logged(&sid_b, &other, "Edit", input(&other, &was(2), "fn two() {}"), edit(2, &was(2), "fn two() {}"), at);
        seed_b += &close(&sid_b, at);

        let log_a = logs.join(format!("{sid_a}.jsonl"));
        let launch = |tid: &str, sid: &str, seed: &str, fill: bool| {
            let file = root.join(format!("seed-{tid}"));
            std::fs::write(&file, seed).unwrap();
            let fill = if fill { "i=0; while [ $i -lt 40 ]; do printf '%s\\n' \"Agent 说的第 $i 行\"; i=$((i+1)); done; " } else { "" };
            let script = format!("cat '{}' > '{}'; {fill}printf '%s\\n' '⏺ 改好了，都在这几个文件里。'; printf '%s\\n' '还有两件事等你定。'; cat; sleep 600", file.display(), logs.join(format!("{sid}.jsonl")).display());
            let argv = vec!["/bin/sh".into(), "-c".into(), format!("exec '{}' -c \"$0\" fixture --session-id {sid}", claude.display()), script];
            call(Request::Launch { colors: None, id: tid.into(), space: space.clone(), argv, cwd: "/tmp".into(), cols: 100, rows: 30, scheme: Default::default() });
            call(Request::Edit { request: None, edit: Edit::CreateTab { tab: Tab { id: format!("tab-{tid}"), space: space.clone(), default_task: None, title: tid.into(), layout: Some(Layout::Leaf(tid.into())) } } });
        };
        let screen = |tid: &str| match call(Request::ReadScreen { id: tid.into(), rows: 40 }) {
            Reply::Text(t) => t,
            x => panic!("{x:?}"),
        };
        let shot = |name: &str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh();
                w.draw(a).clear(a);
                w.render_to_image().unwrap().save(directory.join(name)).unwrap();
            })
            .unwrap();
        };
        let mut fails = 0;
        macro_rules! check {
            ($name:expr, $ok:expr) => {{
                let ok = $ok;
                eprintln!("{} {}", if ok { "PASS" } else { "FAIL" }, $name);
                if !ok {
                    fails += 1;
                }
            }};
        }
        for (tid, sid, seed, fill) in [("b", sid_b.as_str(), seed_b.as_str(), true), ("a", sid_a.as_str(), seed.as_str(), false)] {
            launch(tid, sid, seed, fill);
            wait(300).await;
            handle.update(cx, |v, _, a| v.visual_peek_show_tab(tid, a)).unwrap();
            let mut ready = false;
            for _ in 0..120 {
                wait(500).await;
                if handle.update(cx, |v, _, _| v.visual_completable(tid)).unwrap().1 {
                    ready = true;
                    break;
                }
            }
            assert!(ready, "{tid}: the finished session never came to review");
        }
        wait(400).await;
        macro_rules! state {
            ($tid:expr) => {
                handle.update(cx, |v, w, a| v.visual_changes($tid, w, a)).unwrap()
            };
        }
        macro_rules! pane_bounds {
            ($tid:expr) => {
                handle.update(cx, |v, _, a| v.visual_terminal_bounds($tid, a)).unwrap().expect("bounds")
            };
        }
        macro_rules! click {
            ($x:expr, $y:expr) => {{
                let at = gpui::point(px($x), px($y));
                cx.update_window(handle.into(), |_, w, a| native_click(w, a, at, gpui::MouseButton::Left)).unwrap();
                wait(250).await;
            }};
        }
        macro_rules! key {
            ($k:expr) => {{
                cx.update_window(handle.into(), |_, w, a| {
                    w.dispatch_keystroke(gpui::Keystroke::parse($k).unwrap(), a);
                })
                .unwrap();
                wait(150).await;
            }};
        }
        let grid = pane_bounds!("a");
        let (top, right, left) = (f32::from(grid.origin.y), f32::from(grid.origin.x + grid.size.width), f32::from(grid.origin.x));
        let right_edge = right + T::S14;
        let bar_y = top - T::S10 - T::TURN_BAR / 2.;
        let accept_x = right_edge - T::S10 - 30.;
        let button_x = right_edge - T::S10 - 62. - T::S8 - 26.;
        // Pane a has little on its screen, so its panel covers the lower part.
        let bottom = f32::from(grid.origin.y + grid.size.height) + T::S10;
        let panel_top = bottom - (bottom - (top - T::S10)) * 0.56;
        let title_y = panel_top + T::PANE_HEADER / 2.;
        let close_x = right_edge - T::S6 - 10.;
        let row_y = |i: usize| panel_top + T::PANE_HEADER + T::S4 + T::PANE_ACTION * (i as f32 + 0.5);
        let list_x = left + 40.;
        let term_x = left + 60.;
        let term_y = top + 20.;

        shot("0-closed.png", cx);
        check!("starts closed", !state!("a").open);
        click!(button_x, bar_y);
        wait(500).await;
        let s = state!("a");
        check!("clicking 看改动 opens it with the keyboard", s.open && s.focused);
        check!("the terminal keeps its size", pane_bounds!("a") == grid);
        check!("an input line high up keeps showing: it covers the lower part", s.below);
        check!("lists all four files", s.files.len() == 4);
        let by = |s: &workbench::ChangesProbe, name: &str| s.files.iter().position(|f| f.path.ends_with(name)).unwrap();
        let f = &s.files[by(&s, "twice.rs")];
        check!("a line changed twice shows once", (f.added, f.removed, f.edits, f.one_by_one) == (1, 1, 2, false));
        check!("its text is the last one", f.hunks[0].lines.iter().any(|l| l.text.contains("second_try")) && !f.hunks[0].lines.iter().any(|l| l.text.contains("first_try")));
        check!("its disk agrees", f.disk == Disk::Same);
        let f = &s.files[by(&s, "page.html")];
        check!("a new file is all new", f.created && f.added == 104 && f.removed == 0 && f.disk == Disk::Same);
        let f = &s.files[by(&s, "undone.rs")];
        check!("a change undone adds to nothing", f.hunks.is_empty() && f.edits == 2 && f.added == 0);
        let f = &s.files[by(&s, "stale.rs")];
        check!("a file changed since says so", f.disk == Disk::Changed);
        for (i, file) in s.files.iter().enumerate() {
            while state!("a").selected != i {
                key!("tab");
            }
            let name = file.path.rsplit('/').next().unwrap().to_owned();
            shot(&format!("1-open-{name}.png"), cx);
        }
        while state!("a").selected != 0 {
            key!("tab");
        }
        key!("tab");
        check!("tab goes to the next file", state!("a").selected == 1);
        key!("shift-tab");
        check!("shift-tab goes back", state!("a").selected == 0);
        key!("shift-tab");
        check!("and wraps", state!("a").selected == 3);
        click!(list_x, row_y(1));
        check!("clicking a row selects it, keyboard stays", state!("a").selected == 1 && state!("a").focused);
        // Scroll the long file; each file keeps its own place, closed or not.
        let long = by(&s, "page.html");
        while state!("a").selected != long {
            key!("tab");
        }
        key!("down");
        key!("down");
        key!("down");
        let place = state!("a").scroll;
        check!("arrows scroll", place != (0, 0.));
        key!("pagedown");
        check!("page down scrolls further", state!("a").scroll != place);
        key!("pageup");
        let place = state!("a").scroll;
        key!("tab");
        key!("shift-tab");
        check!("a file keeps its place", state!("a").scroll == place && state!("a").selected == long);
        let before = screen("a");
        key!("enter");
        wait(300).await;
        check!("enter in the panel sends nothing", state!("a").open && screen("a") == before);
        key!("escape");
        let s = state!("a");
        check!("esc closes it, keyboard back to the terminal", !s.open && handle.update(cx, |v, w, a| v.visual_pane_focused(w, a)).unwrap());
        check!("esc reached the panel, not the agent", !screen("a").contains("^["));
        click!(button_x, bar_y);
        let s = state!("a");
        check!("reopening returns to the same file and place", s.open && s.selected == long && s.scroll == place);
        click!(button_x, bar_y);
        check!("clicking 看改动 again closes it", !state!("a").open);
        click!(button_x, bar_y);
        click!(close_x, title_y);
        check!("the ✕ closes it", !state!("a").open);
        click!(button_x, bar_y);
        key!("h");
        key!("i");
        wait(300).await;
        let s = state!("a");
        check!("typing leaves it open, keyboard on the terminal", s.open && !s.focused);
        check!("what was typed reached the terminal, first letter too", screen("a").contains("hi"));
        shot("2-typed.png", cx);
        key!("escape");
        wait(300).await;
        check!("esc on the terminal goes to the agent", state!("a").open && screen("a").contains("^["));
        key!("enter");
        click!(left + 200., row_y(2));
        check!("clicking the panel takes the keyboard", state!("a").focused);
        click!(term_x, term_y);
        let s = state!("a");
        check!("clicking the terminal keeps it open, keyboard there", s.open && !s.focused);
        click!(left + 200., row_y(2));
        click!(2., f32::from(grid.origin.y) + 300.);
        let s = state!("a");
        check!("clicking outside the pane changes nothing, as for a terminal", s.open && s.focused);
        // Another pane has its own; going away and back keeps this one.
        handle.update(cx, |v, _, a| v.visual_peek_show_tab("b", a)).unwrap();
        wait(400).await;
        let other_grid = pane_bounds!("b");
        check!("the other pane's panel starts closed", !state!("b").open);
        let other_right = f32::from(other_grid.origin.x + other_grid.size.width) + T::S14;
        click!(other_right - T::S10 - 62. - T::S8 - 26., f32::from(other_grid.origin.y) - T::S10 - T::TURN_BAR / 2.);
        wait(400).await;
        check!("panes open theirs separately", state!("b").open && state!("a").open);
        check!("an input line low down keeps showing: it covers the upper part", !state!("b").below);
        shot("3-other-pane.png", cx);
        handle.update(cx, |v, _, a| v.visual_peek_show_tab("a", a)).unwrap();
        wait(400).await;
        let s = state!("a");
        check!("coming back it is still open, same place", s.open && s.selected == long);
        check!("both stayed open", state!("b").open);
        // A new prompt: the turn bar goes and the panel with it.
        click!(left + 200., row_y(2));
        use std::io::Write as _;
        let later = at + 2;
        let append = |text: &str| {
            let mut file = std::fs::OpenOptions::new().append(true).open(&log_a).unwrap();
            file.write_all(text.as_bytes()).unwrap();
        };
        append(&open(&sid_a, "接着改", later));
        let mut closed = false;
        for _ in 0..40 {
            wait(250).await;
            if !state!("a").open {
                closed = true;
                break;
            }
        }
        check!("a new turn closes it", closed);
        check!("and the keyboard goes back to the terminal", handle.update(cx, |v, w, a| v.visual_pane_focused(w, a)).unwrap());
        shot("4-running.png", cx);
        // It ends with another edit: the bar returns, closed, and shows only that turn.
        let newer = work.join("newer.rs");
        std::fs::write(&newer, body(5, &[(1, "fn one() {}")])).unwrap();
        append(&logged(&sid_a, &newer, "Edit", input(&newer, &was(1), "fn one() {}"), edit(1, &was(1), "fn one() {}"), later));
        append(&close(&sid_a, later));
        let mut back = false;
        for _ in 0..40 {
            wait(250).await;
            if handle.update(cx, |v, _, _| v.visual_completable("a")).unwrap().1 {
                back = true;
                break;
            }
        }
        check!("the turn ends, review again", back);
        wait(600).await;
        check!("still closed", !state!("a").open);
        click!(button_x, bar_y);
        wait(500).await;
        let s = state!("a");
        check!("it opens on the new turn alone", s.open && s.files.len() == 1 && s.files[0].path.ends_with("newer.rs"));
        shot("5-new-turn.png", cx);
        click!(accept_x, bar_y);
        wait(300).await;
        check!("标记完成 closes it", !state!("a").open);
        eprintln!("changes evidence: {fails} failed");
        std::process::exit(if fails == 0 { 0 } else { 1 });
    })
    .detach();
    true
}
