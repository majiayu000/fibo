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
