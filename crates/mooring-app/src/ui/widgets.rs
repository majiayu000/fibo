//! Small shared view pieces: buttons, labels, status dots and chips.
use super::theme::{Status, StatusStyle, Theme as T};
use gpui::{div, prelude::*, px};

pub fn badge(group: Status, count: usize) -> gpui::Div {
    let (_, ink, bg) = group.colors();
    div()
        .px(px(T::S4))
        .rounded(px(T::CHIP_RADIUS))
        .text_size(px(T::CHIP_SIZE))
        .text_color(ink)
        .bg(bg)
        .child(count.to_string())
}
pub fn task_icon() -> gpui::Div {
    div()
        .size(px(T::CHIP_SIZE))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(T::S2))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::link()))
        .text_color(T::color(T::link()))
        .text_size(px(T::CHIP_SIZE))
        .child("✓")
}
pub fn button(
    id: impl Into<gpui::ElementId>,
    text: impl Into<gpui::SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let text = text.into();
    div()
        .id(id)
        .role(gpui::accesskit::Role::Button)
        .aria_label(text.clone())
        .h(px(T::ROW))
        .px(px(T::S12))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(T::CONTROL_RADIUS))
        .text_size(px(T::SMALL))
        .text_color(T::color(T::body()))
        .cursor_pointer()
        .hover(|s| s.bg(T::color(T::button_hover())))
        .child(text)
}
/// A square button showing one of the app's SVG marks, tinted like button text.
pub fn icon_button(
    id: impl Into<gpui::ElementId>,
    icon: &'static str,
    label: impl Into<gpui::SharedString>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::accesskit::Role::Button)
        .aria_label(label.into())
        .size(px(T::ROW))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(T::CONTROL_RADIUS))
        .cursor_pointer()
        .hover(|s| s.bg(T::color(T::button_hover())))
        .child(
            gpui::svg()
                .path(icon)
                .size(px(T::ICON_GLYPH))
                .text_color(T::color(T::secondary())),
        )
}
pub fn label(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .text_size(px(T::META))
        .text_color(T::color(T::secondary()))
        .child(text.into())
}
/// The terminal style's status: one symbol in the state's ink, as a multiplexer would.
pub fn glyph(group: Status) -> gpui::Div {
    let symbol = group.glyph();
    div()
        .flex_shrink_0()
        .text_color(group.colors().1)
        .child(symbol)
}
/// The v3 mark beside a session in the sidebar and on its tab: a filled dot when it
/// waits on you, a ring for a turn to check, a quarter arc on a pale ring while it runs,
/// a gray ring once it stopped, and ✓ when you marked it done.
pub fn status_mark(group: Status) -> gpui::AnyElement {
    status_mark_seen(group, false)
}
/// The sidebar's mark: a finished turn you have not looked at yet is a filled green dot;
/// once seen it turns back into 等你验收's ring.
pub fn status_mark_seen(group: Status, unseen: bool) -> gpui::AnyElement {
    const SIZE: f32 = 10.;
    let hue = group.colors().0;
    let icon = |path: &'static str, color: gpui::Hsla| {
        gpui::svg().path(path).size(px(SIZE)).text_color(color)
    };
    match group {
        Status::Waiting | Status::Error => div()
            .size(px(T::DOT))
            .flex_shrink_0()
            .rounded_full()
            .bg(hue)
            .into_any_element(),
        Status::Running | Status::Background => div()
            .relative()
            .size(px(SIZE))
            .flex_shrink_0()
            .child(icon("icons/ring.svg", hue.opacity(0.25)))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .child(icon("icons/arc.svg", hue)),
            )
            .into_any_element(),
        Status::Review if unseen => div()
            .size(px(T::DOT))
            .flex_shrink_0()
            .rounded_full()
            .bg(hue)
            .into_any_element(),
        Status::Review => icon("icons/ring.svg", hue).into_any_element(),
        Status::Closed => glyph(group).into_any_element(),
        _ => icon("icons/ring.svg", T::color(T::muted())).into_any_element(),
    }
}
pub fn dot(group: Status) -> gpui::Div {
    div()
        .size(px(T::DOT))
        .flex_shrink_0()
        .rounded_full()
        .bg(group.colors().0)
}
/// Running work keeps its filled status color even after it has been viewed.
/// Other seen rows follow `colors.json`'s `seen.dot` style.
pub fn seen_dot(group: Status) -> gpui::Div {
    use super::status_colors::{SeenDot, StatusColors};
    if group.working() {
        return dot(group);
    }
    let hue = group.colors().0;
    match StatusColors::get().seen_dot {
        SeenDot::Fill => dot(group),
        SeenDot::Fade => dot(group).bg(hue.opacity(0.45)),
        SeenDot::Ring => div()
            .size(px(T::DOT))
            .flex_shrink_0()
            .rounded_full()
            .border(px(T::CARET_WIDTH))
            .border_color(hue),
    }
}
pub fn chip(group: Status) -> gpui::Div {
    div()
        .px(px(T::S6))
        .py(px(T::BORDER_WIDTH))
        .rounded(px(T::CHIP_RADIUS))
        .text_size(px(T::CHIP_SIZE))
        .text_color(group.colors().1)
        .bg(group.colors().2)
        .child(group.label())
}

/// A row of thin ticks, as on the diagnostics panels and the sidebar's quota rows: the used
/// share is filled in whole ticks, the rest stays drawn in the grid color so what is left
/// reads at a glance.
pub fn gauge(share: f64, fill: gpui::Hsla, height: f32) -> gpui::Canvas<()> {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let ticks = (f32::from(bounds.size.width) / T::GAUGE_PITCH).floor() as usize;
            let filled = (ticks as f64 * share.clamp(0., 1.)).round() as usize;
            let empty = T::color(T::chart_grid());
            for i in 0..ticks {
                let at = bounds.origin + gpui::point(px(i as f32 * T::GAUGE_PITCH), px(0.));
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(at, gpui::size(px(T::GAUGE_TICK), bounds.size.height)),
                    if i < filled { fill } else { empty },
                ));
            }
        },
    )
    .w_full()
    .h(px(height))
}
