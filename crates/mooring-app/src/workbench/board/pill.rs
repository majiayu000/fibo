//! The board's rounded labels.
use super::{PILL, T};
use gpui::{Hsla, ParentElement, Styled, div, prelude::FluentBuilder, px};

/// A rounded label with an outline in its own hue and an optional dot.
pub(super) fn pill(
    text: impl Into<gpui::SharedString>,
    edge: Hsla,
    dot: Option<Hsla>,
    ink: Hsla,
) -> gpui::Div {
    div()
        .h(px(PILL))
        .px(px(T::S8))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(T::BOARD_PILL_GAP))
        .rounded(px(PILL / 2.))
        .border(px(T::BORDER_WIDTH))
        .border_color(edge)
        .text_size(px(T::META))
        .text_color(ink)
        .when_some(dot, |d, c| {
            d.child(
                div()
                    .size(px(T::BOARD_PILL_DOT))
                    .rounded_full()
                    .bg(c)
                    .flex_shrink_0(),
            )
        })
        .child(
            div()
                .max_w(px(T::BOARD_PILL_MAX))
                .truncate()
                .child(text.into()),
        )
}

/// A pill for a name of any length (a branch, a machine): it gives up width to what
/// shares its row and truncates, so a narrow card never pushes it past its own edge.
pub(super) fn name_pill(
    text: impl Into<gpui::SharedString>,
    edge: Hsla,
    dot: Option<Hsla>,
    ink: Hsla,
) -> gpui::Div {
    pill(text, edge, dot, ink).flex_shrink_1().min_w_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shrink(d: gpui::Div) -> Option<f32> {
        let mut d = d;
        d.style().flex_shrink
    }

    /// Two fixed pills and a long branch overflowed the running card (11-04): the name
    /// is the one that gives way, the time and status next to it stay whole.
    #[test]
    fn only_a_name_gives_up_width() {
        let c = gpui::black();
        assert_eq!(shrink(pill("5:29", c, None, c)), Some(0.));
        let mut name = name_pill("consolidate-local-sessions", c, None, c);
        assert_eq!(name.style().flex_shrink, Some(1.));
        assert_eq!(
            name.style().min_size.width,
            Some(gpui::Length::Definite(px(0.).into())),
            "without a zero minimum, flex stops at the content's width"
        );
    }
}
