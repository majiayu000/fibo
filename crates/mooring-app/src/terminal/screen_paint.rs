use super::*;
pub fn shape_cell(
    run: &Run,
    font: Font,
    metrics: CellMetrics,
    window: &Window,
) -> (ShapedLine, Pixels) {
    // Cell backgrounds/decorations retain full width; trailing ASCII blanks have no ink.
    let text = run.text.trim_end_matches(' ');
    let tr = TextRun {
        len: text.len(),
        font,
        color: text_color(run),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let ts = window.text_system();
    let mut line = ts.shape_line(
        SharedString::from(text.to_owned()),
        px(font_size()),
        std::slice::from_ref(&tr),
        None,
    );
    let mut offset = px(0.);
    if !run.text.is_ascii()
        && line.width() > px(0.)
        && (run.cols == 2
            || run
                .text
                .chars()
                .any(|c| matches!(c as u32, 0xe000..=0xf8ff | 0xf0000..=0xffffd)))
    {
        let target = metrics.width * run.cols as f32;
        let cjk = run.text.chars().any(|c| matches!(c as u32, 0x2e80..=0xa4cf | 0xac00..=0xd7ff | 0xf900..=0xfaff | 0xff01..=0xff60 | 0x20000..=0x323af));
        let scale = if cjk {
            // Ideographs keep the font's own ideographic width: the terminal's type is the
            // user's, and a design pass does not resize it.
            let ic = metrics.ic_width;
            let fallback_ic = line
                .runs
                .first()
                .and_then(|r| ts.advance(r.font_id, px(font_size()), '水').ok())
                .map(|s| s.width)
                .unwrap_or(line.width());
            (ic / fallback_ic).min(metrics.height / px(font_size()))
        } else {
            (target / line.width()).min(metrics.height / px(font_size()))
        };
        line = ts.shape_line(
            SharedString::from(text.to_owned()),
            px(font_size()) * scale,
            std::slice::from_ref(&tr),
            None,
        );
        // Emoji advances snap to the bitmap strike, so one proportional pass can still
        // leave the glyph a fraction of a pixel wider than its cells.
        if line.width() > target {
            line = ts.shape_line(
                SharedString::from(text.to_owned()),
                px(font_size()) * scale * (target / line.width()),
                &[tr],
                None,
            );
        }
        offset = ((target - line.width()) / 2.).max(px(0.));
    }
    (line, offset)
}
pub fn run_font(run: &Run, f: &Font) -> Font {
    let mut font = if run.text.is_ascii() {
        f.clone()
    } else {
        crate::ui::terminal_text::non_ascii_face(f)
    };
    if run.attrs.bold {
        font.weight = FontWeight::BOLD;
    }
    if run.attrs.italic {
        font.style = FontStyle::Italic;
    }
    font
}
pub(super) fn printable_cells(run: &Run) -> bool {
    run.text.len() == run.cols as usize && run.text.bytes().all(|b| (b' '..=b'~').contains(&b))
}
/// Current viewport only: no history of strings from a long-running terminal.
#[derive(Default)]
pub struct LayoutCache {
    key: Option<(Font, CellMetrics, u64)>,
    rows: Vec<Vec<(Run, ShapedLine, Pixels)>>,
    pub(super) ascii: [Option<Option<AsciiGlyphs>>; 4],
}
impl LayoutCache {
    fn prepare(&mut self, font: &Font, metrics: CellMetrics, rows: usize) {
        let generation = crate::ui::terminal_text::generation();
        if self.key.as_ref() != Some(&(font.clone(), metrics, generation)) {
            self.rows.clear();
            self.ascii = Default::default();
            self.key = Some((font.clone(), metrics, generation));
        }
        self.rows.resize_with(rows, Vec::new);
    }
}
/// How the cursor is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorPaint {
    Hidden,
    /// In the shape the program asked for.
    Shown,
    /// An outline whatever the shape, for a split without focus.
    Hollow,
}
// Render inputs remain explicit alongside GPUI's required Window/App contexts.
// A wrapper type solely to reduce argument count would hide their call-site ownership.
#[allow(clippy::too_many_arguments)]
pub fn paint_cached(
    sc: &Screen,
    f: &Font,
    bounds: Bounds<Pixels>,
    metrics: CellMetrics,
    cursor: CursorPaint,
    cache: &mut LayoutCache,
    window: &mut Window,
    cx: &mut App,
) {
    // One layer for the grid: its thousands of glyphs and cell backgrounds share a
    // draw order instead of each being inserted into GPUI's per-primitive bounds tree.
    // Inside the layer quads draw under sprites, as Ghostty's background/text passes do.
    // Glyph strokes thicken by luminance as the rest of the UI's text does (Ghostty's
    // font-thicken = true): left as drawn, light thin strokes on the dark grid read
    // as blur on a lower-density display.
    let ground = extras::paint_ground(sc, bounds, window);
    window.paint_layer(bounds, |window| {
        window.with_glyph_dilation(None, |window| {
            paint_grid(sc, f, bounds, metrics, ground, cache, window, cx)
        })
    });
    // Cursor and later overlays stay outside the layer so they still draw over glyphs.
    let cw = metrics.width;
    let lh = metrics.height;
    if cursor != CursorPaint::Hidden
        && sc.cursor_visible
        && let Some((x, y)) = sc.cursor
    {
        let cell = Bounds::new(
            point(
                bounds.origin.x + cw * x as f32,
                bounds.origin.y + lh * y as f32,
            ),
            size(cw, lh),
        );
        let color = hsla(sc.cursor_color);
        let t = metrics.stroke * 2.;
        let shape = if cursor == CursorPaint::Hollow {
            CursorShape::BlockHollow
        } else {
            sc.cursor_shape
        };
        match shape {
            // A solid block with its letter redrawn in the background color, as Ghostty
            // does; a letter it cannot redraw (wide, or a ligature font) shows through.
            CursorShape::Block => {
                let under = sc.rows.get(y as usize).and_then(|row| {
                    row.iter()
                        .find(|r| r.col <= x && x < r.col + r.cols)
                        .filter(|r| printable_cells(r) && !r.attrs.invisible)
                        .map(|r| (r, &r.text[usize::from(x - r.col)..][..1]))
                });
                let glyphs = under.and_then(|(run, _)| {
                    let style = usize::from(run.attrs.bold) | (usize::from(run.attrs.italic) << 1);
                    cache.ascii[style].as_ref().and_then(Option::as_ref)
                });
                match (under, glyphs) {
                    (Some((_, letter)), Some(glyphs)) => {
                        window.paint_quad(fill(cell, color));
                        let text = crate::ui::theme::Theme::palette().term_cursor_text;
                        glyphs.paint(
                            letter,
                            cell.origin,
                            metrics,
                            hsla(text.unwrap_or(sc.background)),
                            window,
                        );
                    }
                    (Some((_, " ")), None) | (None, _) => window.paint_quad(fill(cell, color)),
                    _ => window.paint_quad(fill(cell, color.opacity(0.5))),
                }
            }
            CursorShape::Bar => {
                window.paint_quad(fill(Bounds::new(cell.origin, size(t, lh)), color))
            }
            CursorShape::Underline => window.paint_quad(fill(
                Bounds::new(point(cell.left(), cell.bottom() - t), size(cw, t)),
                color,
            )),
            CursorShape::BlockHollow => window.paint_quad(
                gpui::outline(cell, color, gpui::BorderStyle::Solid).border_widths(metrics.stroke),
            ),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_grid(
    sc: &Screen,
    f: &Font,
    bounds: Bounds<Pixels>,
    metrics: CellMetrics,
    ground: f32,
    cache: &mut LayoutCache,
    window: &mut Window,
    cx: &mut App,
) {
    if ground > 0. {
        window.paint_quad(fill(bounds, hsla(sc.background).opacity(ground)));
    }
    let cw = metrics.width;
    let lh = metrics.height;
    cache.prepare(f, metrics, sc.rows.len());
    for (y, row) in sc.rows.iter().enumerate() {
        let oy = bounds.origin.y + lh * y as f32;
        if oy > bounds.origin.y + bounds.size.height {
            break;
        }
        let cached = &mut cache.rows[y];
        let mut index = 0;
        for run in row {
            let ox = bounds.origin.x + cw * run.col as f32;
            if let Some(bg) = run.bg {
                window.paint_quad(fill(
                    Bounds::new(point(ox, oy), size(cw * run.cols as f32, lh)),
                    hsla(bg),
                ));
            }
            if run.attrs.invisible {
                continue;
            }
            paint_decorations(run, point(ox, oy), metrics, window);
            if printable_cells(run) {
                let style = usize::from(run.attrs.bold) | (usize::from(run.attrs.italic) << 1);
                let glyphs = cache.ascii[style].get_or_insert_with(|| {
                    let mut font = f.clone();
                    if run.attrs.bold {
                        font.weight = FontWeight::BOLD;
                    }
                    if run.attrs.italic {
                        font.style = FontStyle::Italic;
                    }
                    AsciiGlyphs::new(font, metrics, window)
                });
                if let Some(glyphs) = glyphs {
                    glyphs.paint(&run.text, point(ox, oy), metrics, text_color(run), window);
                    continue;
                }
            }
            if run.text.trim().is_empty() {
                continue;
            }
            if run.text.chars().count() == 1
                && sprites::paint(
                    run.text.chars().next().unwrap(),
                    point(ox, oy),
                    cw * run.cols as f32,
                    lh,
                    metrics.stroke,
                    text_color(run),
                    window,
                )
            {
                continue;
            }
            if cached.get(index).is_none_or(|(old, _, _)| old != run) {
                let (line, offset) = shape_cell(run, run_font(run, f), metrics, window);
                let entry = (run.clone(), line, offset);
                if index < cached.len() {
                    cached[index] = entry;
                } else {
                    cached.push(entry);
                }
            }
            let (_, line, offset) = &cached[index];
            index += 1;
            if run.text.is_ascii() {
                // One glyph per column: place each on its cell, since the cell width is
                // rounded to backing pixels and the shaped advances are not.
                let baseline = (lh - line.ascent - line.descent) / 2. + line.ascent;
                for shaped in &line.runs {
                    for g in &shaped.glyphs {
                        let _ = window.paint_glyph(
                            point(ox + cw * g.index as f32, oy + baseline),
                            shaped.font_id,
                            g.id,
                            px(font_size()),
                            text_color(run),
                        );
                    }
                }
                continue;
            }
            let _ = line.paint(
                point(ox + *offset, oy),
                lh,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        cached.truncate(index);
    }
}

pub fn text_color(run: &Run) -> Hsla {
    hsla(run.fg).opacity(if run.attrs.faint {
        crate::ui::terminal_text::faint()
    } else {
        1.
    })
}
