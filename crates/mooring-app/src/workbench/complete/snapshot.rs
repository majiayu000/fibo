//! What a run keeps of the terminal for the styles that move or erase its text: every
//! character with where it was and where it lands, and the wipe that clears the real grid
//! under them. Every frame is still a pure function of `t` and this.
use super::*;
use crate::terminal::glyph::{self, Face};
use crate::terminal::screen::{Attrs, CellMetrics};
use gpui::App;
use std::collections::HashMap;

/// Above this many characters a packed screen moves one of every n, so a frame stays
/// around what the grid itself draws; the wipe still clears all of them.
const MAX_LETTERS: usize = 4_000;
/// Where the characters leave the grid for the overlay, and the grid is cleared.
pub(super) const LIFT: f32 = 0.1;
/// How tall one character is in the pile, in rows.
const PILE_STEP: f32 = 0.62;
/// The pile never reaches higher than this share of the grid.
const PILE_MAX: f32 = 0.55;

/// One character of the snapshot.
pub(super) struct Letter {
    pub(super) ch: char,
    /// Its cell's top left.
    pub(super) at: Point<Pixels>,
    pub(super) cols: u16,
    pub(super) color: Hsla,
    pub(super) attrs: Attrs,
    /// Where it lands on the pile, by its cell's top left.
    pub(super) pile: Point<Pixels>,
    /// A little of chance, the same on every frame.
    pub(super) jitter: f32,
}
/// A character, its width in cells, and whether it is bold and italic.
type FaceKey = (char, u16, bool, bool);
/// A line of text: its left and right, its top, the color it starts in.
type Line = (Pixels, Pixels, Pixels, Hsla);
/// What the terminal showed when the run started.
pub(super) struct Snapshot {
    pub(super) letters: Vec<Letter>,
    /// All of them in reading order, for the cursor that deletes them: each one's cell's
    /// right edge and row.
    pub(super) reading: Vec<(Pixels, Pixels)>,
    /// Each line's reach and color, the card's miniature of the screen.
    pub(super) lines: Vec<Line>,
    pub(super) grid: Bounds<Pixels>,
    pub(super) cell: Pixels,
    pub(super) row: Pixels,
    /// The line 退格收工 types, and the color of its words.
    pub(super) typed: Vec<(char, u16)>,
    pub(super) fg: Hsla,
    font: gpui::Font,
    metrics: CellMetrics,
    faces: RefCell<HashMap<FaceKey, Option<Face>>>,
}
impl Snapshot {
    /// None for the styles that leave the terminal's text alone.
    pub(super) fn of(style: Style, pane: &Pane, title: Option<&str>, cx: &App) -> Option<Self> {
        if !matches!(
            style,
            Style::Moor | Style::Gravity | Style::Swirl | Style::Backspace
        ) {
            return None;
        }
        let (grid, row, ground) = pane.grid_area()?;
        let metrics = pane.cell_metrics()?;
        let cell = metrics.width;
        let cells = pane.text_cells();
        let reading = cells
            .iter()
            .map(|c| (c.at.x + cell * c.cols.max(1) as f32, c.at.y))
            .collect();
        let mut lines: Vec<Line> = Vec::new();
        for c in &cells {
            let right = c.at.x + cell * c.cols.max(1) as f32;
            match lines.last_mut() {
                Some(l) if l.2 == c.at.y => l.1 = right,
                _ => lines.push((c.at.x, right, c.at.y, c.color)),
            }
        }
        let letters = if matches!(style, Style::Gravity | Style::Swirl) {
            let step = cells.len().div_ceil(MAX_LETTERS).max(1);
            let mut letters: Vec<Letter> = cells
                .into_iter()
                .step_by(step)
                .map(|c| Letter {
                    ch: c.ch,
                    at: c.at,
                    cols: c.cols,
                    color: c.color,
                    attrs: c.attrs,
                    pile: c.at,
                    jitter: 0.,
                })
                .collect();
            pile_up(&mut letters, grid, cell, row);
            letters
        } else {
            vec![]
        };
        let cols = (f32::from(grid.size.width) / f32::from(cell)).floor() as usize;
        let typed = typed_line(title, cols);
        Some(Self {
            letters,
            reading,
            lines,
            grid,
            cell,
            row,
            typed,
            fg: T::color(T::inverse(ground)),
            font: T::terminal_mono(cx),
            metrics,
            faces: RefCell::default(),
        })
    }
    /// A character of the snapshot with its cell's top left at `at`, as the grid draws it.
    pub(super) fn draw(&self, l: &Letter, at: Point<Pixels>, color: Hsla, window: &mut Window) {
        self.draw_char(l.ch, l.cols, l.attrs, at, color, window);
    }
    pub(super) fn draw_char(
        &self,
        ch: char,
        cols: u16,
        attrs: Attrs,
        at: Point<Pixels>,
        color: Hsla,
        window: &mut Window,
    ) {
        let face = *self
            .faces
            .borrow_mut()
            .entry((ch, cols, attrs.bold, attrs.italic))
            .or_insert_with(|| glyph::face_of(ch, cols, attrs, &self.font, self.metrics, window));
        glyph::paint(ch, cols, face, at, self.metrics, color, window);
    }
    /// The middle of a letter's cell, from its top left.
    pub(super) fn middle(&self, l: &Letter, at: Point<Pixels>) -> Point<Pixels> {
        point(
            at.x + self.cell * l.cols.max(1) as f32 / 2.,
            at.y + self.row / 2.,
        )
    }
    /// How many characters 退格收工 has left at `t`, in reading order.
    pub(super) fn kept(&self, t: f32) -> usize {
        (self.reading.len() as f32 * (1. - sl(t, 0.12, 0.54).powf(1.5))).round() as usize
    }
}
/// The line typed once the screen is empty: as much of it as fits, a character with its
/// width in cells (a wide one takes two, as in the grid).
pub(super) fn typed_line(title: Option<&str>, cols: usize) -> Vec<(char, u16)> {
    let line = match title {
        Some(t) => format!("✓ {} · {t}", Status::Closed.label()),
        None => format!("✓ {}", Status::Closed.label()),
    };
    let mut used = 0;
    line.chars()
        .map(|c| (c, if (c as u32) > 0x2e7f { 2 } else { 1 }))
        .take_while(|(_, w)| {
            used += *w as usize;
            used + 2 <= cols.max(8)
        })
        .collect()
}
/// Gives every character a spot on the pile: dropped near its own column, stacking up.
fn pile_up(letters: &mut [Letter], grid: Bounds<Pixels>, cell: Pixels, row: Pixels) {
    let mut rand = rng(42);
    let mut heights: HashMap<i32, f32> = HashMap::new();
    let step = f32::from(row) * PILE_STEP;
    let floor = grid.bottom() - row;
    let (left, right) = (grid.left(), grid.right() - cell);
    let highest = f32::from(grid.size.height) * PILE_MAX;
    for l in letters {
        let (jitter, spread) = (rand(), rand());
        let x = (l.at.x + px((spread - 0.5) * 32.)).max(left).min(right);
        let stack = heights
            .entry((f32::from(x) / f32::from(cell)).round() as i32)
            .or_default();
        l.pile = point(x, floor - px(stack.min(highest)));
        l.jitter = jitter;
        *stack += step;
    }
}

/// Clears the real grid under what the overlay moves: all of it once the characters have
/// lifted off; for 退格收工 everything after its cursor, so the text left is the terminal's
/// own. Drawn from the grid as it is now, so a program's colored cells go too.
pub(super) fn text_wipe(
    pane: Entity<Pane>,
    style: Style,
    t: f32,
    text: Rc<Snapshot>,
) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| (),
        move |_, _, window, cx| {
            let Some((grid, row, ground)) = pane.read(cx).grid_area() else {
                return;
            };
            let color = T::color(ground);
            let wipe = |left: Pixels, top: Pixels, bottom: Pixels, window: &mut Window| {
                if grid.right() > left && bottom > top {
                    window.paint_quad(gpui::fill(
                        Bounds::new(point(left, top), size(grid.right() - left, bottom - top)),
                        color,
                    ));
                }
            };
            match style {
                Style::Gravity | Style::Swirl if t >= LIFT => {
                    wipe(grid.left(), grid.top(), grid.bottom(), window);
                }
                Style::Backspace if t > 0.12 => match text.kept(t).checked_sub(1) {
                    None => wipe(grid.left(), grid.top(), grid.bottom(), window),
                    Some(last) => {
                        let (right, top) = text.reading[last];
                        wipe(right, top, top + row, window);
                        wipe(grid.left(), top + row, grid.bottom(), window);
                    }
                },
                _ => {}
            }
        },
    )
    .absolute()
    .inset_0()
}
