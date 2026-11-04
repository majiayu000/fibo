//! What a terminal pane paints over or around its grid.
use super::screen::{self, CellMetrics};
use crate::ui::{
    terminal_text::{CursorDefault, TerminalText, padding_of},
    theme::{Theme as T, hsla},
};
use gpui::{Bounds, Pixels, Window, px};
use mooring_core::terminal::libghostty_vt::terminal::CursorStyle;

pub(super) fn cursor_style(shape: CursorDefault) -> CursorStyle {
    match shape {
        CursorDefault::Block => CursorStyle::Block,
        CursorDefault::Bar => CursorStyle::Bar,
        CursorDefault::Underline => CursorStyle::Underline,
    }
}
/// The stronger of the two fades. Ghostty's unfocused-split-opacity: the program's own
/// cursor (Claude and Codex draw one as reversed text) recedes with the rest.
pub(super) fn dim(look: &TerminalText, unfocused_split: bool, background_window: bool) -> f32 {
    let split = if unfocused_split {
        look.unfocused_dim
    } else {
        0.
    };
    let window = if background_window {
        look.window_dim
    } else {
        0.
    };
    split.max(window)
}
/// Where the cells go inside the pane, the margin painted as the terminal's own ground. A
/// peek keeps the terminal's exact size, so it has no margin.
pub(super) fn inset(
    bounds: Bounds<Pixels>,
    sc: &screen::Screen,
    peek: bool,
    window: &mut Window,
) -> Bounds<Pixels> {
    let padding = padding_of();
    if peek || padding <= 0. {
        return bounds;
    }
    window.paint_quad(gpui::fill(bounds, hsla(sc.background)));
    let margin = px(padding);
    Bounds::new(
        bounds.origin + gpui::point(margin, margin),
        gpui::size(
            (bounds.size.width - margin * 2.).max(px(1.)),
            (bounds.size.height - margin * 2.).max(px(1.)),
        ),
    )
}
pub(super) fn selection(
    sc: &screen::Screen,
    bounds: Bounds<Pixels>,
    metrics: CellMetrics,
    window: &mut Window,
) {
    for (row, cols) in sc.selection.iter().enumerate() {
        let Some((left, right)) = *cols else { continue };
        window.paint_quad(gpui::fill(
            Bounds::new(
                gpui::point(
                    bounds.left() + metrics.width * left as f32,
                    bounds.top() + metrics.height * row as f32,
                ),
                gpui::size(
                    metrics.width * (right + 1).saturating_sub(left) as f32,
                    metrics.height,
                ),
            ),
            T::terminal_selection(),
        ));
    }
}
/// Matches in view, the current one stronger; drawn over the text, so translucent, the
/// current one also outlined. The amber T::matched() washes into the popover; over
/// terminal text the pure hue is needed to stay visible in both schemes.
pub(super) fn matches(
    found: &[mooring_core::terminal::SearchMatch],
    current: Option<usize>,
    (top, len): (usize, usize),
    bounds: Bounds<Pixels>,
    metrics: CellMetrics,
    window: &mut Window,
) {
    let matched = hsla(T::SEARCH_HIT);
    for (i, m) in found.iter().enumerate() {
        if m.row < top || m.row >= top + len {
            continue;
        }
        let is_current = current == Some(i);
        let at = Bounds::new(
            gpui::point(
                bounds.left() + metrics.width * m.col as f32,
                bounds.top() + metrics.height * (m.row - top) as f32,
            ),
            gpui::size(metrics.width * m.cols as f32, metrics.height),
        );
        window.paint_quad(gpui::fill(
            at,
            matched.opacity(if is_current { 0.55 } else { 0.25 }),
        ));
        if is_current {
            window.paint_quad(gpui::outline(at, matched, gpui::BorderStyle::Solid));
        }
    }
}
