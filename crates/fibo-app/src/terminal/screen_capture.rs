use super::*;
/// Retain Ghostty's dirty-row state and only rebuild changed viewport rows.
/// One instance belongs to one Pane/VT connection, never shared between terminals.
pub struct Capture {
    state: RenderState<'static>,
    rows: RowIterator<'static>,
    cells: CellIterator<'static>,
    dimensions: (u16, u16),
    colors: Option<libghostty_vt::render::Colors>,
    defaults: Option<(u32, u32, Option<u32>, u32)>,
}
impl Capture {
    pub fn new() -> Result<Self, libghostty_vt::Error> {
        Ok(Self {
            state: RenderState::new()?,
            rows: RowIterator::new()?,
            cells: CellIterator::new()?,
            dimensions: (0, 0),
            colors: None,
            defaults: None,
        })
    }
    pub fn update(&mut self, vt: &Vt, out: &mut Screen) -> Result<usize, libghostty_vt::Error> {
        let snap = self.state.update(vt.terminal())?;
        let dimensions = (snap.cols()?, snap.rows()?);
        let colors = snap.colors()?;
        let mut full = self.colors.as_ref() != Some(&colors)
            || self.dimensions != dimensions
            || snap.dirty()? == libghostty_vt::render::Dirty::Full;
        self.dimensions = dimensions;
        let mut changed = 0;
        // Only the unset defaults come from the UI theme; OSC and explicit ANSI colors survive.
        let def_fg = vt
            .terminal()
            .fg_color()?
            .map(|c| rgb(c.r, c.g, c.b))
            .unwrap_or_else(crate::ui::theme::Theme::terminal_fg);
        let def_bg = vt
            .terminal()
            .bg_color()?
            .map(|c| rgb(c.r, c.g, c.b))
            .unwrap_or_else(crate::ui::theme::Theme::terminal_bg);
        // A theme's bold color is decided here, with the runs, so drawing stays a plain read.
        let bold_fg = crate::ui::theme::Theme::palette().term_bold;
        let min_contrast = crate::ui::terminal_text::min_contrast();
        let defaults = (def_fg, def_bg, bold_fg, min_contrast.to_bits());
        full |= self.defaults != Some(defaults);
        self.defaults = Some(defaults);
        let mut readable = extras::Readable::new(min_contrast);
        self.colors = Some(colors.clone());
        let palette = colors.palette;
        let mut rows = self.rows.update(&snap)?;
        let mut y = 0usize;
        let mut text = String::new();
        let mut line = Vec::new();
        while let Some(row) = rows.next() {
            if out.rows.len() <= y {
                out.rows.push(Vec::new());
            }
            out.selection.resize(out.selection.len().max(y + 1), None);
            // Selection changes leave rows clean, so it is read for every row.
            out.selection[y] = row.selection()?.map(|sel| (sel.start_x, sel.end_x));
            if !full && !row.dirty()? {
                y += 1;
                continue;
            }
            changed += 1;
            out.links.retain(|link| link.row != y as u16);
            let mut b = RowBuilder::new(&mut out.rows[y]);
            let mut cells = self.cells.update(row)?;
            let mut x = 0u16;
            line.clear();
            while let Some(cell) = cells.next() {
                let raw = cell.raw_cell()?;
                let wide = raw.wide()?;
                if matches!(wide, CellWide::SpacerTail | CellWide::SpacerHead) {
                    x += 1;
                    continue;
                }
                text.clear();
                if cell.graphemes_len()? == 0 {
                    text.push(' ');
                } else {
                    cell.graphemes_utf8(&mut text)?;
                }
                let set_fg = cell.fg_color()?;
                let mut fg = set_fg.map(|c| rgb(c.r, c.g, c.b)).unwrap_or(def_fg);
                let mut bg = cell.bg_color()?.map(|c| rgb(c.r, c.g, c.b));
                let mut attrs = Attrs::default();
                if cell.has_styling()? {
                    let st = cell.style()?;
                    if st.bold && set_fg.is_none() {
                        fg = bold_fg.unwrap_or(fg);
                    }
                    if st.inverse {
                        let old = fg;
                        fg = bg.unwrap_or(def_bg);
                        bg = Some(old);
                    }
                    attrs = Attrs {
                        bold: st.bold,
                        italic: st.italic,
                        faint: st.faint,
                        invisible: st.invisible,
                        strikethrough: st.strikethrough,
                        overline: st.overline,
                        underline: match st.underline {
                            SgrUnderline::Single => Underline::Single,
                            SgrUnderline::Double => Underline::Double,
                            SgrUnderline::Curly => Underline::Curly,
                            SgrUnderline::Dotted => Underline::Dotted,
                            SgrUnderline::Dashed => Underline::Dashed,
                            _ => Underline::None,
                        },
                        underline_color: match st.underline_color {
                            StyleColor::None => None,
                            StyleColor::Palette(i) => {
                                let c = palette[i.0 as usize];
                                Some(rgb(c.r, c.g, c.b))
                            }
                            StyleColor::Rgb(c) => Some(rgb(c.r, c.g, c.b)),
                        },
                    };
                }
                fg = readable.fg(fg, bg.unwrap_or(def_bg));
                let width = if matches!(wide, CellWide::Wide) { 2 } else { 1 };
                if raw.has_hyperlink()? {
                    let grid = vt
                        .terminal()
                        .grid_ref(libghostty_vt::terminal::Point::Viewport(
                            libghostty_vt::terminal::PointCoordinate { x, y: y as u32 },
                        ))?;
                    let mut uri = vec![0; 8192];
                    if let Ok(n) = grid.hyperlink_uri(&mut uri)
                        && let Ok(uri) = std::str::from_utf8(&uri[..n])
                        && allowed_link(uri)
                    {
                        out.links.push(Link {
                            row: y as u16,
                            col: x,
                            cols: width,
                            uri: uri.into(),
                            start: (y as u16, x),
                            alone: None,
                        });
                    }
                }
                line.push((x, width, text.chars().next().unwrap_or(' ')));
                b.push(x, width, &text, fg, bg, attrs);
                x += 1;
            }
            if out.cells.len() <= y {
                out.cells.resize(y + 1, Vec::new());
                out.wrapped.resize(y + 1, false);
            }
            out.cells[y].clone_from(&line);
            out.wrapped[y] = row.raw_row()?.is_wrapped()?;
            for link in text_links(y as u16, &line) {
                let taken = |l: &Link| {
                    l.row == link.row && l.col < link.col + link.cols && link.col < l.col + l.cols
                };
                if !out.links.iter().any(taken) {
                    out.links.push(link);
                }
            }
            y += 1;
        }
        out.rows.truncate(y);
        out.selection.truncate(y);
        // Ghostty may start a selection on a wide character's right half or end it on
        // its left half; it copies the whole character, so draw the whole character.
        for (sel, cells) in out.selection.iter_mut().zip(&out.cells) {
            if let Some((start, end)) = sel {
                for &(x, w, _) in cells.iter().filter(|c| c.1 > 1) {
                    if (x..x + w).contains(start) {
                        *start = x;
                    }
                    if (x..x + w).contains(end) {
                        *end = x + w - 1;
                    }
                }
            }
        }
        out.links.retain(|link| (link.row as usize) < y);
        out.cells.truncate(y);
        out.wrapped.truncate(y);
        if changed > 0 || full {
            out.paths = find_paths(&out.cells, &out.wrapped, dimensions.0);
        }
        let cur = snap.cursor()?;
        out.cursor = cur.viewport.map(|v| (v.x, v.y));
        out.cursor_visible = cur.visible;
        out.cursor_blinking = cur.blinking;
        out.cursor_shape = match cur.visual_style {
            CursorVisualStyle::Bar => CursorShape::Bar,
            CursorVisualStyle::Underline => CursorShape::Underline,
            CursorVisualStyle::BlockHollow => CursorShape::BlockHollow,
            _ => CursorShape::Block,
        };
        out.cursor_color = colors
            .cursor
            .map(|c| rgb(c.r, c.g, c.b))
            .unwrap_or_else(crate::ui::theme::Theme::terminal_fg);
        out.background = vt
            .terminal()
            .bg_color()?
            .map(|c| rgb(c.r, c.g, c.b))
            .unwrap_or_else(crate::ui::theme::Theme::terminal_bg);
        snap.clean()?;
        Ok(changed)
    }
}
/// Standalone captures for tests and the reference examples; production keeps a Capture
/// per Pane, so the app binary itself never calls it.
#[cfg(any(test, feature = "visual-test"))]
#[cfg_attr(not(test), allow(dead_code))]
pub fn capture(vt: &Vt) -> Result<Screen, libghostty_vt::Error> {
    let mut out = Screen::default();
    Capture::new()?.update(vt, &mut out)?;
    Ok(out)
}
