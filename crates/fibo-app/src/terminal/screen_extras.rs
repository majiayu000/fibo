//! What the grid draws past its cells' own glyphs, moved here to keep `screen` readable:
//! decorations, the selection's text, the cursor row's band, and the contrast floor.
use super::*;
use crate::ui::theme::Theme;

/// Underline, strikethrough and overline drawn on the cell grid, like Ghostty's sprites,
/// so adjacent runs join and every SGR 4 variant has its own shape.
pub(super) fn paint_decorations(
    run: &Run,
    origin: gpui::Point<Pixels>,
    m: CellMetrics,
    window: &mut Window,
) {
    let a = run.attrs;
    let w = m.width * run.cols as f32;
    let t = m.stroke;
    let color = text_color(run);
    let mut line = |y: Pixels, x: Pixels, len: Pixels, color: Hsla| {
        window.paint_quad(fill(
            Bounds::new(point(origin.x + x, origin.y + y), size(len, t)),
            color,
        ));
    };
    if a.underline != Underline::None {
        let color = a
            .underline_color
            .or(crate::ui::theme::Theme::palette().term_underline)
            .map(|c| hsla(c).opacity(color.a))
            .unwrap_or(color);
        // Halfway between the baseline and the cell bottom, clear of descenders.
        let y = (m.baseline + (m.height - m.baseline) / 2. - t / 2.).min(m.height - t);
        match a.underline {
            Underline::Single => line(y, px(0.), w, color),
            Underline::Double => {
                let y = (y - t).max(m.baseline);
                line(y, px(0.), w, color);
                line(y + t * 2., px(0.), w, color);
            }
            Underline::Dotted | Underline::Dashed => {
                let (on, period) = if a.underline == Underline::Dotted {
                    (t, t * 2.)
                } else {
                    (m.width / 2., m.width)
                };
                let mut x = px(0.);
                while x < w {
                    line(y, x, on.min(w - x), color);
                    x += period;
                }
            }
            Underline::Curly => {
                let amp = (m.height - y - t).max(t) / 2.;
                let steps = (f32::from(w) / f32::from(t)).ceil() as usize;
                for i in 0..steps {
                    let x = t * i as f32;
                    let phase = f32::from(x) / f32::from(m.width) * std::f32::consts::TAU;
                    line(y + amp * phase.sin(), x, t.min(w - x), color);
                }
            }
            Underline::None => {}
        }
    }
    if a.strikethrough {
        line(m.baseline - m.x_height / 2. - t / 2., px(0.), w, color);
    }
    if a.overline {
        line(px(0.), px(0.), w, color);
    }
}

/// A theme's color for selected text, over the selection. Only runs a selection touches,
/// and only for a theme that sets the color, so ordinary drawing never comes here.
#[allow(clippy::too_many_arguments)]
pub fn paint_selected_text(
    sc: &Screen,
    f: &Font,
    bounds: Bounds<Pixels>,
    metrics: CellMetrics,
    cache: &LayoutCache,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(color) = Theme::palette().term_selected_text else {
        return;
    };
    for (y, cols) in sc.selection.iter().enumerate() {
        let (Some((left, right)), Some(row)) = (*cols, sc.rows.get(y)) else {
            continue;
        };
        let oy = bounds.origin.y + metrics.height * y as f32;
        for run in row.iter().filter(|r| !r.attrs.invisible) {
            let (from, to) = (run.col.max(left), (run.col + run.cols - 1).min(right));
            if from > to {
                continue;
            }
            let origin = point(bounds.origin.x + metrics.width * from as f32, oy);
            let printable = printable_cells(run);
            let text = if printable {
                &run.text[usize::from(from - run.col)..=usize::from(to - run.col)]
            } else {
                run.text.as_str()
            };
            if text.trim().is_empty() {
                continue;
            }
            let style = usize::from(run.attrs.bold) | (usize::from(run.attrs.italic) << 1);
            if printable && let Some(Some(glyphs)) = &cache.ascii[style] {
                glyphs.paint(text, origin, metrics, hsla(color), window);
                continue;
            }
            let part = Run {
                col: from,
                cols: to - from + 1,
                text: text.to_owned(),
                fg: color,
                ..run.clone()
            };
            let (line, offset) = shape_cell(&part, run_font(&part, f), metrics, window);
            let _ = line.paint(
                point(origin.x + offset, oy),
                metrics.height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
    }
}
/// The band along the cursor's row, whether or not the cursor is in its blink's off half.
pub fn paint_guide(sc: &Screen, bounds: Bounds<Pixels>, metrics: CellMetrics, window: &mut Window) {
    const STRENGTH: f32 = 0.12;
    if !sc.cursor_visible {
        return;
    }
    let Some((_, y)) = sc.cursor else { return };
    let color = Theme::palette()
        .term_cursor_guide
        .unwrap_or(sc.cursor_color);
    window.paint_quad(fill(
        Bounds::new(
            point(bounds.origin.x, bounds.origin.y + metrics.height * y as f32),
            size(bounds.size.width, metrics.height),
        ),
        hsla(color).opacity(STRENGTH),
    ));
}
/// What lies under a terminal's cells: the theme's picture, if it has one, and how opaque
/// the ground is over it. A see-through theme's tint is the window's own base, so a terminal
/// on the theme's ground paints none; one a program recolored keeps its color, as thin.
pub(super) fn paint_ground(sc: &Screen, bounds: Bounds<Pixels>, window: &mut Window) -> f32 {
    let p = Theme::palette();
    if let Some(backdrop) = p.backdrop
        && let Some(image) = backdrop.picture.image()
    {
        let device = image.size(0);
        let scale = window.scale_factor();
        let natural = size(
            px(device.width.0 as f32 / scale),
            px(device.height.0 as f32 / scale),
        );
        for at in crate::ui::backdrop::placements(backdrop.mode, natural, bounds) {
            let _ = window.paint_image(bounds, at, Default::default(), image.clone(), 0, false);
        }
        return backdrop.ground();
    }
    match p.opacity {
        100 => 1.,
        _ if sc.background == p.term.bg => 0.,
        percent => f32::from(percent) / 100.,
    }
}
/// WCAG relative luminance over 0–1.
fn luminance(color: u32) -> f32 {
    fibo_core::terminal::luminance(color)
}
fn contrast(a: u32, b: u32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn mix_to(from: u32, to: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let (a, b) = (((from >> shift) & 255) as f32, ((to >> shift) & 255) as f32);
        ((a + (b - a) * t).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}
/// `fg` moved no further than it must toward white or black, whichever can reach `min`
/// against `bg`; when neither can, the one that gets closest.
pub(super) fn raise_contrast(fg: u32, bg: u32, min: f32) -> u32 {
    if contrast(fg, bg) >= min {
        return fg;
    }
    let (white, black) = (contrast(0xffffff, bg), contrast(0x000000, bg));
    let toward_white = if luminance(fg) >= luminance(bg) {
        white >= min || white >= black
    } else {
        !(black >= min || black >= white)
    };
    let target = if toward_white { 0xffffff } else { 0x000000 };
    if contrast(target, bg) < min {
        return target;
    }
    let (mut low, mut high) = (0f32, 1f32);
    for _ in 0..8 {
        let middle = (low + high) / 2.;
        if contrast(mix_to(fg, target, middle), bg) >= min {
            high = middle;
        } else {
            low = middle;
        }
    }
    mix_to(fg, target, high)
}
/// The contrast floor for one capture. Off, it is one comparison a cell; on, cells of one
/// style in a row reuse the answer, and it runs only on rows that changed.
pub(super) struct Readable {
    min: f32,
    last: (u32, u32, u32),
}
impl Readable {
    pub(super) fn new(min: f32) -> Self {
        Self {
            min,
            last: (u32::MAX, u32::MAX, 0),
        }
    }
    #[inline]
    pub(super) fn fg(&mut self, fg: u32, bg: u32) -> u32 {
        if self.min <= 1. {
            return fg;
        }
        if (self.last.0, self.last.1) != (fg, bg) {
            self.last = (fg, bg, raise_contrast(fg, bg, self.min));
        }
        self.last.2
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_moves_only_as_far_as_the_floor_needs() {
        assert_eq!(raise_contrast(0xffffff, 0x000000, 4.5), 0xffffff);
        let raised = raise_contrast(0x444444, 0x202020, 4.5);
        assert!(contrast(raised, 0x202020) >= 4.5);
        assert!(contrast(raised, 0x202020) < 4.9, "{raised:x}");
        assert!(luminance(raised) > luminance(0x444444));
        let darker = raise_contrast(0xbbbbbb, 0xffffff, 4.5);
        assert!(contrast(darker, 0xffffff) >= 4.5);
        assert!(luminance(darker) < luminance(0xbbbbbb));
    }
    #[test]
    fn text_that_cannot_reach_the_floor_goes_as_far_as_it_can() {
        // Mid gray reaches 7:1 only toward black.
        let toward = raise_contrast(0x808080, 0x808080, 7.);
        assert_eq!(toward, 0x000000);
        // Nothing reaches 21:1 on mid gray; the darkest or lightest is the best there is.
        assert!(matches!(
            raise_contrast(0x808080, 0x808080, 21.),
            0x000000 | 0xffffff
        ));
    }
    #[test]
    fn off_changes_nothing_and_on_reuses_the_answer() {
        let mut off = Readable::new(1.);
        assert_eq!(off.fg(0x222222, 0x222222), 0x222222);
        let mut on = Readable::new(4.5);
        let first = on.fg(0x444444, 0x202020);
        assert_eq!(on.last, (0x444444, 0x202020, first));
        assert_eq!(on.fg(0x444444, 0x202020), first);
    }
}
