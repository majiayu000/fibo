//! A read-only pane preserves the terminal's grid. Enlarge it when the whole grid
//! fits; otherwise keep the saved text size and let the viewer scroll the grid.
use super::Pane;
use crate::terminal::screen::{self, CellMetrics};
use crate::ui::theme::Theme as T;
use gpui::{Along, Axis, Bounds, Font, Pixels, Point, Size, Window, fill, point, px, size};

pub(super) struct Fit {
    key: (Option<(u16, u16)>, Size<Pixels>, CellMetrics, f32),
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
        let key = (grid, room, base, window.scale_factor());
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
    let mut size = ((saved * ratio * 2.).floor() / 2.).min(screen::SIZE_RANGE.1);
    while size >= saved + 0.5 {
        let metrics = measure(size)?;
        let (w, h) = need(&metrics);
        if w <= room.width && h <= room.height {
            return Some((size, metrics));
        }
        size -= 0.5;
    }
    None
}

const BAR: Pixels = px(10.);

/// The offset within a grid that does not fit at the user's text size. This never
/// changes the PTY, and is cleared as soon as the pane becomes writable.
#[derive(Default)]
pub(super) struct Pan {
    offset: Point<Pixels>,
    limit: Point<Pixels>,
    pub(super) dragging: Option<Axis>,
}

impl Pan {
    pub(super) fn layout(
        &mut self,
        grid: Option<(u16, u16)>,
        room: Size<Pixels>,
        metrics: CellMetrics,
    ) {
        if grid.is_none() && self.limit == Point::default() {
            return;
        }
        let at_bottom = self.offset.y == self.limit.y;
        self.limit = grid.map_or_else(Point::default, |(cols, rows)| {
            point(
                (metrics.width * f32::from(cols) - room.width).max(Pixels::ZERO),
                (metrics.height * f32::from(rows) - room.height).max(Pixels::ZERO),
            )
        });
        self.offset.x = self.offset.x.min(self.limit.x);
        self.offset.y = if at_bottom {
            self.limit.y
        } else {
            self.offset.y.min(self.limit.y)
        };
        self.dragging = self.dragging.filter(|axis| self.overflows(*axis));
    }

    pub(super) fn overflows(&self, axis: Axis) -> bool {
        self.limit.along(axis) > Pixels::ZERO
    }

    pub(super) fn bounds(&self, view: Bounds<Pixels>, scale: f32) -> Bounds<Pixels> {
        let offset = self
            .offset
            .map(|v| px((f32::from(v) * scale).round() / scale));
        Bounds::new(
            view.origin - offset,
            size(
                view.size.width + self.limit.x,
                view.size.height + self.limit.y,
            ),
        )
    }

    pub(super) fn scroll(&mut self, axis: Axis, delta: Pixels) -> bool {
        let old = self.offset;
        self.offset = old.apply_along(axis, |value| {
            (value + delta).clamp(Pixels::ZERO, self.limit.along(axis))
        });
        old != self.offset
    }

    pub(super) fn reveal(&mut self, room: Size<Pixels>, cell: Point<Pixels>) {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let location = cell.along(axis) - self.offset.along(axis);
            let span = room.along(axis);
            if location < Pixels::ZERO || location >= span {
                self.scroll(axis, location - span / 2.);
            }
        }
    }

    fn bar(&self, view: Bounds<Pixels>, axis: Axis) -> Option<(Bounds<Pixels>, Pixels)> {
        let overflow = self.limit.along(axis);
        let length = view.size.along(axis);
        let cross = axis.invert();
        if overflow <= Pixels::ZERO || length <= BAR * 2. || view.size.along(cross) <= BAR {
            return None;
        }
        let track = Bounds::new(
            view.origin
                .apply_along(cross, |start| start + view.size.along(cross) - BAR),
            view.size
                .apply_along(cross, |_| BAR)
                .apply_along(axis, |length| length - BAR),
        );
        let track_length = track.size.along(axis);
        let thumb =
            (track_length * (length / (length + overflow))).clamp(BAR, track_length - px(1.));
        Some((track, thumb))
    }

    pub(super) fn bar_at(&self, view: Bounds<Pixels>, at: Point<Pixels>) -> Option<Axis> {
        [Axis::Horizontal, Axis::Vertical]
            .into_iter()
            .find(|&axis| {
                self.bar(view, axis)
                    .is_some_and(|(track, _)| track.contains(&at))
            })
    }

    pub(super) fn drag_to(&mut self, view: Bounds<Pixels>, at: Point<Pixels>, axis: Axis) {
        let Some((track, thumb)) = self.bar(view, axis) else {
            return;
        };
        let position = at.along(axis) - track.origin.along(axis) - thumb / 2.;
        let progress = (position / (track.size.along(axis) - thumb)).clamp(0., 1.);
        self.offset = self
            .offset
            .apply_along(axis, |_| self.limit.along(axis) * progress);
    }

    pub(super) fn paint(&self, view: Bounds<Pixels>, window: &mut Window) {
        let color = T::color(T::secondary());
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let Some((track, thumb)) = self.bar(view, axis) else {
                continue;
            };
            let progress = self.offset.along(axis) / self.limit.along(axis);
            let start = (track.size.along(axis) - thumb) * progress;
            let handle = Bounds::new(
                track
                    .origin
                    .apply_along(axis, |origin| origin + start)
                    .apply_along(axis.invert(), |origin| origin + px(2.)),
                track
                    .size
                    .apply_along(axis, |_| thumb)
                    .apply_along(axis.invert(), |_| BAR - px(4.)),
            );
            window.paint_quad(fill(track, color.opacity(0.1)));
            window.paint_quad(fill(handle, color.opacity(0.65)).corner_radii(px(3.)));
        }
    }
}

impl Pane {
    /// Translate cell coordinates together with the paint, hit tests and text selection.
    pub(super) fn grid_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds.map(|view| self.pan.bounds(view, self.scale))
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
    fn an_oversize_grid_uses_the_saved_size() {
        let base = cells(13.);
        let grid = (127, 34);
        for room in [size(px(500.), px(900.)), size(px(1200.), px(200.))] {
            assert!(fitted(13., base, grid, room, |_| panic!("measured a smaller size")).is_none());
        }
    }

    #[test]
    fn a_read_only_viewport_can_reach_every_edge_and_clears_on_takeover() {
        let view = Bounds::new(point(px(30.), px(40.)), size(px(500.), px(320.)));
        let metrics = cells(13.);
        let mut pan = Pan::default();
        pan.layout(Some((127, 34)), view.size, metrics);
        let end = pan.bounds(view, 2.);
        assert_eq!(end.bottom(), view.bottom());
        assert_eq!(end.left(), view.left());
        assert!(pan.scroll(Axis::Horizontal, px(1000.)));
        assert!(pan.scroll(Axis::Vertical, px(-1000.)));
        let right_top = pan.bounds(view, 2.);
        assert_eq!(right_top.right(), view.right());
        assert_eq!(right_top.top(), view.top());
        assert!(!pan.scroll(Axis::Horizontal, px(100.)));
        pan.layout(None, view.size, metrics);
        assert_eq!(pan.bounds(view, 2.), view);
    }

    #[test]
    fn a_grid_that_already_fits_keeps_the_saved_size() {
        let base = cells(13.);
        let room = size(base.width * 80. + px(3.), base.height * 30. + px(3.));
        assert!(fitted(13., base, (80, 30), room, |s| Some(cells(s))).is_none());
    }
}
