//! Small pieces of the palette's list.
use super::*;

pub(super) fn section_heading(title: &'static str, note: &str) -> gpui::Div {
    div()
        .px(px(T::S16))
        .pt(px(T::S10))
        .pb(px(T::S4))
        .flex()
        .gap(px(T::S6))
        .text_size(px(T::META))
        .text_color(T::color(T::muted()))
        .font_features(T::tabular())
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(T::color(T::secondary()))
                .child(title),
        )
        .child(div().flex_1())
        .child(note.to_owned())
}
pub(super) fn glyph(text: &'static str) -> gpui::AnyElement {
    div()
        .text_color(T::color(T::secondary()))
        .child(text)
        .into_any_element()
}
pub(in crate::workbench) fn kbd(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .flex_shrink_0()
        .px(px(T::S4))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::control()))
        .rounded(px(T::CHIP_RADIUS))
        .text_size(px(T::CHIP_SIZE))
        .text_color(T::color(T::secondary()))
        .child(text.into())
}
