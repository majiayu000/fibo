//! One character of a terminal drawn outside its grid, exactly as the grid draws it, for
//! the completion styles that move the text themselves.
use super::screen::{Attrs, CellMetrics, Run, run_font, shape_cell, sprites, text_color};
use gpui::{Font, FontId, GlyphId, Hsla, Pixels, Point, Window, point, px};

/// One character on screen.
pub struct TextCell {
    pub ch: char,
    /// Its cell's top left.
    pub at: Point<Pixels>,
    pub cols: u16,
    pub color: Hsla,
    pub attrs: Attrs,
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
    let Some(f) = face else {
        return;
    };
    let origin = point(at.x + f.offset, at.y + f.baseline);
    let _ = if f.emoji {
        window.paint_emoji(origin, f.font, f.glyph, f.size)
    } else {
        window.paint_glyph(origin, f.font, f.glyph, f.size.max(px(1.)), color)
    };
}
