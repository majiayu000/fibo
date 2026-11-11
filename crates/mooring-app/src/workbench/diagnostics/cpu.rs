//! Where the cores went and the CPU ranking.
use super::charts::{
    Segment, axis, bar_row, chart_note, chip, cpu_scale, heading, plot, stack_chart, tick,
};
use super::*;

impl DiagnosticView {
    /// Where the whole machine's cores went: colored apps, the rest of the ranking,
    /// busy time no app accounts for, and idle.
    pub(super) fn core_stack(&self, report: &Diagnostics) -> Option<gpui::Div> {
        let total = report.cpu_cores? as f64;
        let busy = report.cpu_percent? / 100. * total;
        let ranked: Vec<&AppUsage> = report.ranked().collect();
        let rest: f64 = ranked
            .iter()
            .skip(COLORED)
            .map(|a| a.cpu_percent)
            .sum::<f64>()
            / 100.;
        let unattributed = report.unattributed_cores()?;
        let mut segments: Vec<Segment> = ranked
            .iter()
            .take(COLORED)
            .enumerate()
            .map(|(i, a)| {
                Segment::new(a.name.clone(), a.cpu_percent / 100., T::series(i))
                    .detail(t!("{count} 个进程", count = a.processes))
            })
            .collect();
        if ranked.len() > COLORED {
            segments.push(Segment::new(
                t!("排行其余 {count} 项", count = ranked.len() - COLORED),
                rest,
                T::series_other(),
            ));
        }
        let mut hatched = Segment::new(t!("未归属").into(), unattributed, T::hatch())
            .detail(t!("排行之外或采样间隙里的短命进程").into());
        hatched.hatched = true;
        segments.push(hatched);
        let idle = (total - busy).max(0.);
        if idle >= 0.05 {
            segments.push(Segment::new(t!("空闲").into(), idle, T::chart_grid()));
        }
        let (stack, legend) = stack_chart("diag-core", segments, |v| {
            (
                t!(
                    "{cores} 核 · 整机 {pct}%",
                    cores = format!("{v:.2}"),
                    pct = format!("{:.1}", v / total * 100.)
                ),
                t!("{cores} 核", cores = format!("{v:.2}")),
            )
        });
        let mut scale = div()
            .relative()
            .h(px(T::TICK_ROW))
            .mt(px(T::S4))
            .text_size(px(T::CHIP_SIZE))
            .text_color(T::color(T::muted()));
        for q in 0..=4 {
            let at = q as f32 / 4.;
            let text = if q == 4 {
                t!("{cores} 核", cores = format!("{:.0}", total * at as f64))
            } else {
                format!("{:.0}", total * at as f64)
            };
            scale = scale.child(tick(text, at));
        }
        let mut section = div()
            .px(px(T::S20))
            .pt(px(T::S16))
            .pb(px(T::S16))
            .border_b(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .child(heading(
                t!("全部核心都在忙什么"),
                t!("宽度 = 平均占用的核心数 · 悬停看明细"),
            ))
            .child(stack)
            .child(scale)
            .child(legend);
        if unattributed >= 0.5 {
            section = section.child(chart_note(
                t!("推测"),
                t!(
                    "约 {cores} 核没有归到排行里的应用。可能来自排行之外的进程，或在两次采样之间出生又退出的短命进程（排行只统计前后都在的进程）。要确认，需要抓进程创建记录。",
                    cores = format!("{unattributed:.1}")
                ),
            ));
        }
        Some(section)
    }
    pub(super) fn cpu_ranking(&self, report: &Diagnostics, cx: &Context<Self>) -> gpui::Div {
        let ranked: Vec<&AppUsage> = report.ranked().collect();
        let (max, step) = cpu_scale(ranked.first().map_or(0., |a| a.cpu_percent));
        let mut column = div()
            .flex_1()
            .min_w_0()
            .px(px(T::S20))
            .py(px(T::S16))
            .child(heading(t!("CPU 占用排行"), t!("平均 · 虚线 = 1 个核心")))
            .child(axis(max, step, Some(100.), T::BAR_LABEL));
        // The right column holds the scatter only; GPU moved beside the memory ranking.
        let first = first_rows(0);
        // A 「再显示 1 项」 row costs as much space as the row it hides.
        let first = if ranked.len() <= first + 2 {
            ranked.len()
        } else {
            first
        };
        let shown = if self.all_rows {
            ranked.len()
        } else {
            ranked.len().min(first)
        };
        for (i, app) in ranked.iter().take(shown).enumerate() {
            let named: Vec<(String, String)> = app
                .terminals
                .iter()
                .filter_map(|id| {
                    self.terminals
                        .get(id)
                        .map(|title| (id.clone(), title.clone()))
                })
                .collect();
            let open = self.open.as_deref() == Some(app.name.as_str()) && !named.is_empty();
            let mut extra = div().flex().items_center().gap(px(T::S4)).flex_shrink_0();
            if app.processes > 1 {
                extra = extra.child(chip(format!("×{}", app.processes)));
            }
            if !named.is_empty() {
                let name = app.name.clone();
                extra = extra.child(
                    div()
                        .id(gpui::SharedString::from(format!("diag-terms-{}", app.name)))
                        .flex_shrink_0()
                        .px(px(T::S6))
                        .rounded(px(T::S8))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::control()))
                        .text_size(px(T::CHIP_SIZE))
                        .text_color(T::color(T::link()))
                        .cursor_pointer()
                        .hover(|s| s.bg(T::color(T::button_hover())))
                        .child(t!(
                            "{count} 终端 {arrow}",
                            count = named.len(),
                            arrow = if open { "▴" } else { "▾" }
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open =
                                (this.open.as_deref() != Some(name.as_str())).then(|| name.clone());
                            cx.notify();
                        })),
                );
            }
            let mut lines = vec![
                app.name.clone(),
                t!(
                    "平均 {avg}% ≈ {cores} 核",
                    avg = format!("{:.1}", app.cpu_percent),
                    cores = cores(app.cpu_percent)
                ),
            ];
            if app.cpu_peak_percent > app.cpu_percent + 0.05 {
                lines.push(t!(
                    "区间峰值 {peak}%",
                    peak = format!("{:.1}", app.cpu_peak_percent)
                ));
            }
            lines.push(t!(
                "内存 {mem} · {count} 个进程",
                mem = mib(app.memory_bytes),
                count = app.processes
            ));
            let color = if i < COLORED {
                T::series(i)
            } else {
                T::series_other()
            };
            let mut bars = plot(app.cpu_percent, max, step, Some(100.), color);
            if app.cpu_peak_percent > app.cpu_percent + 0.05 {
                bars = bars.child(
                    div()
                        .absolute()
                        .top(px((T::ROW - T::TICK_ROW) / 2.))
                        .h(px(T::TICK_ROW))
                        .w(px(T::S2))
                        .rounded(px(T::BORDER_WIDTH))
                        .bg(T::color(T::primary()))
                        .left(gpui::relative((app.cpu_peak_percent / max).min(1.) as f32)),
                );
            }
            column = column.child(
                bar_row(
                    format!("diag-cpu-{}", app.name).into(),
                    &app.name,
                    extra,
                    bars,
                    // Four digits drop the decimal so the value column keeps one line.
                    if app.cpu_percent >= 1000. {
                        format!("{:.0}%", app.cpu_percent)
                    } else {
                        format!("{:.1}%", app.cpu_percent)
                    },
                    T::BAR_LABEL,
                )
                .tooltip(tip(lines)),
            );
            if open {
                let mut list = div().flex().flex_wrap().gap(px(T::S6)).py(px(T::S8));
                for (id, title) in named {
                    list = list.child(
                        button(
                            gpui::SharedString::from(format!("diag-term-{id}")),
                            format!("{title} →"),
                        )
                        .h(px(T::TICK_ROW + T::S6))
                        .px(px(T::S8))
                        .bg(T::color(T::button()))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(DiagnosticAction::Terminal(id.clone()))
                        })),
                    );
                }
                column = column.child(
                    list.border_b(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::border())),
                );
            }
        }
        if ranked.len() > first {
            column = column.child(
                button(
                    "diagnostic-more",
                    if self.all_rows {
                        t!("收起").to_owned()
                    } else {
                        t!("再显示 {count} 项", count = ranked.len() - first)
                    },
                )
                .mt(px(T::S6))
                .px(px(T::S8))
                .text_color(T::color(T::link()))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.all_rows = !this.all_rows;
                    cx.notify();
                })),
            );
        }
        let one_interval = ranked
            .iter()
            .all(|a| a.cpu_peak_percent <= a.cpu_percent + 0.05);
        if one_interval && !ranked.is_empty() {
            column = column.child(
                div()
                    .mt(px(T::S10))
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .child(t!(
                        "只采到 1 个区间，峰值暂同平均；第 2 个区间后会出现峰值刻度。"
                    )),
            );
        }
        if ranked.is_empty() {
            column = column.child(label(t!("这次没有测到 CPU 占用。")));
        }
        column
    }
}
