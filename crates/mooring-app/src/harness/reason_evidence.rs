use super::*;

// The status chip with why the session needs you (reasons rules), in four real panes:
// an approval, an error, a question's short title, and a text no rule knows.
pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) {
    if let (Some(handle), Some(directory)) = (&handle, evidence.clone())
        && std::env::var_os("MOORING_REASON_EVIDENCE").is_some()
    {
        let handle = *handle;
        cx.spawn(async move |cx| {
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
            for (id, line) in [
                ("rw", "WebSearch"),
                ("re", "OAuth"),
                ("rq", "合并授权"),
                ("rr", "grok"),
            ] {
                let argv = vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    format!("printf '  … {line}\\n'; exec /bin/sh -i"),
                ];
                call(Request::Launch {
                    id: id.into(),
                    space: space.clone(),
                    argv,
                    cwd: "/tmp".into(),
                    cols: 80,
                    rows: 20,
                    scheme: Default::default(),
                    colors: None,
                });
            }
            let leaf = |id: &str| Layout::Leaf(id.into());
            let split = |axis, a, b| Layout::Split {
                axis,
                ratio: 5000,
                first: Box::new(a),
                second: Box::new(b),
            };
            let layout = split(
                mooring_core::layout::Axis::Right,
                split(mooring_core::layout::Axis::Down, leaf("rw"), leaf("re")),
                split(mooring_core::layout::Axis::Down, leaf("rq"), leaf("rr")),
            );
            call(Request::Edit {
                request: None,
                edit: Edit::CreateTab {
                    tab: Tab {
                        id: "tab-reason".into(),
                        space,
                        default_task: None,
                        title: "reason".into(),
                        layout: Some(layout),
                    },
                },
            });
            wait(800).await;
            handle
                .update(cx, |v, _, a| v.visual_peek_show_tab("reason", a))
                .unwrap();
            wait(800).await;
            handle
                .update(cx, |v, _, a| v.visual_reason_fixture(a))
                .unwrap();
            for (name, style) in [
                ("reason-standard.png", crate::ui::style::Style::Standard),
                ("reason-terminal.png", crate::ui::style::Style::Terminal),
            ] {
                cx.update(|a| a.set_global(style));
                wait(600).await;
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                    w.render_to_image()
                        .unwrap()
                        .save(directory.join(name))
                        .unwrap();
                })
                .unwrap();
            }
            eprintln!("reason evidence written");
            cx.update(|a| a.quit());
        })
        .detach();
    }
}
