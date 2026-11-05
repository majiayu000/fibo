//! The memory panel, where memory went and the memory ranking.
use super::charts::{
    Segment, axis_by, bar_row, chart_note, chip, gb_scale, heading, parts_gauge, plot, stack_chart,
    tick,
};
use super::*;

/// The memory panel leads with pressure: a high percentage alone does not mean short,
/// since macOS keeps memory full of cache and compressed pages on purpose.
pub(super) fn memory_panel(memory: Option<Memory>) -> gpui::Div {
    let pressure = memory.and_then(|m| m.pressure);
    let tone = pressure.and_then(|p| match p {
        Pressure::Normal => None,
        Pressure::Warn => Some(Tone::Warn),
        Pressure::Critical => Some(Tone::Danger),
    });
    let ink = tone.map_or_else(|| T::color(T::primary()), Tone::ink);
    let unread = || "不可读".to_string();
    let reading = |key: &'static str, value: String, colored: bool| {
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(T::S2))
            .child(
                div()
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .child(key),
            )
            .child(
                div()
                    .truncate()
                    .text_size(px(T::PANEL_VALUE))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if colored { ink } else { T::color(T::primary()) })
                    .child(value),
            )
    };
    // Direction A (11-03): the pressure leads, in words, with the percentage beside it.
    let pill = div()
        .flex()
        .items_center()
        .gap(px(T::S4))
        .px(px(T::S6))
        .rounded(px(T::S8))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::control()))
        .text_size(px(T::SMALL))
        .text_color(T::color(T::secondary()))
        .child(
            div()
                .size(px(T::DOT))
                .rounded_full()
                .bg(tone.map_or_else(|| T::color(T::muted()), Tone::dot)),
        )
        .child(pressure.map_or_else(
            || "压力不可读".to_string(),
            |p| format!("压力{}", p.label()),
        ));
    let swap = memory.map_or(0, |m| m.swap_bytes);
    let (parts, hint) = match memory.filter(|m| m.total_bytes > 0) {
        Some(m) => {
            let share = |b: u64| b as f64 / m.total_bytes as f64;
            (
                vec![
                    (
                        share(m.app_bytes()),
                        tone.map_or_else(|| T::color(T::secondary()), |t| t.colors().0),
                    ),
                    (share(m.wired_bytes), T::color(T::series_other())),
                    (share(m.compressed_bytes), T::color(T::hatch())),
                ],
                format!(
                    // The gauge's three parts, in order; cache has its own reading above.
                    "App {:.1} · 联动 {:.1} · 压缩 {:.1} GB",
                    m.app_bytes() as f64 / GB,
                    m.wired_bytes as f64 / GB,
                    m.compressed_bytes as f64 / GB
                ),
            )
        }
        None => (Vec::new(), "内存读数不可读".into()),
    };
    div()
        .flex_1()
        .min_w_0()
        .px(px(T::S16))
        .py(px(T::S14))
        .flex()
        .flex_col()
        .gap(px(T::S12))
        .border_r(px(T::BORDER_WIDTH))
        .border_color(T::color(T::border()))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(label("内存 · 最新读数"))
                .child(pill),
        )
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(px(T::S10))
                .min_w_0()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(T::TITLE))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(ink)
                        .child(pressure.map_or_else(unread, |p| p.label().into())),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::secondary()))
                        .child(memory.map_or_else(unread, |m| {
                            format!(
                                "已用 {} / {:.0} GB · {:.0}%",
                                gib(m.used_bytes).trim_end_matches(" GB"),
                                m.total_bytes as f64 / GB,
                                m.percent().unwrap_or_default()
                            )
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .gap(px(T::S12))
                .child(reading(
                    "压缩",
                    memory.map_or_else(unread, |m| gib(m.compressed_bytes)),
                    false,
                ))
                .child(reading(
                    "交换",
                    memory.map_or_else(unread, |_| if swap == 0 { "0".into() } else { gib(swap) }),
                    swap > 0 && tone.is_some(),
                ))
                .child(reading(
                    "缓存",
                    memory.map_or_else(unread, |m| gib(m.cached_bytes)),
                    false,
                )),
        )
        .child(parts_gauge(parts, T::GAUGE_HEIGHT))
        .child(
            div()
                .text_size(px(T::SMALL))
                .text_color(T::color(T::muted()))
                .truncate()
                .child(hint),
        )
}

impl DiagnosticView {
    /// Where memory went, app by app. Widths split the apps' total, which counts compressed
    /// pages at their original size, so it is a share of the apps, not of physical memory.
    pub(super) fn memory_stack(&self, report: &Diagnostics) -> Option<gpui::Div> {
        let total = report.apps_memory();
        if total == 0 {
            return None;
        }
        let colors = app_colors(report);
        let ranked = report.memory_ranked();
        // Seven apps and the rest fill the legend's two rows of four.
        let top: Vec<&&AppUsage> = ranked.iter().take(7).collect();
        let shown: u64 = top.iter().map(|a| a.memory_bytes).sum();
        let mut segments: Vec<Segment> = top
            .iter()
            .map(|a| {
                Segment::new(
                    a.name.clone(),
                    a.memory_bytes as f64 / GB,
                    colors.get(&a.name).copied().unwrap_or(T::series_other()),
                )
                .detail(format!("{} 个进程", a.processes))
            })
            .collect();
        let others = report.apps.iter().filter(|a| a.memory_bytes > 0).count() - top.len();
        if others > 0 {
            let mut rest = Segment::new(
                format!("其余 {others} 个应用"),
                total.saturating_sub(shown) as f64 / GB,
                T::hatch(),
            );
            // Hatched, so the long tail never reads as one more gray app.
            rest.hatched = true;
            segments.push(rest);
        }
        let all = total as f64 / GB;
        let (stack, legend) = stack_chart("diag-mem", segments, |v| {
            (
                format!(
                    "{} · 各应用合计的 {:.1}%",
                    mib((v * GB) as u64),
                    v / all * 100.
                ),
                mib((v * GB) as u64),
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
            scale = scale.child(tick(
                format!("{:.0}{}", all * at as f64, if q == 4 { " GB" } else { "" }),
                at,
            ));
        }
        let used = report.memory.map_or_else(String::new, |m| {
            format!("，所以比已用的 {} 多", gib(m.used_bytes))
        });
        Some(
            div()
                .px(px(T::S20))
                .pt(px(T::S16))
                .pb(px(T::S16))
                .border_b(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .child(heading("内存都给了谁", "宽度 = 各应用占用 · 悬停看明细"))
                .child(stack)
                .child(scale)
                .child(legend)
                .child(chart_note(
                    "说明",
                    format!(
                        "各应用合计 {}，按活动监视器「内存」列的口径：被压缩的页按原始大小算{used}。这条看谁占得多，还剩多少看压力和交换。",
                        gib(total)
                    ),
                )),
        )
    }
    pub(super) fn memory_ranking(&self, report: &Diagnostics, cx: &Context<Self>) -> gpui::Div {
        let ranked = report.memory_ranked();
        let colors = app_colors(report);
        let (max, step) = gb_scale(ranked.first().map_or(0., |a| a.memory_bytes as f64 / GB));
        let mut column = div()
            .flex_1()
            .min_w_0()
            .px(px(T::S20))
            .py(px(T::S16))
            .child(heading("内存占用排行", "最新读数 · 活动监视器口径"))
            .child(axis_by(max, step, None, T::BAR_LABEL, |v| {
                if v == 0. {
                    "0".into()
                } else if v < 1. {
                    format!("{:.0} MB", v * 1024.)
                } else {
                    format!("{v:.0} GB")
                }
            }));
        let first = FIRST_ROWS.max(report.gpu_ranked().len() + 3);
        let first = if ranked.len() <= first + 2 {
            ranked.len()
        } else {
            first
        };
        let shown = if self.all_memory_rows {
            ranked.len()
        } else {
            ranked.len().min(first)
        };
        let mut assigned: HashSet<&str> = HashSet::new();
        for app in ranked.iter().take(shown) {
            // A terminal's tree spans several apps (shell, Agent, node); it sits under
            // the biggest of them.
            let mut terms: Vec<_> = report
                .terminals
                .iter()
                .filter(|t| app.terminals.contains(&t.id) && !assigned.contains(t.id.as_str()))
                .filter_map(|t| self.terminals.get(&t.id).map(|title| (t, title.clone())))
                .collect();
            terms.sort_by_key(|t| std::cmp::Reverse(t.0.memory_bytes));
            assigned.extend(terms.iter().map(|(t, _)| t.id.as_str()));
            let folded = self.memory_folded.contains(&app.name);
            let mut extra = div().flex().items_center().gap(px(T::S4)).flex_shrink_0();
            if app.processes > 1 {
                extra = extra.child(chip(format!("×{}", app.processes)));
            }
            if !terms.is_empty() {
                let name = app.name.clone();
                extra = extra.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "diag-memterms-{}",
                            app.name
                        )))
                        .flex_shrink_0()
                        .px(px(T::S6))
                        .rounded(px(T::S8))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::control()))
                        .text_size(px(T::CHIP_SIZE))
                        .text_color(T::color(T::link()))
                        .cursor_pointer()
                        .hover(|s| s.bg(T::color(T::button_hover())))
                        .child(format!(
                            "{} 终端 {}",
                            terms.len(),
                            if folded { "▾" } else { "▴" }
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.memory_folded.remove(&name) {
                                this.memory_folded.insert(name.clone());
                            }
                            cx.notify();
                        })),
                );
            }
            let color = colors.get(&app.name).copied().unwrap_or(T::series_other());
            column = column.child(
                bar_row(
                    format!("diag-mem-{}", app.name).into(),
                    &app.name,
                    extra,
                    plot(app.memory_bytes as f64 / GB, max, step, None, color),
                    mib(app.memory_bytes),
                    T::BAR_LABEL,
                )
                .tooltip(tip(vec![
                    app.name.clone(),
                    format!("内存 {} · {} 个进程", mib(app.memory_bytes), app.processes),
                    format!("CPU 平均 {:.1}%", app.cpu_percent),
                ])),
            );
            if !terms.is_empty() && !folded {
                let mut list = div()
                    .pb(px(T::S6))
                    .border_b(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()));
                for (t, title) in terms {
                    let id = t.id.clone();
                    let share = (t.memory_bytes as f64 / GB / max).min(1.) as f32;
                    list = list.child(
                        div()
                            .id(gpui::SharedString::from(format!("diag-memterm-{id}")))
                            .flex()
                            .items_center()
                            .gap(px(T::S10))
                            .h(px(T::ROW - T::S6))
                            .cursor_pointer()
                            .hover(|s| s.bg(T::color(T::raised())))
                            .child(
                                div()
                                    .w(px(T::BAR_LABEL))
                                    .flex_shrink_0()
                                    .pl(px(T::S12))
                                    .flex()
                                    .items_center()
                                    .gap(px(T::S6))
                                    .child(
                                        div()
                                            .size(px(T::S6))
                                            .flex_shrink_0()
                                            .rounded_full()
                                            .bg(T::color(color)),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(T::SMALL))
                                            .text_color(T::color(T::secondary()))
                                            .child(title.clone()),
                                    ),
                            )
                            .child(
                                div().flex_1().min_w_0().relative().h_full().child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top(px((T::ROW - T::S6 - T::S4) / 2.))
                                        .h(px(T::S4))
                                        .min_w(px(T::S2))
                                        .w(gpui::relative(share))
                                        .rounded_r(px(T::CHIP_RADIUS))
                                        .bg(T::color(color).opacity(0.6)),
                                ),
                            )
                            .child(
                                div()
                                    .w(px(T::BAR_VALUE))
                                    .flex_shrink_0()
                                    .text_right()
                                    .text_size(px(T::SMALL))
                                    .text_color(T::color(T::secondary()))
                                    .child(format!("{} ↗", mib(t.memory_bytes))),
                            )
                            .tooltip(tip(vec![
                                title,
                                format!(
                                    "整棵进程树 {} · {} 个进程",
                                    mib(t.memory_bytes),
                                    t.processes
                                ),
                                "含 Agent 拉起的 node、MCP 等子进程".into(),
                                "点击跳到这个终端".into(),
                            ]))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(DiagnosticAction::Terminal(id.clone()))
                            })),
                    );
                }
                column = column.child(
                    list.child(
                        div()
                            .pl(px(T::S12))
                            .pt(px(T::S4))
                            .text_size(px(T::META))
                            .text_color(T::color(T::muted()))
                            .child("按终端算整棵进程树，含它拉起的子进程；和上面应用的数不相加。"),
                    ),
                );
            }
        }
        if ranked.len() > first {
            column = column.child(
                button(
                    "diagnostic-memory-more",
                    if self.all_memory_rows {
                        "收起".to_owned()
                    } else {
                        format!("再显示 {} 项", ranked.len() - first)
                    },
                )
                .mt(px(T::S6))
                .px(px(T::S8))
                .text_color(T::color(T::link()))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.all_memory_rows = !this.all_memory_rows;
                    cx.notify();
                })),
            );
        }
        if ranked.is_empty() {
            column = column.child(label("这次没有读到应用的内存。"));
        }
        column
    }
}
