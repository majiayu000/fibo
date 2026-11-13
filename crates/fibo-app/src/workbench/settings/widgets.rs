//! 设置's controls shared by its pages: cards, segments, switches and key caps.
use super::*;

/// An outlined button: plain enough for a row, still plainly a button.
pub(super) fn secondary(
    id: impl Into<gpui::ElementId>,
    text: impl Into<gpui::SharedString>,
) -> gpui::Stateful<gpui::Div> {
    button(id, text)
        .flex_shrink_0()
        .h(px(SEGMENT_HEIGHT))
        .px(px(T::S10))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::control()))
        .bg(T::color(T::popover()))
        .text_color(T::color(T::primary()))
}
/// A card: rows separated by hairlines, as macOS groups its settings.
pub(super) fn card() -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(T::CARD_RADIUS))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::card_border()))
        .bg(T::color(T::popover()))
        .overflow_hidden()
}
pub(super) fn heading(text: &'static str) -> gpui::Div {
    div()
        .pt(px(T::S20))
        .pb(px(T::S8))
        .px(px(T::S4))
        .text_size(px(T::META))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(T::color(T::secondary()))
        .child(text)
}
/// One segment of a segmented control.
pub(super) fn segment(
    id: impl Into<gpui::ElementId>,
    text: impl Into<gpui::SharedString>,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    let text = text.into();
    div()
        .id(id)
        .role(gpui::accesskit::Role::Button)
        .aria_label(text.clone())
        .h(px(SEGMENT_HEIGHT - T::S4))
        .px(px(T::S10))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(T::CONTROL_RADIUS - T::S2))
        .text_size(px(T::SMALL))
        .cursor_pointer()
        .border(px(T::BORDER_WIDTH))
        .map(|d| {
            if on {
                d.bg(T::color(T::popover()))
                    .border_color(T::color(T::control()))
                    .text_color(T::color(T::primary()))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .shadow_sm()
            } else {
                d.border_color(gpui::transparent_black())
                    .text_color(T::color(T::secondary()))
                    .hover(|s| s.text_color(T::color(T::primary())))
            }
        })
        .child(text)
}
pub(super) fn segments() -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(T::S2))
        .p(px(T::S2))
        .rounded(px(T::CONTROL_RADIUS))
        .bg(T::color(T::chip()))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::border()))
}
/// An on/off switch; the knob sits on the side it means.
pub(super) fn switch(id: impl Into<gpui::ElementId>, on: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::accesskit::Role::Switch)
        .aria_label(if on { t!("已开启") } else { t!("已关闭") })
        .w(px(SWITCH_WIDTH))
        .h(px(SWITCH_HEIGHT))
        .flex_shrink_0()
        .p(px(T::S2))
        .flex()
        .items_center()
        .when(on, |d| d.justify_end())
        .rounded_full()
        .cursor_pointer()
        .bg(T::color(if on { T::accent() } else { T::control() }))
        .child(
            div()
                .size(px(SWITCH_HEIGHT - T::S4))
                .rounded_full()
                .bg(T::color(if on { T::on_accent() } else { T::popover() }))
                .shadow_sm(),
        )
}
/// A chord drawn as a key cap.
pub(super) fn keycap(text: impl Into<gpui::SharedString>, muted: bool) -> gpui::Div {
    div()
        .h(px(SEGMENT_HEIGHT - T::S2))
        .px(px(T::S8))
        .flex()
        .items_center()
        .rounded(px(T::CHIP_RADIUS + T::S2))
        .border(px(T::BORDER_WIDTH))
        .border_b(px(T::BORDER_WIDTH * 2.))
        .border_color(T::color(T::control()))
        .bg(T::color(T::popover()))
        .text_size(px(T::SMALL))
        .text_color(T::color(if muted { T::muted() } else { T::primary() }))
        .child(text.into())
}
