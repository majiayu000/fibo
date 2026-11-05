//! The right column: CPU against memory, and the GPU share.
use super::charts::{axis, axis_max, bar_row, cpu_scale, heading, plot, tick};
use super::*;

impl DiagnosticView {
    /// CPU on a linear x, memory footprint on a log y: tells CPU hogs from memory hogs.
    pub(super) fn scatter(&self, report: &Diagnostics) -> gpui::Div {
        const MB: f64 = 1_048_576.;
        let ranked: Vec<&AppUsage> = report.ranked().collect();
        let (x_max, x_step) = cpu_scale(ranked.first().map_or(0., |a| a.cpu_percent));
        let biggest = ranked
            .iter()
            .map(|a| a.memory_bytes as f64 / MB)
            .fold(8., f64::max);
        // Ticks every power of eight from 8 MB up past the largest app.
        let mut ticks = vec![8.];
        while *ticks.last().unwrap() < biggest {
            ticks.push(ticks.last().unwrap() * 8.);
        }
        // Headroom of one doubling above the largest app, not a whole empty tick band.
        let (lo, hi) = (4f64.log2(), (biggest * 2.).log2().max(16f64.log2()));
        ticks.retain(|mb| mb.log2() <= hi);
        let y = |mb: f64| (1. - (mb.max(4.).log2() - lo) / (hi - lo)) as f32;
        let x = |cpu: f64| (cpu / x_max).min(1.) as f32;
        // Label the three biggest CPU users and the two biggest memory users.
        let mut by_rss = ranked.clone();
        by_rss.sort_by_key(|a| std::cmp::Reverse(a.memory_bytes));
        let labelled: Vec<&str> = ranked
            .iter()
            .take(3)
            .chain(by_rss.iter().take(2))
            .map(|a| a.name.as_str())
            .collect();
        let label_tops = label_tops(
            &ranked
                .iter()
                .filter(|a| labelled.contains(&a.name.as_str()))
                .map(|a| {
                    (
                        a.name.as_str(),
                        x(a.cpu_percent),
                        y(a.memory_bytes as f64 / MB),
                    )
                })
                .collect::<Vec<_>>(),
        );
        let mut area = div().flex_1().relative().h(px(T::SCATTER_HEIGHT));
        let mut gutter = div()
            .w(px(T::AXIS_GUTTER))
            .flex_shrink_0()
            .relative()
            .h(px(T::SCATTER_HEIGHT));
        for mb in &ticks {
            let at = y(*mb);
            area = area.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(gpui::relative(at))
                    .h(px(T::BORDER_WIDTH))
                    .bg(T::color(T::chart_grid())),
            );
            gutter = gutter.child(
                div()
                    .absolute()
                    .right(px(T::S6))
                    .top(gpui::relative(at))
                    .mt(px(-T::S8))
                    .whitespace_nowrap()
                    .child(if *mb >= 1024. {
                        format!("{:.0} GB", mb / 1024.)
                    } else {
                        format!("{mb:.0} MB")
                    }),
            );
        }
        let mut v = x_step;
        while v <= x_max + 0.01 {
            let line = div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(gpui::relative((v / x_max) as f32));
            area = area.child(if v == 100. {
                line.border_l(px(T::BORDER_WIDTH))
                    .border_dashed()
                    .border_color(T::color(T::chart_ref()))
            } else {
                line.w(px(T::BORDER_WIDTH)).bg(T::color(T::chart_grid()))
            });
            v += x_step;
        }
        if 100. % x_step != 0. {
            area = area.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(gpui::relative((100. / x_max) as f32))
                    .border_l(px(T::BORDER_WIDTH))
                    .border_dashed()
                    .border_color(T::color(T::chart_ref())),
            );
        }
        for app in &ranked {
            let (px_, py_) = (x(app.cpu_percent), y(app.memory_bytes as f64 / MB));
            let named = labelled.contains(&app.name.as_str());
            area = area.child(
                div()
                    .id(gpui::SharedString::from(format!("diag-pt-{}", app.name)))
                    .absolute()
                    .left(gpui::relative(px_))
                    .top(gpui::relative(py_))
                    .ml(px(-T::POINT / 2.))
                    .mt(px(-T::POINT / 2.))
                    .size(px(T::POINT))
                    .rounded_full()
                    .border(px(T::S2))
                    .border_color(T::color(T::popover()))
                    .bg(T::color(if named { T::primary() } else { T::series(0) }))
                    .tooltip(tip(vec![
                        app.name.clone(),
                        format!(
                            "CPU {:.1}% · 内存 {}",
                            app.cpu_percent,
                            mib(app.memory_bytes)
                        ),
                        format!("{} 个进程", app.processes),
                    ])),
            );
            if let Some(top) = label_tops.get(app.name.as_str()) {
                let text = div()
                    .absolute()
                    .top(px(*top))
                    .whitespace_nowrap()
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::body()))
                    .child(app.name.clone());
                area = area.child(if px_ > 0.6 {
                    text.right(gpui::relative(1. - px_)).mr(px(T::S8))
                } else {
                    text.left(gpui::relative(px_)).ml(px(T::S8))
                });
            }
        }
        let mut x_labels = div().flex_1().relative().h(px(T::TICK_ROW));
        let mut v = 0.;
        while v <= x_max + 0.01 {
            let text = if v == 100. {
                "1 核".to_owned()
            } else {
                format!("{v:.0}%")
            };
            x_labels = x_labels.child(tick(text, (v / x_max) as f32));
            v += x_step;
        }
        div()
            .child(heading("CPU × 内存", "纵轴为对数 · 内存为活动监视器口径"))
            .child(
                div()
                    .flex()
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .child(gutter)
                    .child(
                        area.border_l(px(T::BORDER_WIDTH))
                            .border_b(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::control())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .mt(px(T::S4))
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .child(div().w(px(T::AXIS_GUTTER)).flex_shrink_0())
                    .child(x_labels),
            )
    }
    pub(super) fn gpu_ranking(&self, report: &Diagnostics) -> gpui::Div {
        let apps = report.gpu_ranked();
        let mut section = div().child(heading("GPU 活动份额", "已观测计数的占比"));
        if apps.is_empty() {
            return section.child(label("没有可比较的 GPU 活动数据，不代表 GPU 空闲。"));
        }
        let max = axis_max(apps[0].gpu_share_percent.unwrap_or_default(), 20., 20.);
        // The column is narrow: two intervals keep the labels apart.
        let step = max / 2.;
        let width = T::BAR_LABEL - T::S20;
        section = section.child(axis(max, step, None, width));
        for app in apps {
            let share = app.gpu_share_percent.unwrap_or_default();
            let value = if share > 0. && share < 0.1 {
                "<0.1%".into()
            } else {
                format!("{share:.1}%")
            };
            section = section.child(
                bar_row(
                    format!("diag-gpu-{}", app.name).into(),
                    &app.name,
                    div(),
                    plot(share, max, step, None, T::series(0)),
                    value,
                    width,
                )
                .tooltip(tip(vec![
                    app.name.clone(),
                    format!("已观测 GPU 活动的 {share:.1}%"),
                    "不是设备利用率".into(),
                ])),
            );
        }
        section.child(
            div()
                .mt(px(T::S10))
                .text_size(px(T::META))
                .text_color(T::color(T::muted()))
                .child("只统计驱动公开的计数增量；没有读数不代表占用为零。"),
        )
    }
}
