//! Explicit local disk scans and per-worktree confirmation.
use super::charts::heading;
use super::*;
use fibo_core::diagnostics::ReclaimableWorktree;

#[derive(Default)]
pub(super) struct WorktreePanel {
    rows: Option<Vec<ReclaimableWorktree>>,
    pending: Option<(ReclaimableWorktree, bool)>,
    operation: Option<(DiagnosticConnection, bool)>,
    error: Option<String>,
}

impl DiagnosticView {
    pub(super) fn has_worktree_result_or_cleanup(&self) -> bool {
        self.worktrees.as_ref().is_some_and(|panel| {
            panel.rows.is_some() || panel.operation.as_ref().is_some_and(|(_, scan)| !scan)
        })
    }

    pub(super) fn close_worktree_scan(&mut self) {
        if let Some(panel) = &mut self.worktrees {
            panel.pending = None;
            if panel.operation.as_ref().is_some_and(|(_, scan)| *scan) {
                panel.operation = None;
            }
        }
    }

    fn run_worktree_request(&mut self, request: Request, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.worktrees else {
            return;
        };
        if panel.operation.is_some() {
            return;
        }
        let scan = matches!(request, Request::ScanWorktrees);
        let path = match &request {
            Request::ReclaimWorktree { path, .. } => Some(path.clone()),
            _ => None,
        };
        let socket = self.socket.clone();
        let connection = DiagnosticConnection::default();
        let state = connection.state.clone();
        panel.operation = Some((connection, scan));
        panel.error = None;
        panel.pending = None;
        let (tx, mut rx) = unbounded();
        std::thread::spawn({
            let state = state.clone();
            move || {
                let run = || -> io::Result<Reply> {
                    let mut stream = state.connect(socket, None)?;
                    send(&mut stream, CONTROL, &request)?;
                    match receive(&mut stream)? {
                        Reply::Error(e) => Err(io::Error::other(e)),
                        reply => Ok(reply),
                    }
                };
                let _ = tx.unbounded_send(run());
            }
        });
        cx.spawn(async move |this, cx| {
            let Some(result) = rx.next().await else {
                return;
            };
            if state.cancelled.load(Ordering::Acquire) {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                let Some(panel) = &mut this.worktrees else {
                    return;
                };
                panel.operation = None;
                match result {
                    Ok(Reply::ReclaimableWorktrees(rows)) if scan => panel.rows = Some(rows),
                    Ok(Reply::Ok) if !scan => {
                        if let (Some(path), Some(rows)) = (path, &mut panel.rows) {
                            // Any cached size for this row is now stale.
                            rows.retain(|row| row.path != path);
                        }
                    }
                    Ok(_) => panel.error = Some(t!("诊断响应无效").into()),
                    Err(error) => panel.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn worktree_view(&self, cx: &Context<Self>) -> Option<gpui::Div> {
        let panel = self.worktrees.as_ref()?;
        let running = panel.operation.as_ref();
        let mut section = div()
            .px(px(T::S20))
            .py(px(T::S12))
            .border_b(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .child(heading(
                t!("可回收的工作区"),
                t!("已合入上游且没有进程在用的 Git 工作区"),
            ));
        if let Some((row, entire)) = &panel.pending {
            let path = row.path.clone();
            let head = row.head.clone();
            let entire = *entire;
            return Some(
                section
                    .child(div().child(if entire {
                        t!("永久移除这个工作区？")
                    } else {
                        t!("永久删除这个工作区的 target/？")
                    }))
                    .child(label(path.clone()))
                    .child(
                        div()
                            .flex()
                            .gap(px(T::S8))
                            .child(button("worktree-keep", t!("取消")).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.worktrees.as_mut().unwrap().pending = None;
                                    cx.notify();
                                },
                            )))
                            .child(
                                button("worktree-confirm", t!("确认删除"))
                                    .text_color(Tone::Danger.ink())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.run_worktree_request(
                                            Request::ReclaimWorktree {
                                                path: path.clone(),
                                                head: head.clone(),
                                                entire,
                                            },
                                            cx,
                                        );
                                    })),
                            ),
                    ),
            );
        }
        section = section.child(match running {
            Some((_, true)) => button("worktree-scan", t!("取消扫描"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.close_worktree_scan();
                    cx.notify();
                }))
                .into_any_element(),
            Some((_, false)) => label(t!("正在清理工作区…")).into_any_element(),
            None => button(
                "worktree-scan",
                if panel.rows.is_some() {
                    t!("重新扫描工作区")
                } else {
                    t!("扫描工作区")
                },
            )
            .on_click(
                cx.listener(|this, _, _, cx| this.run_worktree_request(Request::ScanWorktrees, cx)),
            )
            .into_any_element(),
        });
        if let Some(error) = &panel.error {
            section = section.child(div().text_color(Tone::Danger.ink()).child(error.clone()));
        }
        if let Some(rows) = &panel.rows {
            if rows.is_empty() {
                section = section.child(label(t!("没有找到可回收的工作区")));
            }
            for (index, row) in rows.iter().enumerate() {
                let size = |bytes| {
                    if bytes >= 1 << 30 {
                        gib(bytes)
                    } else {
                        mib(bytes)
                    }
                };
                let mut actions = div().flex().flex_wrap().gap(px(T::S6));
                for (entire, can, title) in [
                    (false, row.target_bytes.is_some(), t!("只删 target/")),
                    (true, row.can_remove, t!("移除工作区")),
                ] {
                    if can && running.is_none() {
                        let row = row.clone();
                        actions = actions.child(
                            button(
                                gpui::SharedString::from(format!("worktree-{index}-{entire}")),
                                title,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.worktrees.as_mut().unwrap().pending =
                                        Some((row.clone(), entire));
                                    cx.notify();
                                },
                            )),
                        );
                    }
                }
                let target = row.target_bytes.map_or_else(|| "—".into(), size);
                section = section.child(
                    div()
                        .py(px(T::S8))
                        .border_t(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::border()))
                        .child(div().child(format!(
                            "{} · {}",
                            row.branch,
                            commit_time(row.committed_at)
                        )))
                        .child(label(row.path.clone()))
                        .child(label(t!(
                            "target/ {target} · 整个目录 {total}",
                            target,
                            total = size(row.total_bytes)
                        )))
                        .when_some(row.space.as_ref(), |d, name| {
                            d.child(label(t!("空间 {name}", name)))
                        })
                        .child(actions),
                );
            }
        }
        Some(section)
    }
}

fn commit_time(at: u64) -> String {
    let date = fibo_platform::clock::local(at);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        date.year, date.month, date.day, date.hour, date.minute
    )
}
