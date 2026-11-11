//! The diagnostic sheet itself.
use super::charts::{corners, panel};
use super::memory::memory_panel;
use super::*;

impl Render for DiagnosticView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.sampling.is_some();
        let mut body = div()
            .id("diagnostic-body")
            .overflow_y_scroll()
            .min_h_0()
            .flex_1();
        // Two sampling intervals; the first fills when the first report arrives.
        let done = self
            .report
            .as_ref()
            .map_or(0, |r| if r.complete { 2 } else { 1 });
        // While the first interval runs, its step fills with the time the host needs.
        let waited = self
            .sampling
            .as_ref()
            .filter(|_| self.report.is_none())
            .map(|s| s.started.elapsed());
        let mut steps = div().flex().gap(px(T::S2));
        for i in 0..2 {
            let mut step = div()
                .w(px(T::STEP_WIDTH))
                .h(px(T::STEP_HEIGHT))
                .rounded(px(T::S2))
                .overflow_hidden()
                .bg(T::color(if i < done {
                    T::secondary()
                } else {
                    T::chart_grid()
                }));
            if i == 0
                && let Some(waited) = waited
            {
                let share = (waited.as_secs_f32() / FIRST_REPORT.as_secs_f32()).min(1.);
                step = step.child(
                    div()
                        .h_full()
                        .w(gpui::relative(share))
                        .bg(T::color(T::secondary())),
                );
            }
            steps = steps.child(step);
        }
        let state = if running {
            t!("采样中")
        } else if self.report.as_ref().is_some_and(|r| r.complete) {
            t!("采样已结束")
        } else {
            t!("采样已停止")
        };
        let mut status = div()
            .flex()
            .items_center()
            .gap(px(T::S10))
            .text_size(px(T::SMALL))
            .text_color(T::color(T::secondary()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_color(T::color(T::primary()))
                    .child(
                        div()
                            .size(px(T::DOT))
                            .rounded_full()
                            .bg(T::color(if running { T::primary() } else { T::muted() })),
                    )
                    .child(state),
            )
            .child(steps);
        if let Some(report) = &self.report {
            status = status
                .child(t!(
                    "已采 {elapsed} 秒 · 区间 {done} / 2",
                    elapsed = format!("{:.1}", report.elapsed_secs),
                    done
                ))
                .child(t!("· 开始于 {time}", time = clock(report.started_at)));
        }
        // While waiting, the countdown lives on the body's last line, not here too.
        if let Some(report) = &self.report {
            let listed: f64 = report.ranked().map(|a| a.cpu_percent).sum::<f64>() / 100.;
            let busy = report
                .cpu_cores
                .zip(report.cpu_percent)
                .map(|(c, p)| p / 100. * c as f64);
            let memory = report.memory;
            let unread = || t!("不可读").to_string();
            body = body.child(
                div()
                    .px(px(T::S20))
                    .py(px(T::S16))
                    .border_b(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .border(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::border()))
                            .child(panel(
                                t!("整机 CPU · 采样平均"),
                                report.cpu_percent,
                                [
                                    (
                                        t!("核心"),
                                        report.cpu_cores.map_or_else(unread, |c| c.to_string()),
                                    ),
                                    (
                                        t!("在忙"),
                                        busy.map_or_else(unread, |b| {
                                            t!("约 {cores} 核", cores = format!("{b:.1}"))
                                        }),
                                    ),
                                    (
                                        t!("排行能解释"),
                                        t!("{cores} 核", cores = format!("{listed:.1}")),
                                    ),
                                ],
                                t!(
                                    "排行前 {ranked} 项 · 共 {processes} 个进程",
                                    ranked = report.ranked().count(),
                                    processes = report.process_count
                                ),
                                false,
                            ))
                            .child(memory_panel(memory))
                            .child(panel(
                                t!("GPU 设备 · 最新读数"),
                                report.gpu_percent,
                                [
                                    (
                                        t!("活跃度"),
                                        report
                                            .gpu_percent
                                            .map_or_else(unread, |p| format!("{p:.1}%")),
                                    ),
                                    (
                                        t!("有计数的应用"),
                                        t!("{count} 个", count = report.gpu_ranked().len()),
                                    ),
                                    (t!("区间"), format!("{done} / 2")),
                                ],
                                t!("设备活跃度，不是各应用份额之和").into(),
                                true,
                            ))
                            .child(corners()),
                    ),
            );
            if let Some(stack) = self.core_stack(report) {
                body = body.child(stack);
            }
            if let Some(stack) = self.memory_stack(report) {
                body = body.child(stack);
            }
            let pair = |left: gpui::Div, right: gpui::Div| {
                div()
                    .flex()
                    .items_start()
                    .child(left)
                    .child(
                        div()
                            .w(px(T::BORDER_WIDTH))
                            .self_stretch()
                            .bg(T::color(T::border())),
                    )
                    .child(
                        div()
                            .w(gpui::relative(0.42))
                            .flex_shrink_0()
                            .px(px(T::S20))
                            .py(px(T::S16))
                            .child(right),
                    )
            };
            let cpu = pair(self.cpu_ranking(report, cx), self.scatter(report));
            let memory_row = pair(self.memory_ranking(report, cx), self.gpu_ranking(report));
            // Under memory pressure the memory row is what to read first.
            let tight = report
                .memory
                .and_then(|m| m.pressure)
                .is_some_and(|p| p != Pressure::Normal);
            let line = |d: gpui::Div| {
                d.border_t(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
            };
            body = if tight {
                body.child(memory_row).child(line(cpu))
            } else {
                body.child(cpu).child(line(memory_row))
            };
        } else {
            body = body.child(match (&self.sampling, waited) {
                (Some(sampling), Some(waited)) => self.waiting(sampling, waited, cx),
                _ => div()
                    .p(px(T::S20))
                    .child(t!("采样已停止，尚无可比较的数据。")),
            });
        }
        if let Some(error) = &self.error {
            body = body.child(
                div()
                    .px(px(T::S20))
                    .pb(px(T::S16))
                    .text_color(Tone::Danger.ink())
                    .child(error.clone()),
            );
        }
        let notes = self.report.as_ref().map_or(0, |r| r.notes.len());
        let mut panel = div()
            .id("diagnostic-panel")
            .w(px(T::DIAGNOSTIC_WIDTH))
            .max_w_full()
            .h(px(T::DIAGNOSTIC_HEIGHT))
            .max_h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(T::POPOVER_RADIUS))
            .bg(T::color(T::popover()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            .font_features(tabular())
            .shadow_lg()
            .track_focus(&self.focus)
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape"
                    || (event.keystroke.key == "w" && event.keystroke.modifiers.secondary())
                {
                    cx.emit(DiagnosticAction::Close);
                } else if event.keystroke.key == "c" && event.keystroke.modifiers.secondary() {
                    this.copy(cx);
                }
                cx.stop_propagation();
            }))
            .child(
                div()
                    .px(px(T::S20))
                    .pt(px(T::S16))
                    .pb(px(T::S14))
                    .border_b(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .flex()
                    .items_center()
                    .gap(px(T::S12))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(T::S6))
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap(px(T::S10))
                                    .child(
                                        div()
                                            .text_size(px(T::FORM_TITLE))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(T::color(T::primary()))
                                            .child(t!("本机诊断")),
                                    )
                                    .child(label(t!(
                                        "看谁在占 CPU / 内存 / GPU，再把证据交给 Agent"
                                    ))),
                            )
                            .child(status),
                    )
                    .child(
                        button("diagnostic-close", t!("关闭 · Esc"))
                            .bg(T::color(T::button()))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(DiagnosticAction::Close))),
                    ),
            )
            .child(body);
        if self.notes_open
            && let Some(report) = &self.report
        {
            {
                let mut list = div()
                    .px(px(T::S20))
                    .py(px(T::S10))
                    .border_t(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .bg(T::color(T::raised()))
                    .flex()
                    .flex_col()
                    .gap(px(T::S4))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::secondary()));
                for note in &report.notes {
                    list = list.child(format!("· {note}"));
                }
                panel = panel.child(list);
            }
        }
        panel.child(
            div()
                .px(px(T::S20))
                .py(px(T::S12))
                .border_t(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .flex()
                .items_center()
                .gap(px(T::S10))
                .when(notes > 0, |d| {
                    d.child(
                        button(
                            "diagnostic-notes",
                            t!(
                                "ⓘ 读数说明 · {notes} 条 {arrow}",
                                notes,
                                arrow = if self.notes_open { "▴" } else { "▾" }
                            ),
                        )
                        .px(px(T::S8))
                        .text_color(T::color(T::secondary()))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notes_open = !this.notes_open;
                            cx.notify();
                        })),
                    )
                })
                .child(label(t!("只在本机读取，结束后停止采样")))
                .child(div().flex_1())
                .child(
                    button(
                        "diagnostic-run",
                        if running {
                            t!("停止采样")
                        } else {
                            t!("重新采样")
                        },
                    )
                    .bg(T::color(T::button()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.sampling.is_some() {
                            this.sampling = None;
                            cx.notify();
                        } else {
                            this.start(cx);
                        }
                    })),
                )
                .when(self.report.is_some(), |d| {
                    d.child(
                        button(
                            "diagnostic-copy",
                            if self.copied {
                                t!("已复制 ✓").to_string()
                            } else {
                                crate::ui::words::local_field(t!("复制摘要给 Agent · ⌘C"))
                            },
                        )
                        .bg(T::color(T::accent()))
                        .text_color(T::color(T::on_accent()))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
                }),
        )
    }
}
