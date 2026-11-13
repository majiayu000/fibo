//! Drawing parts the diagnostic charts share: axes, bars, stacks, panels and chips.
use super::*;

/// The memory axis end and step in GB: the smallest of 1, 2, 5, 10 … GB that needs at most
/// four intervals, since the memory ranking shares the narrower half of the panel.
pub(super) fn gb_scale(largest: f64) -> (f64, f64) {
    let step = [0.25, 0.5]
        .into_iter()
        .chain(
            [1., 2., 5.]
                .into_iter()
                .cycle()
                .zip((0..).flat_map(|n| [10f64.powi(n); 3]))
                .map(|(k, decade)| k * decade),
        )
        .find(|&step| (largest / step).ceil() <= 4.)
        .unwrap();
    (axis_max(largest, step, step), step)
}
/// A tick gauge filled by several parts in order, for the memory panel.
pub(super) fn parts_gauge(parts: Vec<(f64, gpui::Hsla)>, height: f32) -> gpui::Canvas<()> {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let ticks = (f32::from(bounds.size.width) / T::GAUGE_PITCH).floor() as usize;
            let empty = T::color(T::chart_grid());
            let mut ends = Vec::new();
            let mut sum = 0.;
            for (share, color) in &parts {
                sum += share.clamp(0., 1.);
                ends.push(((ticks as f64 * sum.min(1.)).round() as usize, *color));
            }
            for i in 0..ticks {
                let color = ends.iter().find(|(end, _)| i < *end).map_or(empty, |e| e.1);
                let at = bounds.origin + gpui::point(px(i as f32 * T::GAUGE_PITCH), px(0.));
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(at, gpui::size(px(T::GAUGE_TICK), bounds.size.height)),
                    color,
                ));
            }
        },
    )
    .w_full()
    .h(px(height))
}
/// An axis end that is a round step past the largest value, never below `floor`.
pub(super) fn axis_max(largest: f64, step: f64, floor: f64) -> f64 {
    ((largest / step).ceil() * step).max(floor)
}
/// The CPU axis end and tick step: the smallest of 50 %, 100 %, 200 %, 500 %, 1000 % …
/// that needs at most six intervals, so labels of a 16-core machine never pile up.
pub(super) fn cpu_scale(largest: f64) -> (f64, f64) {
    let steps = [1., 2., 5.]
        .into_iter()
        .cycle()
        .zip((0..).flat_map(|n| [10f64.powi(n); 3]))
        .map(|(k, decade)| k * decade * 100.);
    let step = std::iter::once(50.)
        .chain(steps)
        .find(|&step| (largest / step).ceil() <= 6.)
        .unwrap();
    (axis_max(largest, step, 100.), step)
}
pub(super) fn heading(text: &'static str, aside: &'static str) -> gpui::Div {
    div()
        .flex()
        .items_baseline()
        .justify_between()
        .gap(px(T::S12))
        .mb(px(T::S8))
        .child(
            div()
                .text_color(T::color(T::primary()))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(text),
        )
        .child(
            div()
                .text_size(px(T::META))
                .text_color(T::color(T::muted()))
                .child(aside),
        )
}
/// One of the three top panels: name and percent, three readings, a tick gauge, a hint.
/// Only a nearly full gauge takes a state color; otherwise it stays neutral.
pub(super) fn panel(
    name: &'static str,
    percent: Option<f64>,
    readings: [(&'static str, String); 3],
    hint: String,
    last: bool,
) -> gpui::Div {
    use fibo_core::quota::{NEAR, OVER};
    let group = percent.and_then(|p| {
        if p >= f64::from(OVER) {
            Some(Tone::Danger)
        } else if p >= f64::from(NEAR) {
            Some(Tone::Warn)
        } else {
            None
        }
    });
    let badge = div()
        .px(px(T::S4))
        .rounded(px(T::S2))
        .text_size(px(T::SMALL))
        .child(percent.map_or_else(|| t!("不可读").into(), |p| format!("{p:.0}%")));
    let badge = match group {
        Some(g) => {
            let (_, ink, wash) = g.colors();
            badge.text_color(ink).bg(wash)
        }
        None => badge.text_color(T::color(T::muted())),
    };
    let mut values = div().flex().gap(px(T::S12));
    for (key, value) in readings {
        values = values.child(
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
                        .text_color(T::color(T::primary()))
                        .child(value),
                ),
        );
    }
    let fill = group.map_or_else(|| T::color(T::secondary()), |g| g.colors().0);
    let cell = div()
        .flex_1()
        .min_w_0()
        .px(px(T::S16))
        .py(px(T::S14))
        .flex()
        .flex_col()
        .gap(px(T::S12))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(label(name))
                .child(badge),
        )
        .child(values)
        .child(gauge(percent.unwrap_or(0.) / 100., fill, T::GAUGE_HEIGHT))
        .child(
            div()
                .text_size(px(T::SMALL))
                .text_color(T::color(T::muted()))
                .truncate()
                .child(hint),
        );
    if last {
        cell
    } else {
        cell.border_r(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
    }
}
/// The small square brackets on the panels' outer corners, drawn over their border.
pub(super) fn corners() -> gpui::Canvas<()> {
    gpui::canvas(
        |_, _, _| (),
        |bounds, (), window, _| {
            let color = T::color(T::secondary());
            let (w, h) = (bounds.size.width, bounds.size.height);
            let (arm, line) = (px(T::CORNER), px(T::BORDER_WIDTH));
            for (x, y) in [(px(0.), px(0.)), (w - arm, px(0.)), (px(0.), h - line)]
                .into_iter()
                .chain([(w - arm, h - line)])
            {
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(bounds.origin + gpui::point(x, y), gpui::size(arm, line)),
                    color,
                ));
            }
            for (x, y) in [(px(0.), px(0.)), (w - line, px(0.)), (px(0.), h - arm)]
                .into_iter()
                .chain([(w - line, h - arm)])
            {
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(bounds.origin + gpui::point(x, y), gpui::size(line, arm)),
                    color,
                ));
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
/// An axis label centered on its tick; the two ends align inward so they stay inside.
pub(super) fn tick(text: String, at: f32) -> gpui::Div {
    let label = div().absolute().top_0().whitespace_nowrap().child(text);
    if at <= 0.01 {
        label.left_0()
    } else if at >= 0.99 {
        label.right_0()
    } else {
        label
            .left(gpui::relative(at))
            .ml(px(-T::TICK_LABEL / 2.))
            .w(px(T::TICK_LABEL))
            .flex()
            .justify_center()
    }
}
/// Tick labels over a bar column: the grid shares the bar plot's x scale.
pub(super) fn axis(max: f64, step: f64, reference: Option<f64>, label_width: f32) -> gpui::Div {
    axis_by(max, step, reference, label_width, |v| {
        if Some(v) == reference {
            t!("1 核").to_owned()
        } else {
            format!("{v:.0}%")
        }
    })
}
pub(super) fn axis_by(
    max: f64,
    step: f64,
    reference: Option<f64>,
    label_width: f32,
    text: impl Fn(f64) -> String,
) -> gpui::Div {
    let mut ticks = div().flex_1().relative().h(px(T::TICK_ROW));
    let mut v = 0.;
    while v <= max + 0.01 {
        let text = text(v);
        // The value column to the right leaves room, so the end label centres on its
        // tick like the others instead of leaning into its neighbour.
        let at = (v / max) as f32;
        let mut tick = if at >= 0.99 {
            tick(text, 0.5).left(gpui::relative(1.))
        } else {
            tick(text, at)
        };
        if Some(v) == reference {
            tick = tick
                .text_color(T::color(T::secondary()))
                .font_weight(gpui::FontWeight::SEMIBOLD);
        }
        ticks = ticks.child(tick);
        v += step;
    }
    div()
        .flex()
        .gap(px(T::S10))
        .text_size(px(T::CHIP_SIZE))
        .text_color(T::color(T::muted()))
        .child(div().w(px(label_width)).flex_shrink_0())
        .child(ticks)
        .child(div().w(px(T::BAR_VALUE)).flex_shrink_0())
}
/// One bar on the shared scale, with vertical grid lines and an optional dashed reference.
pub(super) fn plot(
    value: f64,
    max: f64,
    step: f64,
    reference: Option<f64>,
    color: u32,
) -> gpui::Div {
    let mut plot = div().flex_1().min_w_0().relative().h(px(T::ROW));
    let mut v = step;
    while v < max + 0.01 {
        let line = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(gpui::relative((v / max) as f32));
        plot = plot.child(if Some(v) == reference {
            line.border_l(px(T::BORDER_WIDTH))
                .border_dashed()
                .border_color(T::color(T::chart_ref()))
        } else {
            line.w(px(T::BORDER_WIDTH)).bg(T::color(T::chart_grid()))
        });
        v += step;
    }
    if let Some(reference) = reference.filter(|r| r % step != 0.) {
        plot = plot.child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(gpui::relative((reference / max) as f32))
                .border_l(px(T::BORDER_WIDTH))
                .border_dashed()
                .border_color(T::color(T::chart_ref())),
        );
    }
    plot.child(
        div()
            .absolute()
            .left_0()
            .top(px((T::ROW - T::BAR) / 2.))
            .h(px(T::BAR))
            .min_w(px(T::S2))
            .w(gpui::relative((value / max).min(1.) as f32))
            .rounded_r(px(T::CHIP_RADIUS))
            .bg(T::color(color)),
    )
}
pub(super) fn bar_row(
    id: gpui::SharedString,
    name: &str,
    extra: gpui::Div,
    plot: gpui::Div,
    value: String,
    label_width: f32,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(T::S10))
        .min_h(px(T::ROW))
        .border_b(px(T::BORDER_WIDTH))
        .border_color(T::color(T::border()))
        .hover(|s| s.bg(T::color(T::raised())))
        .child(
            div()
                .w(px(label_width))
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(T::S6))
                .overflow_hidden()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(T::color(T::primary()))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(name.to_owned()),
                )
                .child(extra),
        )
        .child(plot)
        .child(
            div()
                .w(px(T::BAR_VALUE))
                .flex_shrink_0()
                .text_right()
                .whitespace_nowrap()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(T::color(T::primary()))
                .child(value),
        )
}
/// One part of a stacked bar and its legend entry.
pub(super) struct Segment {
    name: String,
    value: f64,
    color: u32,
    pub(super) hatched: bool,
    detail: String,
}
impl Segment {
    pub(super) fn new(name: String, value: f64, color: u32) -> Self {
        Self {
            name,
            value,
            color,
            hatched: false,
            detail: String::new(),
        }
    }
    pub(super) fn detail(mut self, detail: String) -> Self {
        self.detail = detail;
        self
    }
}
/// A stacked bar whose widths follow the values, and its four-column legend. `text` gives
/// a value's hover line and its legend number.
pub(super) fn stack_chart(
    prefix: &str,
    segments: Vec<Segment>,
    text: impl Fn(f64) -> (String, String),
) -> (gpui::Div, gpui::Div) {
    let mut stack = div().flex().gap(px(T::S2)).h(px(T::STACK_HEIGHT));
    let mut legend = div()
        .mt(px(T::S10))
        .grid()
        .grid_cols(4)
        .gap_x(px(T::S20))
        .gap_y(px(T::S6));
    let last = segments.len().saturating_sub(1);
    for (i, seg) in segments.into_iter().enumerate() {
        let (color, hatched) = (seg.color, seg.hatched);
        let fill = move |d: gpui::Div| {
            if hatched {
                d.bg(gpui::pattern_slash(T::color(color), T::S2, T::S6))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(color))
            } else {
                d.bg(T::color(color))
            }
        };
        let (hover, number) = text(seg.value);
        let mut lines = vec![seg.name.clone(), hover];
        if !seg.detail.is_empty() {
            lines.push(seg.detail);
        }
        let mut segment = fill(div())
            .id(gpui::SharedString::from(format!("{prefix}-{i}")))
            .h_full()
            .min_w(px(T::STEP_HEIGHT))
            .flex_grow(seg.value as f32)
            .flex_basis(gpui::relative(0.))
            .tooltip(tip(lines));
        if i == 0 {
            segment = segment.rounded_l(px(T::CHIP_RADIUS));
        }
        if i == last {
            segment = segment.rounded_r(px(T::CHIP_RADIUS));
        }
        stack = stack.child(segment);
        legend = legend.child(
            div()
                .flex()
                .items_center()
                .gap(px(T::S6))
                .min_w_0()
                .text_size(px(T::SMALL))
                .child(fill(
                    div().size(px(T::SWATCH)).flex_shrink_0().rounded(px(T::S2)),
                ))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(T::color(T::body()))
                        .child(seg.name),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(T::color(T::secondary()))
                        .child(number),
                ),
        );
    }
    (stack, legend)
}
/// The quiet boxed note under a chart, with a small tag such as 推测 or 说明.
pub(super) fn chart_note(tag: &'static str, text: String) -> gpui::Div {
    div()
        .mt(px(T::S12))
        .px(px(T::S12))
        .py(px(T::S8))
        .rounded(px(T::CONTROL_RADIUS))
        .bg(T::color(T::raised()))
        .flex()
        .gap(px(T::S8))
        .text_size(px(T::SMALL))
        .text_color(T::color(T::secondary()))
        .child(
            div()
                .flex_shrink_0()
                .px(px(T::S6))
                .rounded(px(T::CHIP_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(T::control()))
                .text_size(px(T::CHIP_SIZE))
                .child(tag),
        )
        .child(div().flex_1().min_w_0().child(text))
}
pub(super) fn chip(text: String) -> gpui::Div {
    div()
        .flex_shrink_0()
        .px(px(T::S6))
        .rounded(px(T::CHIP_RADIUS))
        .bg(T::color(T::chip()))
        .text_size(px(T::CHIP_SIZE))
        .text_color(T::color(T::secondary()))
        .child(text)
}
