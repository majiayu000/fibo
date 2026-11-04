//! One character of a terminal drawn outside its grid, exactly as the grid draws it, for
//! the completion styles that move the text themselves.
use super::screen::{Attrs, CellMetrics, Run, run_font, shape_cell, sprites, text_color};
use gpui::{Bounds, Font, FontId, GlyphId, Hsla, Pixels, Point, TextRun, Window, point, px};

/// One character on screen.
pub struct TextCell {
    pub ch: char,
    /// Its cell's top left.
    pub at: Point<Pixels>,
    pub cols: u16,
    pub color: Hsla,
    pub attrs: Attrs,
}

/// What the pinned ask's bar offers at its right end.
pub const ASK_BACK: &str = "回到这句 ↑";

/// The pinned ask along the terminal's top, as it is laid out over the grid.
pub struct AskBar {
    pub area: Bounds<Pixels>,
    pub mark: String,
    pub text: String,
    /// It offers a way back to the ask.
    pub back: bool,
}

/// One glyph of a line of text that is not in the grid.
pub struct LineGlyph {
    pub ch: char,
    /// From the line's left.
    pub x: Pixels,
    pub face: Face,
}

/// How one character is drawn, shaped once and reused for every frame.
#[derive(Clone, Copy)]
pub struct Face {
    font: FontId,
    glyph: GlyphId,
    size: Pixels,
    emoji: bool,
    baseline: Pixels,
    offset: Pixels,
}

/// Every character in `rows`, in reading order; `at` places a column and a row.
pub(super) fn cells_of(
    rows: &[Vec<Run>],
    at: impl Fn(u16, usize) -> Point<Pixels>,
) -> Vec<TextCell> {
    let mut cells = Vec::new();
    for (y, runs) in rows.iter().enumerate() {
        for run in runs {
            let each = (run.cols as usize / run.text.chars().count().max(1)).max(1) as u16;
            let mut col = run.col;
            for ch in run.text.chars() {
                if !ch.is_whitespace() && ch != '\0' {
                    cells.push(TextCell {
                        ch,
                        at: at(col, y),
                        cols: each,
                        color: text_color(run),
                        attrs: run.attrs,
                    });
                }
                col += each;
            }
        }
    }
    cells
}

/// Shapes one character the way the grid shapes its run, or None if it has no glyph.
pub fn face_of(
    ch: char,
    cols: u16,
    attrs: Attrs,
    font: &Font,
    metrics: CellMetrics,
    window: &Window,
) -> Option<Face> {
    let run = Run {
        col: 0,
        cols,
        text: ch.to_string(),
        fg: 0,
        bg: None,
        attrs,
    };
    let shaped_font = run_font(&run, font);
    let (line, offset) = shape_cell(&run, shaped_font, metrics, window);
    let first = line.runs.first()?;
    let glyph = first.glyphs.first()?;
    Some(Face {
        font: first.font_id,
        glyph: glyph.id,
        size: line.font_size,
        emoji: glyph.is_emoji,
        baseline: (metrics.height - line.ascent - line.descent) / 2. + line.ascent,
        offset,
    })
}

/// A line of text shaped as a text element shapes it, centered in a row of `height`, and
/// how wide it is.
pub fn line_glyphs(
    text: &str,
    font: &Font,
    size: Pixels,
    height: Pixels,
    window: &Window,
) -> (Vec<LineGlyph>, Pixels) {
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(text.to_owned().into(), size, &[run], None);
    let baseline = (height - line.ascent - line.descent) / 2. + line.ascent;
    let mut glyphs = Vec::new();
    for shaped in &line.runs {
        for g in &shaped.glyphs {
            let Some(ch) = text.get(g.index..).and_then(|rest| rest.chars().next()) else {
                continue;
            };
            if ch.is_whitespace() {
                continue;
            }
            glyphs.push(LineGlyph {
                ch,
                x: g.position.x,
                face: Face {
                    font: shaped.font_id,
                    glyph: g.id,
                    size: line.font_size,
                    emoji: g.is_emoji,
                    baseline,
                    offset: px(0.),
                },
            });
        }
    }
    (glyphs, line.width)
}

/// Paints a character with its cell's top left at `at`. Box-drawing characters the grid
/// draws as sprites stay sprites, so nothing changes when one leaves the grid. The caller
/// has turned glyph dilation off, as the grid does.
pub fn paint(
    ch: char,
    cols: u16,
    face: Option<Face>,
    at: Point<Pixels>,
    metrics: CellMetrics,
    color: Hsla,
    window: &mut Window,
) {
    if !ch.is_ascii()
        && sprites::paint(
            ch,
            at,
            metrics.width * cols.max(1) as f32,
            metrics.height,
            metrics.stroke,
            color,
            window,
        )
    {
        return;
    }
    if let Some(f) = face {
        paint_face(f, at, color, window);
    }
}

/// Paints a shaped character with its cell's top left at `at`.
pub fn paint_face(f: Face, at: Point<Pixels>, color: Hsla, window: &mut Window) {
    let origin = point(at.x + f.offset, at.y + f.baseline);
    let _ = if f.emoji {
        window.paint_emoji(origin, f.font, f.glyph, f.size)
    } else {
        window.paint_glyph(origin, f.font, f.glyph, f.size.max(px(1.)), color)
    };
}
