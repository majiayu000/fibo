//! What a run keeps of the terminal for the styles that move or erase its text: every
//! character with where it was and where it lands, and the wipe that clears the real grid
//! under them. Every frame is still a pure function of `t` and this.
use super::*;
use crate::terminal::glyph::{self, AskBar, Face, TextCell};
use crate::terminal::screen::{Attrs, CellMetrics};
use gpui::App;
use std::cell::OnceCell;
use std::collections::HashMap;

/// Above this many characters a packed screen moves one of every n, so a frame stays
/// around what the grid itself draws; the others fade where they are. 6419 cost under a
/// millisecond a frame in a release build.
const MAX_LETTERS: usize = 20_000;
/// Where the characters leave the grid for the overlay, and the grid is cleared.
pub(super) const LIFT: f32 = 0.1;
/// What a text element ends a line too long for it with.
const ELLIPSIS: &str = "…";
/// How long the characters that do not move take to fade.
const REST_FADE: f32 = 0.12;
/// How long the bar's band outlasts its words.
const BAND_FADE: f32 = 0.1;
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
    /// How it was shaped, if it is not one of the grid's.
    face: Option<Face>,
}
/// A character, its width in cells, and whether it is bold and italic.
type FaceKey = (char, u16, bool, bool);
/// A line of text: its left and right, its top, the color it starts in.
type Line = (Pixels, Pixels, Pixels, Hsla);
/// What the terminal showed when the run started.
pub(super) struct Snapshot {
    /// The characters that move, shaped on the first frame that draws them: a line of
    /// text outside the grid can only be shaped with a window.
    letters: OnceCell<(Vec<Letter>, Vec<Letter>)>,
    /// The grid's characters that go into `letters`.
    moving: RefCell<Vec<TextCell>>,
    /// The pinned ask over the grid's top, which the text it hides never showed from.
    bar: Option<AskBar>,
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
    bar_font: gpui::Font,
    texts: std::sync::Arc<gpui::TextSystem>,
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
        let bar = pane.ask_bar(cx);
        let mut cells = pane.text_cells();
        if let Some(bar) = &bar {
            cells.retain(|c| c.at.y + row <= bar.area.top() || c.at.y >= bar.area.bottom());
        }
        // The bar's words where the cursor and the card's lines take them, by cells.
        let reading = bar
            .iter()
            .flat_map(|b| bar_cells(b, cell))
            .chain(cells.iter().map(|c| (c.at.x, c.cols, c.at.y, c.color)))
            .map(|(x, cols, y, color)| (x, x + cell * cols.max(1) as f32, y, color))
            .collect::<Vec<_>>();
        let mut lines: Vec<Line> = Vec::new();
        for &(left, right, top, color) in &reading {
            match lines.last_mut() {
                Some(l) if l.2 == top => l.1 = right,
                _ => lines.push((left, right, top, color)),
            }
        }
        let reading = reading.into_iter().map(|c| (c.1, c.2)).collect();
        let moves = matches!(style, Style::Gravity | Style::Swirl);
        let moving = if moves { cells } else { vec![] };
        let cols = (f32::from(grid.size.width) / f32::from(cell)).floor() as usize;
        let typed = typed_line(title, cols);
        Some(Self {
            letters: OnceCell::new(),
            moving: RefCell::new(moving),
            bar,
            reading,
            lines,
            grid,
            cell,
            row,
            typed,
            fg: T::color(T::inverse(ground)),
            font: T::terminal_mono(cx),
            bar_font: T::mono(cx),
            texts: cx.text_system().clone(),
            metrics,
            faces: RefCell::default(),
        })
    }
    /// The characters that move, the bar's first as they read first.
    pub(super) fn letters<'a>(&'a self, window: &Window) -> &'a [Letter] {
        &self.parts(window).0
    }
    /// Those and the characters left over from a screen too packed to move them all.
    fn parts<'a>(&'a self, window: &Window) -> &'a (Vec<Letter>, Vec<Letter>) {
        self.letters.get_or_init(|| {
            let cells = self.moving.take();
            let step = cells.len().div_ceil(MAX_LETTERS).max(1);
            let mut letters = self.bar_letters(window);
            let mut rest = Vec::new();
            for (i, c) in cells.into_iter().enumerate() {
                let letter = Letter {
                    ch: c.ch,
                    at: c.at,
                    cols: c.cols,
                    color: c.color,
                    attrs: c.attrs,
                    pile: c.at,
                    jitter: 0.,
                    face: None,
                };
                if i % step == 0 {
                    letters.push(letter);
                } else {
                    rest.push(letter);
                }
            }
            pile_up(&mut letters, self.grid, self.cell, self.row);
            #[cfg(feature = "visual-test")]
            SPECKS.store(letters.len(), std::sync::atomic::Ordering::Relaxed);
            (letters, rest)
        })
    }
    /// Fades the characters that do not move, where they were.
    pub(super) fn fade_rest(&self, t: f32, window: &mut Window) {
        let left = 1. - sl(t, LIFT, LIFT + REST_FADE);
        if left <= 0. {
            return;
        }
        for l in &self.parts(window).1 {
            self.draw(l, l.at, l.color.opacity(left), window);
        }
    }
    /// The bar's words, shaped and placed as its element lays them out, so nothing moves
    /// when they take over from it. Only the styles that move letters ask for these.
    fn bar_letters(&self, window: &Window) -> Vec<Letter> {
        let Some(bar) = &self.bar else {
            return vec![];
        };
        let height = bar.area.size.height;
        let (left, top) = (bar.area.left(), bar.area.top());
        let right = bar.area.right() - px(T::S8);
        let shape = |text: &str, size: f32| {
            glyph::line_glyphs(text, &self.bar_font, px(size), height, window)
        };
        let (mark, mark_width) = shape(&bar.mark, T::MONO_SIZE);
        let (back, back_width) = if bar.back {
            shape(glyph::ASK_BACK, T::META)
        } else {
            (vec![], px(0.))
        };
        // A text element is as wide as its line rounded up.
        let words_left = left + mark_width.ceil() + px(T::S8);
        let words_end = if bar.back {
            right - back_width.ceil() - px(T::S8)
        } else {
            right
        };
        let letter = |g: glyph::LineGlyph, x: Pixels, color: u32| Letter {
            ch: g.ch,
            at: point(x + g.x, top),
            cols: 1,
            color: T::color(color),
            attrs: Attrs::default(),
            pile: point(x + g.x, top),
            jitter: 0.,
            face: Some(g.face),
        };
        let mut letters = Vec::new();
        letters.extend(mark.into_iter().map(|g| letter(g, left, T::muted())));
        // Where the element ends a line too long for it, by the same reckoning.
        let cut = self
            .texts
            .line_wrapper(self.bar_font.clone(), px(T::MONO_SIZE))
            .should_truncate_line(
                &bar.text,
                words_end - words_left,
                ELLIPSIS,
                gpui::TruncateFrom::End,
            );
        let (words, _) = match cut {
            Some(at) => shape(&format!("{}{ELLIPSIS}", &bar.text[..at]), T::MONO_SIZE),
            None => shape(&bar.text, T::MONO_SIZE),
        };
        letters.extend(
            words
                .into_iter()
                .map(|g| letter(g, words_left, T::primary())),
        );
        letters.extend(
            back.into_iter()
                .map(|g| letter(g, right - back_width.ceil(), T::muted())),
        );
        letters
    }
    /// A character of the snapshot with its cell's top left at `at`, as it was drawn.
    pub(super) fn draw(&self, l: &Letter, at: Point<Pixels>, color: Hsla, window: &mut Window) {
        if let Some(face) = l.face {
            glyph::paint_face(face, at, color, window);
            return;
        }
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
/// Roughly where the bar's words are, for what only needs their reach: left, width in
/// cells, top, color. The element's own spacing is `Snapshot::bar_letters`.
fn bar_cells(bar: &AskBar, cell: Pixels) -> impl Iterator<Item = (Pixels, u16, Pixels, Hsla)> + '_ {
    let (top, end) = (bar.area.top(), bar.area.right());
    let mut x = bar.area.left();
    bar.mark
        .chars()
        .map(|c| (c, T::color(T::muted())))
        .chain(bar.text.chars().map(|c| (c, T::color(T::primary()))))
        .map_while(move |(c, color)| {
            let cols = if (c as u32) > 0x2e7f { 2 } else { 1 };
            let left = x;
            x += cell * cols as f32;
            (x <= end).then_some((left, cols, top, color))
        })
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
            let bar = text.bar.as_ref().map(|b| b.area);
            match style {
                Style::Gravity | Style::Swirl if t >= LIFT => {
                    wipe(grid.left(), grid.top(), grid.bottom(), window);
                    if let Some(area) = bar {
                        // The bar's words have left; its band goes after them.
                        window.paint_quad(gpui::fill(area, color));
                        let band = T::color(T::button());
                        let left = 1. - sl(t, LIFT, LIFT + BAND_FADE);
                        window.paint_quad(gpui::fill(area, band.opacity(left)));
                    }
                }
                Style::Backspace if t > 0.12 => {
                    let Some(last) = text.kept(t).checked_sub(1) else {
                        wipe(grid.left(), grid.top(), grid.bottom(), window);
                        if let Some(area) = bar {
                            window.paint_quad(gpui::fill(area, color));
                        }
                        return;
                    };
                    let (right, top) = text.reading[last];
                    match bar.filter(|area| top == area.top()) {
                        // The cursor is in the bar, which ends past the grid in a margin.
                        Some(area) => window.paint_quad(gpui::fill(
                            Bounds::new(
                                point(right, top),
                                size((area.right() - right).max(px(0.)), row),
                            ),
                            color,
                        )),
                        None => wipe(right, top, top + row, window),
                    }
                    wipe(grid.left(), top + row, grid.bottom(), window);
                }
                _ => {}
            }
        },
    )
    .absolute()
    .inset_0()
}
