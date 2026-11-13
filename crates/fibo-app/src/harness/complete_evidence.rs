use super::*;

// 标记完成: a finished session's pane plays the fill, seal and countdown, then closes;
// 「留着这个画面」 mid-run keeps another pane open.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_COMPLETE_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
            let executor = cx.background_executor().clone();
            let wait = move |ms| executor.timer(Duration::from_millis(ms));
            wait(1500).await;
            std::fs::create_dir_all(&directory).unwrap();
            let socket = handle.update(cx, |v, _, _| v.visual_socket()).unwrap();
            let root = socket.parent().unwrap().to_owned();
            let call = |r: Request| rpc(&socket, &r).unwrap();
            let workspace = || match call(Request::Workspace) { Reply::Workspace(w) => w, x => panic!("{x:?}") };
            let w = workspace();
            let space = match w.spaces.first() {
                Some(s) => s.id.clone(),
                None => {
                    call(Request::Edit { request: None, edit: Edit::AddSpace(Space { id: "s".into(), name: "Tests".into(), cwd: "/tmp".into() }) });
                    "s".into()
                }
            };
            // Claude stand-ins whose transcripts end with a finished turn.
            let logs = PathBuf::from(std::env::var_os("CLAUDE_CONFIG_DIR").expect("scratch CLAUDE_CONFIG_DIR")).join("projects/-tmp");
            std::fs::create_dir_all(&logs).unwrap();
            let claude = root.join("claude");
            let _ = std::os::unix::fs::symlink("/bin/sh", &claude);
            let launch = |tid: &str, n: u32, ask: &str| {
                let id = format!("{:08x}-0000-4000-8000-00000000000{n}", std::process::id());
                let at = fibo_core::content::now(); let ask = std::env::var("MOORING_COMPLETE_ASK").unwrap_or(ask.into());
                let user = serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/tmp","entrypoint":"cli","userType":"external","timestamp":at,"message":{"content":ask}});
                let reply = serde_json::json!({"type":"assistant","sessionId":id,"uuid":"a0","parentUuid":"u0","cwd":"/tmp","timestamp":at,"message":{"role":"assistant","model":"claude-test","stop_reason":"end_turn","content":[{"type":"text","text":"已完成，测试全过。"}]}});
                let end = serde_json::json!({"type":"system","subtype":"turn_duration","sessionId":id,"uuid":"s0","parentUuid":"a0","cwd":"/tmp","timestamp":at});
                let seed = root.join(format!("seed-{tid}"));
                std::fs::write(&seed, format!("{user}\n{reply}\n{end}\n")).unwrap();
                // A full screen of text when measuring, so the dissolve sends its most specks.
                let fill = if std::env::var_os("MOORING_COMPLETE_FRAMES").is_some() { format!("printf '%s\\n' '❯ {ask}'; i=0; while [ $i -lt 40 ]; do printf '%s\\n' \"{}\"; i=$((i+1)); done; ", std::env::var("MOORING_COMPLETE_FILL").unwrap_or("  test auth::timeout::retries_after_expiry ... ok   刷新 token 后重新计时 $i 行 0123456789 abcdefghij".into())) } else { String::new() };
                let script = format!("cat '{}' > '{}'; {fill}printf '%s\\n' '⏺ 已完成，测试全过。'; sleep 600", seed.display(), logs.join(format!("{id}.jsonl")).display());
                let argv = vec!["/bin/sh".into(), "-c".into(), format!("exec '{}' -c \"$0\" fixture --session-id {id}", claude.display()), script];
                call(Request::Launch { colors: None, id: tid.into(), space: space.clone(), argv, cwd: "/tmp".into(), cols: 100, rows: 30, scheme: Default::default() });
                call(Request::Edit { request: None, edit: Edit::CreateTab { tab: Tab { id: format!("tab-{tid}"), space: space.clone(), default_task: None, title: tid.into(), layout: Some(Layout::Leaf(tid.into())) } } });
            };
            let shot = |name: &str, cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh(); w.draw(a).clear(a);
                    w.render_to_image().unwrap().save(directory.join(name)).unwrap();
                }).unwrap();
            };
            for (tid, n, ask) in [("done", 1, "修好登录页的报错"), ("keep", 2, "补上导出按钮")] {
                launch(tid, n, ask);
                wait(300).await;
                handle.update(cx, |v, _, a| v.visual_peek_show_tab(tid, a)).unwrap();
                let mut ready = false;
                for _ in 0..120 {
                    wait(500).await;
                    let (session, review, _) = handle.update(cx, |v, _, _| v.visual_completable(tid)).unwrap();
                    if session.is_some() && review { ready = true; break; }
                }
                if !ready {
                    let w = workspace();
                    for s in &w.agent_sessions {
                        eprintln!("session {} terminal={:?} current={} content={:?}", s.id, s.terminal, s.current, w.content.get(&s.id).map(|c| (c.turn, c.updated)));
                    }
                    eprintln!("pane: {:?}", handle.update(cx, |v, _, _| v.visual_completable(tid)).unwrap());
                }
                assert!(ready, "{tid}: the finished session never offered 标记完成");
                wait(300).await;
                let style = std::env::var("MOORING_COMPLETE_STYLE").ok();
                if tid == "done" && let Some(style) = &style {
                    // A plugin's animation, held at points of the run for comparison with
                    // docs/complete-animation/directions.html, then played for real.
                    let style: fibo_core::effect::Style = serde_json::from_value(serde_json::json!(style)).expect("style");
                    let effect = fibo_core::effect::Complete { land: std::env::var_os("MOORING_COMPLETE_NO_LAND").is_none(), ..fibo_core::effect::Complete::of(style) };
                    shot("0-review.png", cx);
                    let times = handle.update(cx, |v, w, _| v.visual_complete_times(w)).unwrap(); for &t in &times {
                        handle.update(cx, |v, _, a| {
                            if t == times[0] {
                                v.visual_complete_with(tid, effect, Some(t), a);
                            } else {
                                v.visual_pin_completing(tid, Some(t), a);
                            }
                        }).unwrap();
                        wait(150).await;
                        shot(&format!("{style:?}-t{:03}.png", (t * 100.).round() as u32), cx);
                    }
                    handle.update(cx, |v, _, a| v.visual_keep_completed(tid, a)).unwrap();
                    let _ = handle.update(cx, |v, w, a| { if std::env::var_os("MOORING_COMPLETE_ACTIVATE").is_some() { w.activate_window(); a.activate(true); } v.visual_take_paints() }); wait(200).await; let _ = handle.update(cx, |v, _, _| v.visual_take_paints());
                    handle.update(cx, |v, _, a| v.visual_complete_with(tid, effect, None, a)).unwrap();
                    let mut closed = false;
                    for _ in 0..60 {
                        wait(100).await;
                        if !handle.update(cx, |v, _, _| v.visual_completable(tid)).unwrap().2 { closed = true; break; }
                    }
                    assert!(closed, "the completed pane never closed after {style:?}");
                    let (gaps, paints) = handle.update(cx, |v, _, _| v.visual_take_paints()).unwrap();
                    let stat = |mut v: Vec<f32>| {
                        v.sort_by(f32::total_cmp);
                        let at = |q: f32| v.get(((v.len() as f32 - 1.) * q).round() as usize).copied().unwrap_or(0.);
                        format!("n={} p50={:.2} p95={:.2} max={:.2}", v.len(), at(0.5), at(0.95), at(1.))
                    };
                    let specks = handle.update(cx, |v, _, _| v.visual_specks()).unwrap();
                    let active = handle.update(cx, |_, w, _| w.is_window_active()).unwrap(); eprintln!("FRAMES {style:?}: active={active} specks={specks} gap_ms {} | overlay_paint_ms {}", stat(gaps), stat(paints));
                    wait(300).await;
                    shot(&format!("{style:?}-closed.png"), cx);
                } else if tid == "done" && let Ok(at) = std::env::var("MOORING_COMPLETE_CARD") {
                    // 标记完成 from the session card on its own pane: a real click at the
                    // card's solid button ("x,y" in window points) folds the card and plays
                    // the animation picked in settings, then closes the pane.
                    handle.update(cx, |v, _, a| v.visual_open_here(tid, a)).unwrap();
                    wait(400).await;
                    shot("card-0-open.png", cx);
                    if let Some((x, y)) = at.split_once(',') {
                        let at = gpui::point(px(x.parse().unwrap()), px(y.parse().unwrap()));
                        cx.update_window(handle.into(), |_, w, a| native_click(w, a, at, gpui::MouseButton::Left)).unwrap();
                        wait(600).await;
                        shot("card-1-playing.png", cx);
                        let mut closed = false;
                        for _ in 0..60 {
                            wait(100).await;
                            if !handle.update(cx, |v, _, _| v.visual_completable(tid)).unwrap().2 { closed = true; break; }
                        }
                        assert!(closed, "the pane never closed after 标记完成 on its card");
                        let w = workspace();
                        assert!(w.agent_sessions.iter().any(|s| s.outcome == Outcome::Completed), "outcome not saved");
                        wait(300).await;
                        shot("card-2-closed.png", cx);
                        eprintln!("complete card PASS: the card's 标记完成 played the animation and closed the pane");
                    }
                    cx.update(|cx| cx.quit());
                    return;
                } else if tid == "done" {
                    shot("0-review.png", cx);
                    handle.update(cx, |v, _, a| v.visual_complete(tid, a)).unwrap();
                    // Timed from the click: a shot itself takes a while in a debug build.
                    let started = std::time::Instant::now();
                    for (ms, name) in [(120, "1-fill"), (400, "2-seal"), (900, "3-countdown"), (1450, "4-fading")] {
                        let due = Duration::from_millis(ms);
                        wait(due.saturating_sub(started.elapsed()).as_millis() as u64).await;
                        let at = started.elapsed().as_millis();
                        shot(&format!("{name}-{at}ms.png"), cx);
                    }
                    let mut closed = false;
                    for _ in 0..40 {
                        wait(100).await;
                        if !handle.update(cx, |v, _, _| v.visual_completable(tid)).unwrap().2 { closed = true; break; }
                    }
                    assert!(closed, "the completed pane never closed");
                    let w = workspace();
                    assert!(w.agent_sessions.iter().any(|s| s.outcome == Outcome::Completed), "outcome not saved");
                    wait(300).await;
                    shot("5-closed.png", cx);
                } else {
                    handle.update(cx, |v, _, a| v.visual_complete(tid, a)).unwrap();
                    wait(600).await;
                    handle.update(cx, |v, _, a| v.visual_keep_completed(tid, a)).unwrap();
                    wait(2000).await;
                    assert!(handle.update(cx, |v, _, _| v.visual_completable(tid)).unwrap().2, "a kept pane closed");
                    shot("6-kept.png", cx);
                }
            }
            eprintln!("complete native PASS: review offers 标记完成, fill/seal/countdown/fade frames, pane closes with outcome saved, a kept pane stays");
            cx.update(|cx| cx.quit());
        }).detach();
        return true;
    }
    false
}
