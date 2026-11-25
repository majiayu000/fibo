//! Repair a live terminal that never made it into any of the host's tabs.
use super::*;

impl DiagnosticView {
    fn recover_terminal(&mut self, id: String, place: bool, cx: &mut Context<Self>) {
        if self.unplaced_busy {
            return;
        }
        self.unplaced_busy = true;
        self.unplaced_confirm = None;
        self.unplaced_error = None;
        let path = self.socket.clone();
        let terminal = id.clone();
        let task = cx.background_spawn(async move {
            let connection = DiagnosticConnection::default();
            let mut stream = connection.state.connect(path, Some(REPLY_WAIT))?;
            send(
                &mut stream,
                CONTROL,
                &Request::RecoverTerminal {
                    id: terminal,
                    place,
                },
            )?;
            receive::<Reply>(&mut stream)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.unplaced_busy = false;
                match result {
                    Ok(Reply::Workspace(workspace)) if place => {
                        if let Some(report) = &mut this.report {
                            report.unplaced.retain(|t| t.id != id);
                        }
                        cx.emit(DiagnosticAction::Recovered(Box::new(workspace), id));
                    }
                    Ok(Reply::Ok) if !place => {
                        if let Some(report) = &mut this.report {
                            report.unplaced.retain(|t| t.id != id);
                        }
                    }
                    Ok(Reply::Error(e)) => this.unplaced_error = Some(e),
                    Err(e) => this.unplaced_error = Some(e.to_string()),
                    _ => this.unplaced_error = Some(t!("诊断响应无效").into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn unplaced_view(&self, cx: &Context<Self>) -> Option<gpui::Div> {
        let report = self.report.as_ref()?;
        let count = report.unplaced.len();
        if count == 0 {
            return None;
        }
        let mut section = div()
            .px(px(T::S20))
            .py(px(T::S12))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .child(
                button(
                    "unplaced-expand",
                    t!(
                        "没有窗格的终端 {count} 个 {arrow}",
                        count,
                        arrow = if self.unplaced_open { "▴" } else { "▾" }
                    ),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.unplaced_open = !this.unplaced_open;
                    this.unplaced_confirm = None;
                    cx.notify();
                })),
            );
        if !self.unplaced_open {
            return Some(section);
        }
        if let Some(error) = &self.unplaced_error {
            section = section.child(div().text_color(Tone::Danger.ink()).child(error.clone()));
        }
        for terminal in &report.unplaced {
            let mut row = div()
                .py(px(T::S8))
                .child(div().child(terminal.program.clone()))
                .child(label(terminal.cwd.clone()));
            let mem = report
                .terminals
                .iter()
                .find(|t| t.id == terminal.id)
                .map_or_else(|| "—".into(), |t| mib(t.memory_bytes));
            let started = terminal.started_at.map_or_else(
                || "—".into(),
                |at| format!("{} · {}", ago(at, now()), clock(at)),
            );
            row = row.child(label(t!("内存 {mem} · 启动于 {started}", mem, started)));
            if !self.unplaced_busy {
                let id = terminal.id.clone();
                let mut actions = div().flex().gap(px(T::S8));
                if self.unplaced_confirm.as_ref() == Some(&id) {
                    actions = actions
                        .child(button("unplaced-cancel", t!("取消")).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.unplaced_confirm = None;
                                cx.notify();
                            },
                        )))
                        .child(
                            button("unplaced-confirm", t!("确认结束终端"))
                                .text_color(Tone::Danger.ink())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.recover_terminal(id.clone(), false, cx)
                                })),
                        );
                } else {
                    let place = id.clone();
                    actions = actions
                        .child(
                            button(
                                gpui::SharedString::from(format!("unplaced-place-{id}")),
                                t!("放进窗格"),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.recover_terminal(place.clone(), true, cx)
                                },
                            )),
                        )
                        .child(
                            button(
                                gpui::SharedString::from(format!("unplaced-end-{id}")),
                                t!("结束终端"),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.unplaced_confirm = Some(id.clone());
                                    cx.notify();
                                },
                            )),
                        );
                }
                row = row.child(actions);
            }
            section = section.child(row);
        }
        Some(section)
    }
}
