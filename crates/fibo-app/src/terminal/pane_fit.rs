//! A read-only pane shows a grid another window sized. Drawn at the saved text size it
//! leaves the rest of this pane empty (or is cut off); it draws at the size that fills
//! the pane instead. The grid itself is untouched: only the window typing sets it.
use super::Pane;
use crate::terminal::screen::{self, CellMetrics};
use gpui::{Font, Pixels, Size, Window};

pub(super) struct Fit {
    key: (Option<(u16, u16)>, Size<Pixels>, CellMetrics),
    drawn: Option<(f32, CellMetrics)>,
}

impl Pane {
    /// The text size to draw this frame at (None: the saved one) and its cells.
    pub(super) fn fit_metrics(
        &mut self,
        base: CellMetrics,
        grid: Option<(u16, u16)>,
        room: Size<Pixels>,
        font: &Font,
        window: &Window,
    ) -> (Option<f32>, CellMetrics) {
        let key = (grid, room, base);
        if self.fit.as_ref().is_none_or(|f| f.key != key) {
            let drawn = grid.and_then(|grid| {
                fitted(screen::font_size(), base, grid, room, |size| {
                    let _drawn = screen::DrawnSize::set(Some(size));
                    CellMetrics::measure(font, window)
                })
            });
            self.fit = Some(Fit { key, drawn });
        }
        match self.fit.as_ref().and_then(|f| f.drawn) {
            Some((size, metrics)) => (Some(size), metrics),
            None => (None, base),
        }
    }
}

/// The largest size, in half points, whose cells put `grid` inside `room`; None when
/// that is the saved size. Cells round to backing pixels, so a size is measured before
/// it is taken rather than assumed to scale with the text.
fn fitted(
    saved: f32,
    base: CellMetrics,
    (cols, rows): (u16, u16),
    room: Size<Pixels>,
    measure: impl Fn(f32) -> Option<CellMetrics>,
) -> Option<(f32, CellMetrics)> {
    let need = |m: &CellMetrics| (m.width * f32::from(cols), m.height * f32::from(rows));
    let (w, h) = need(&base);
    if cols == 0 || rows == 0 || w <= Pixels::ZERO || h <= Pixels::ZERO {
        return None;
    }
    let ratio = (room.width / w).min(room.height / h);
    let (low, high) = screen::SIZE_RANGE;
    let mut size = ((saved * ratio * 2.).floor() / 2.).clamp(low, high);
    loop {
        if (size - saved).abs() < 0.25 {
            return None;
        }
        let metrics = measure(size)?;
        let (w, h) = need(&metrics);
        if (w <= room.width && h <= room.height) || size <= low {
            return Some((size, metrics));
        }
        size -= 0.5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};

    /// Cells proportional to the text size, each rounded up to a whole pixel.
    fn cells(text: f32) -> CellMetrics {
        CellMetrics {
            width: px((text * 0.6).ceil()),
            advance: px(text * 0.6),
            height: px((text * 1.2).ceil()),
            baseline: px(text),
            x_height: px(text / 2.),
            stroke: px(1.),
            ic_width: px(text),
        }
    }

    #[test]
    fn a_grid_narrower_than_the_pane_is_drawn_larger_and_still_fits() {
        let room = size(px(1000.), px(900.));
        let (text, m) = fitted(13., cells(13.), (80, 30), room, |s| Some(cells(s))).unwrap();
        assert!(text > 13., "drawn at {text}");
        assert!(m.width * 80. <= room.width && m.height * 30. <= room.height);
        // Half a point more would no longer fit.
        let next = cells(text + 0.5);
        assert!(next.width * 80. > room.width || next.height * 30. > room.height);
    }

    #[test]
    fn a_grid_wider_than_the_pane_is_drawn_smaller_rather_than_cut_off() {
        let room = size(px(400.), px(900.));
        let (text, m) = fitted(13., cells(13.), (80, 30), room, |s| Some(cells(s))).unwrap();
        assert!(text < 13.);
        assert!(m.width * 80. <= room.width);
    }

    #[test]
    fn a_grid_that_already_fits_keeps_the_saved_size() {
        let base = cells(13.);
        let room = size(base.width * 80. + px(3.), base.height * 30. + px(3.));
        assert!(fitted(13., base, (80, 30), room, |s| Some(cells(s))).is_none());
    }
}
