#[path = "screen_extras.rs"]
mod extras;
#[path = "sprites.rs"]
mod sprites;
use extras::paint_decorations;
#[allow(unused_imports)]
pub use extras::{paint_guide, paint_selected_text};
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub col: u16,
    pub cols: u16,
    pub text: String,
    pub fg: u32,
    pub bg: Option<u32>,
    pub attrs: Attrs,
}

/// SGR attributes other than colors; a run breaks when any of them changes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Attrs {
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub underline: Underline,
    /// SGR 58; `None` draws the underline in the text color.
    pub underline_color: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// DECSCUSR shape as resolved by Ghostty (hollow when the terminal is unfocused).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    #[default]
    Block,
    BlockHollow,
    Bar,
    Underline,
}

#[derive(Default)]
pub struct Screen {
    pub rows: Vec<Vec<Run>>,
    pub cursor: Option<(u16, u16)>,
    pub cursor_visible: bool,
    /// The terminal's blink mode (DEC 12, DECSCUSR); the pane decides the phase.
    pub cursor_blinking: bool,
    pub cursor_shape: CursorShape,
    /// The scheme's cursor color, or one the program set (OSC 12).
    pub cursor_color: u32,
    pub background: u32,
    pub links: Vec<Link>,
    /// Text that reads like a file path; only one that exists on disk becomes clickable.
    pub paths: Vec<Link>,
    /// Each row's cells (column, width, first char) and whether it soft-wraps into the
    /// next, kept so a path broken over rows is found again when only one row changes.
    pub cells: Vec<Vec<(u16, u16, char)>>,
    pub wrapped: Vec<bool>,
    /// Per viewport row, the selected columns (inclusive) of the terminal's selection.
    pub selection: Vec<Option<(u16, u16)>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub row: u16,
    pub col: u16,
    pub cols: u16,
    pub uri: String,
    /// A path broken over rows is one word in several pieces; each piece carries the
    /// whole word in `uri`, where the first piece starts, and its own text in case the
    /// joined word is not a real file (a path that just ended at the edge).
    pub start: (u16, u16),
    pub alone: Option<String>,
}
impl Screen {
    pub fn link_at(&self, row: u16, col: u16) -> Option<&Link> {
        self.links
            .iter()
            .find(|l| l.row == row && col >= l.col && col < l.col + l.cols)
    }
    pub fn path_at(&self, row: u16, col: u16) -> Option<&Link> {
        self.paths
            .iter()
            .find(|l| l.row == row && col >= l.col && col < l.col + l.cols)
    }
}
/// Only explicitly clicked OSC 8 links reach the OS. No OSC callback opens URLs.
pub fn allowed_link(uri: &str) -> bool {
    if uri.len() > 8192 || uri.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    uri.strip_prefix("https://")
        .or_else(|| uri.strip_prefix("http://"))
        .is_some_and(|s| !s.is_empty() && !s.starts_with('/'))
        || uri.strip_prefix("file:///").is_some_and(|s| !s.is_empty())
}
/// Plain-text http(s) URLs in one row, found by text as iTerm2 and Ghostty do when the
/// program printed no OSC 8 link. `cells` holds each cell's column, width and first char.
/// A URL ends at whitespace or any non-ASCII char, so `端口：http://a/b。` stops before `。`;
/// trailing punctuation and unbalanced closing brackets belong to the prose around it.
/// Words that read like file paths: `~/x`, `/x`, `./x`, `a/b`, or `name.ext`, each with
/// an optional `:line[:col]`. A word ends at whitespace, quotes, brackets, box lines and
/// CJK punctuation, so `链接到了 ~/.claude/skills。` yields `~/.claude/skills`. Whether it
/// exists is the pane's question, asked only on hover.
///
/// A row's last word runs on into the next row's first word when the row soft-wraps, or
/// when it reaches the right edge (`cols`) as an Agent's own line breaking leaves a long
/// path, so a path broken by a narrow pane is still one word.
fn find_paths(rows: &[Vec<(u16, u16, char)>], wrapped: &[bool], cols: u16) -> Vec<Link> {
    let words: Vec<Vec<(usize, usize)>> = rows.iter().map(|cells| row_words(cells)).collect();
    let text = |y: usize, (i, end): (usize, usize)| -> String {
        rows[y][i..end].iter().map(|c| c.2).collect()
    };
    let mut paths = Vec::new();
    // The rows whose first word was taken into the word above.
    let mut taken = vec![false; rows.len()];
    for y in 0..rows.len() {
        for (k, &word) in words[y].iter().enumerate() {
            if k == 0 && taken[y] {
                continue;
            }
            let mut pieces = vec![(y, word)];
            let (mut ly, mut lk) = (y, k);
            loop {
                let (_, end) = words[ly][lk];
                let next = ly + 1;
                let soft = wrapped.get(ly).copied().unwrap_or(false);
                let edge = rows[ly][end - 1].0 + rows[ly][end - 1].1;
                if lk + 1 != words[ly].len()
                    || next >= rows.len()
                    || words[next].is_empty()
                    // Only spaces may follow the piece in its row.
                    || !rows[ly][end..].iter().all(|c| c.2 == ' ')
                    || !(soft || edge + 2 >= cols)
                    // A soft wrap continues at column 0.
                    || (soft && words[next][0].0 != 0)
                {
                    break;
                }
                // Prose that happens to reach the edge is not glued to the next line.
                let so_far: String = pieces.iter().map(|&(py, w)| text(py, w)).collect();
                if !looks_like_path(&(so_far + &text(next, words[next][0]))) {
                    break;
                }
                pieces.push((next, words[next][0]));
                (ly, lk) = (next, 0);
            }
            let whole: String = pieces.iter().map(|&(py, w)| text(py, w)).collect();
            let whole = trim_word(&whole);
            if !looks_like_path(&whole) {
                continue;
            }
            let start = (y as u16, rows[y][word.0].0);
            let joined = pieces.len() > 1;
            for (n, &(py, (i, end))) in pieces.iter().enumerate() {
                if n > 0 {
                    taken[py] = true;
                }
                let mut end = end;
                if n + 1 == pieces.len() {
                    while end > i && matches!(rows[py][end - 1].2, '.' | ':' | '!' | '?') {
                        end -= 1;
                    }
                }
                if end == i {
                    continue;
                }
                let own = trim_word(&text(py, (i, end)));
                let last = rows[py][end - 1];
                paths.push(Link {
                    row: py as u16,
                    col: rows[py][i].0,
                    cols: last.0 + last.1 - rows[py][i].0,
                    uri: whole.clone(),
                    start,
                    alone: (joined && looks_like_path(&own)).then_some(own),
                });
            }
        }
    }
    paths
}
/// A word's trailing `.`, `:`, `!` and `?` belong to the sentence around it.
fn trim_word(word: &str) -> String {
    word.trim_end_matches(['.', ':', '!', '?']).to_owned()
}
/// Where each word of one row starts and ends, as indexes into `cells`.
fn row_words(cells: &[(u16, u16, char)]) -> Vec<(usize, usize)> {
    let separator = |c: char| {
        c.is_whitespace()
            || "\"'`()[]{}<>|,;*".contains(c)
            || "。，、；：！？（）「」『』《》【】“”‘’…".contains(c)
            || ('\u{2500}'..='\u{259f}').contains(&c)
    };
    let mut words = Vec::new();
    let mut i = 0;
    while i < cells.len() {
        if separator(cells[i].2) {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < cells.len() && !separator(cells[end].2) {
            end += 1;
        }
        words.push((i, end));
        i = end;
    }
    words
}
fn looks_like_path(word: &str) -> bool {
    let (file, _) = split_line(word);
    if file.contains("://") || !file.chars().any(char::is_alphanumeric) {
        return false;
    }
    if file.contains('/') {
        return true;
    }
    // A bare file name needs a stem and an extension with a letter, so "5.5" and
    // "v0.1.41" stay prose.
    file.rsplit_once('.').is_some_and(|(stem, ext)| {
        stem.chars().any(char::is_alphabetic)
            && (1..=8).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    })
}
/// `a.rs:42:7` is `a.rs` at line 42; anything else is all file.
pub fn split_line(word: &str) -> (&str, Option<u32>) {
    let (file, at) = split_position(word);
    (file, at.map(|(line, _)| line))
}
/// `a.rs:42:7` is `a.rs` at line 42, column 7.
pub fn split_position(word: &str) -> (&str, Option<(u32, Option<u32>)>) {
    let mut parts = word.rsplitn(3, ':');
    let last = parts.next().unwrap_or_default();
    let mid = parts.next();
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    match (mid, parts.next()) {
        (Some(line), Some(file)) if digits(line) && digits(last) && !file.is_empty() => {
            match line.parse() {
                Ok(n) => (file, Some((n, last.parse().ok()))),
                Err(_) => (word, None),
            }
        }
        (Some(_), _) if digits(last) && word.len() > last.len() + 1 => {
            let file = &word[..word.len() - last.len() - 1];
            match last.parse() {
                Ok(n) => (file, Some((n, None))),
                Err(_) => (word, None),
            }
        }
        _ => (word, None),
    }
}
/// Where a path word points: `~` is home, a relative path starts at `cwd`.
pub fn resolve_path(word: &str, cwd: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    let (file, _) = split_line(word);
    if let Some(rest) = file.strip_prefix("~/") {
        return Some(std::path::PathBuf::from(std::env::var_os("HOME")?).join(rest));
    }
    let path = std::path::Path::new(file);
    if path.is_absolute() {
        Some(path.to_owned())
    } else {
        Some(cwd?.join(path))
    }
}
fn text_links(row: u16, cells: &[(u16, u16, char)]) -> Vec<Link> {
    let starts_with = |i: usize, p: &str| {
        cells.len() - i >= p.len() && p.chars().zip(&cells[i..]).all(|(a, c)| a == c.2)
    };
    let url_char = |c: char| {
        c.is_ascii_graphic() && !matches!(c, '<' | '>' | '"' | '`' | '{' | '}' | '|' | '\\' | '^')
    };
    let mut links = Vec::new();
    let mut i = 0;
    while i < cells.len() {
        let boundary = i == 0 || !cells[i - 1].2.is_ascii_alphanumeric();
        if !(boundary && (starts_with(i, "https://") || starts_with(i, "http://"))) {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < cells.len() && url_char(cells[end].2) {
            end += 1;
        }
        let mut uri: String = cells[i..end].iter().map(|c| c.2).collect();
        while let Some(last) = uri.chars().last() {
            let open = match last {
                ')' => '(',
                ']' => '[',
                _ => '\0',
            };
            let unbalanced = open != '\0' && uri.matches(last).count() > uri.matches(open).count();
            if !(matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*') || unbalanced) {
                break;
            }
            uri.pop();
            end -= 1;
        }
        if end > i && allowed_link(&uri) {
            let last = cells[end - 1];
            links.push(Link {
                row,
                col: cells[i].0,
                cols: last.0 + last.1 - cells[i].0,
                uri,
                start: (row, cells[i].0),
                alone: None,
            });
        }
        i = end.max(i + 1);
    }
    links
}
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// Builds runs cell by cell; a run breaks when the style changes.
pub struct RowBuilder<'a> {
    row: &'a mut Vec<Run>,
    ascii: bool,
}

impl<'a> RowBuilder<'a> {
    pub fn new(row: &'a mut Vec<Run>) -> Self {
        row.clear();
        Self { row, ascii: false }
    }
    pub fn push(
        &mut self,
        col: u16,
        width: u16,
        text: &str,
        fg: u32,
        bg: Option<u32>,
        attrs: Attrs,
    ) {
        if let Some(last) = self.row.last_mut()
            && last.fg == fg
            && last.bg == bg
            && last.attrs == attrs
            && last.col + last.cols == col
            && width == 1
            && text.is_ascii()
            && self.ascii
        {
            last.text.push_str(text);
            last.cols += width;
            return;
        }
        // Wide / non-ASCII cells get their own run so each is placed on the cell grid.
        self.ascii = text.is_ascii();
        self.row.push(Run {
            col,
            cols: width,
            text: text.to_owned(),
            fg,
            bg,
            attrs,
        });
    }
}

// The dark scheme's defaults; a terminal without its own colors falls back to the theme's.
#[cfg(any(test, feature = "visual-test"))]
pub const DEFAULT_FG: u32 = mooring_core::terminal::DEFAULT_FG;

use mooring_core::terminal::{
    Vt,
    libghostty_vt::{
        self,
        render::CursorVisualStyle,
        render::{CellIterator, RenderState, RowIterator},
        screen::CellWide,
        style::{StyleColor, Underline as SgrUnderline},
    },
};
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

use crate::ui::theme::hsla;
use gpui::{
    App, Bounds, Font, FontStyle, FontWeight, Hsla, Pixels, ShapedLine, SharedString, TextAlign,
    TextRun, UnderlineStyle, Window, fill, point, px, size,
};
/// Terminal text size in points; settings and zoom shortcuts share one saved value.
pub fn font_size() -> f32 {
    crate::ui::terminal_text::size()
}
pub fn zoom(step: Option<i8>) -> Result<f32, String> {
    let mut text = crate::ui::terminal_text::current();
    text.size = zoomed(text.size, step);
    let size = text.size;
    crate::ui::terminal_text::update(text)?;
    Ok(size)
}
fn zoomed(size: f32, step: Option<i8>) -> f32 {
    step.map_or(crate::ui::theme::Theme::TERMINAL_FONT_SIZE, |s| {
        (size + f32::from(s)).clamp(8., 36.)
    })
}

/// One measurement shared by terminal paint, resize, preedit and IME geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellMetrics {
    /// The face's own advance rounded to backing pixels, as Ghostty font/Metrics.zig
    /// does: a fractional pitch (JetBrains Mono 13pt is 15.6px at 2x) puts most glyph
    /// stems across two pixels, which reads as blur on a lower-density display.
    pub width: Pixels,
    /// The face's unrounded advance, where shaping places each glyph.
    pub advance: Pixels,
    pub height: Pixels,
    /// Offsets from the cell top, matching where `ShapedLine::paint` puts the baseline.
    pub baseline: Pixels,
    pub x_height: Pixels,
    /// Decoration and bar-cursor thickness.
    pub stroke: Pixels,
    pub ic_width: Pixels,
}
impl CellMetrics {
    pub fn measure(font: &Font, window: &Window) -> Option<Self> {
        let ts = window.text_system();
        let id = ts.resolve_font(font);
        // Width and height follow Ghostty font/Metrics.zig: the face's advance and its
        // ascent + descent, rounded to backing pixels so every cell starts on a pixel edge.
        // Glyphs are then placed per cell rather than at their shaped advances.
        let advance = ts.advance(id, px(font_size()), 'm').ok()?.width;
        let scale = window.scale_factor();
        let width = px((f32::from(advance) * scale).round().max(1.) / scale);
        // GPUI's TextSystem reports descent as a negative distance; ShapedLine::paint
        // uses it positive, which TextSystem::baseline_offset gets wrong by one descent.
        let ascent = ts.ascent(id, px(font_size()));
        let descent = -ts.descent(id, px(font_size()));
        let height = px((f32::from(ascent + descent)
            * crate::ui::terminal_text::line_height()
            * scale)
            .round()
            / scale);
        let ic_width = ts
            .advance(id, px(font_size()), '水')
            .ok()
            .filter(|s| s.width > px(0.))
            .map(|s| s.width)
            .unwrap_or_else(|| {
                let mut top = px(0.);
                let mut bottom = px(0.);
                for ch in ' '..='~' {
                    if let Ok(b) = ts.typographic_bounds(id, px(font_size()), ch) {
                        top = top.min(b.top());
                        bottom = bottom.max(b.bottom());
                    }
                }
                (bottom - top).min(advance * 2.)
            });
        (advance > px(0.)).then_some(Self {
            width,
            advance,
            height,
            baseline: (height - ascent - descent) / 2. + ascent,
            x_height: ts.x_height(id, px(font_size())),
            stroke: px((font_size() / 14.).max(1.)),
            ic_width,
        })
    }
    pub fn cursor_bounds(self, bounds: Bounds<Pixels>, cursor: (u16, u16)) -> Bounds<Pixels> {
        Bounds::new(
            point(
                bounds.left() + self.width * cursor.0 as f32,
                bounds.top() + self.height * cursor.1 as f32,
            ),
            size(self.width, self.height),
        )
    }
}

#[derive(Default)]
pub struct CursorAnchor(Option<(u16, u16)>);
impl CursorAnchor {
    pub fn observe(&mut self, cursor: Option<(u16, u16)>) {
        if cursor.is_some() {
            self.0 = cursor;
        }
    }
    pub fn get(&self) -> Option<(u16, u16)> {
        self.0
    }
}

pub struct MarkedText {
    pub line: ShapedLine,
    pub bounds: Bounds<Pixels>,
}
impl MarkedText {
    pub fn new(text: &str, font: Font, anchor: Bounds<Pixels>, window: &Window) -> Self {
        let line = window.text_system().shape_line(
            SharedString::from(text.to_owned()),
            px(font_size()),
            &[TextRun {
                len: text.len(),
                font,
                color: hsla(crate::ui::theme::Theme::terminal_fg()),
                background_color: None,
                underline: Some(UnderlineStyle {
                    color: Some(hsla(crate::ui::theme::Theme::terminal_fg())),
                    thickness: px(crate::ui::theme::Theme::BORDER_WIDTH),
                    wavy: false,
                }),
                strikethrough: None,
            }],
            None,
        );
        let bounds = Bounds::new(
            anchor.origin,
            size(
                anchor.size.width * (line.width() / anchor.size.width).ceil().max(1.),
                anchor.size.height,
            ),
        );
        Self { line, bounds }
    }
    pub fn range_bounds(&self, range: std::ops::Range<usize>) -> Bounds<Pixels> {
        let start = utf16_byte_offset(&self.line.text, range.start);
        let end = utf16_byte_offset(&self.line.text, range.end).max(start);
        let x = self.line.x_for_index(start);
        Bounds::new(
            point(self.bounds.left() + x, self.bounds.top()),
            size(self.line.x_for_index(end) - x, self.bounds.size.height),
        )
    }
    pub fn paint(&self, background: u32, window: &mut Window, cx: &mut App) {
        window.paint_quad(fill(self.bounds, hsla(background)));
        window.with_glyph_dilation(Some(0), |window| {
            let _ = self.line.paint(
                self.bounds.origin,
                self.bounds.size.height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        });
    }
}
fn utf16_byte_offset(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units + ch.len_utf16() > offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

fn shape_cell(
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
// Printable ASCII has no shaping context when terminal ligatures are disabled.
// Shape this bounded alphabet once per face/style, then reuse the exact native
// glyph IDs. Non-ASCII and fonts whose advances aren't one cell retain shaping.
struct AsciiGlyphs {
    glyphs: [(gpui::FontId, gpui::GlyphId); 95],
    baseline: Pixels,
}
impl AsciiGlyphs {
    fn new(font: Font, metrics: CellMetrics, window: &Window) -> Option<Self> {
        if !["liga", "calt"].iter().all(|tag| {
            font.features
                .tag_value_list()
                .iter()
                .any(|(name, value)| name == tag && *value == 0)
        }) {
            return None;
        }
        let text: String = (' '..='~').collect();
        let line = window.text_system().shape_line(
            text.into(),
            px(font_size()),
            &[TextRun {
                len: 95,
                font,
                color: hsla(crate::ui::theme::Theme::terminal_fg()),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let mut glyphs = [None; 95];
        for run in &line.runs {
            for glyph in &run.glyphs {
                if glyph.index >= 95
                    || glyphs[glyph.index].is_some()
                    || glyph.is_emoji
                    || f32::from(glyph.position.x - metrics.advance * glyph.index as f32).abs()
                        > 0.02
                    || glyph.position.y != px(0.)
                {
                    return None;
                }
                glyphs[glyph.index] = Some((run.font_id, glyph.id));
            }
        }
        if glyphs.iter().any(Option::is_none)
            || f32::from(line.width() - metrics.advance * 95.).abs() > 0.02
        {
            return None;
        }
        Some(Self {
            glyphs: glyphs.map(Option::unwrap),
            baseline: (metrics.height - line.ascent - line.descent) / 2. + line.ascent,
        })
    }
    fn paint(
        &self,
        text: &str,
        origin: gpui::Point<Pixels>,
        metrics: CellMetrics,
        color: Hsla,
        window: &mut Window,
    ) {
        for (col, byte) in text.bytes().enumerate() {
            if byte == b' ' {
                continue;
            }
            let (font, glyph) = self.glyphs[(byte - b' ') as usize];
            let _ = window.paint_glyph(
                point(
                    origin.x + metrics.width * col as f32,
                    origin.y + self.baseline,
                ),
                font,
                glyph,
                px(font_size()),
                color,
            );
        }
    }
}
fn run_font(run: &Run, f: &Font) -> Font {
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
fn printable_cells(run: &Run) -> bool {
    run.text.len() == run.cols as usize && run.text.bytes().all(|b| (b' '..=b'~').contains(&b))
}
/// Current viewport only: no history of strings from a long-running terminal.
#[derive(Default)]
pub struct LayoutCache {
    key: Option<(Font, CellMetrics, u64)>,
    rows: Vec<Vec<(Run, ShapedLine, Pixels)>>,
    ascii: [Option<Option<AsciiGlyphs>>; 4],
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

fn text_color(run: &Run) -> Hsla {
    hsla(run.fg).opacity(if run.attrs.faint {
        crate::ui::terminal_text::faint()
    } else {
        1.
    })
}

#[cfg(test)]
#[path = "screen_tests.rs"]
mod tests;

/// Run by the isolated native-render harness, using macOS fonts rather than GPUI's
/// mock text system. Both IME range geometry and the painted overlay use this layout.
#[cfg(feature = "visual-test")]
pub fn verify_native_geometry(window: &Window, cx: &App) {
    let font = crate::ui::theme::Theme::mono(cx);
    let metrics = CellMetrics::measure(&font, window).expect("native font measurement");
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut face = font.clone();
        if bold {
            face.weight = FontWeight::BOLD;
        }
        if italic {
            face.style = FontStyle::Italic;
        }
        let cached =
            AsciiGlyphs::new(face.clone(), metrics, window).expect("native mono ASCII cache");
        for text in [
            "0123456789 row 01",
            "!=>->==fiWWW_[]{}~",
            "ASCII with spaces",
        ] {
            let run = Run {
                col: 0,
                cols: text.len() as u16,
                text: text.into(),
                fg: DEFAULT_FG,
                bg: None,
                attrs: Attrs::default(),
            };
            let (line, _) = shape_cell(&run, face.clone(), metrics, window);
            assert_eq!(
                cached.baseline,
                (metrics.height - line.ascent - line.descent) / 2. + line.ascent
            );
            for shaped in &line.runs {
                for g in &shaped.glyphs {
                    assert_eq!(
                        cached.glyphs[(text.as_bytes()[g.index] - b' ') as usize],
                        (shaped.font_id, g.id)
                    );
                    assert!(
                        f32::from(g.position.x - metrics.advance * g.index as f32).abs() < 0.02
                    );
                }
            }
        }
    }
    for ch in [
        '\u{e0b0}', '\u{e0b2}', '\u{e0a0}', '\u{f120}', '\u{f07b}', '\u{f013}',
    ] {
        let run = Run {
            col: 0,
            cols: 1,
            text: ch.to_string(),
            fg: DEFAULT_FG,
            bg: None,
            attrs: Attrs::default(),
        };
        let (line, offset) = shape_cell(&run, font.clone(), metrics, window);
        assert!(
            line.runs.iter().any(|r| !r.glyphs.is_empty())
                && line
                    .runs
                    .iter()
                    .flat_map(|r| &r.glyphs)
                    .all(|g| g.id.0 != 0),
            "missing symbol {ch}"
        );
        assert!(
            (f32::from(line.width() + offset * 2. - metrics.width)).abs() < 0.1,
            "symbol not aligned: {ch}"
        );
    }
    eprintln!("native bundled symbol font: six Nerd/Powerline glyphs present and cell-aligned");
    let pane = Bounds::new(point(px(37.), px(83.)), size(px(700.), px(500.)));
    let anchor = metrics.cursor_bounds(pane, (11, 5));
    assert_eq!(
        anchor.origin,
        point(
            pane.left() + metrics.width * 11.,
            pane.top() + metrics.height * 5.
        )
    );
    for text in ["nihao", "n你😀hao", "e\u{301}", "👨‍👩‍👧‍👦🇨🇳👍🏽"] {
        let marked = MarkedText::new(text, font.clone(), anchor, window);
        assert_eq!(marked.bounds.origin, anchor.origin);
        let whole = marked.range_bounds(0..text.encode_utf16().count());
        assert_eq!(whole.origin, marked.bounds.origin);
        assert_eq!(whole.size.width, marked.line.width());
        assert_eq!(whole.size.height, metrics.height);
        for (byte, _) in text.char_indices() {
            let index = text[..byte].encode_utf16().count();
            let caret = marked.range_bounds(index..index);
            assert_eq!(
                caret.left(),
                marked.bounds.left() + marked.line.x_for_index(byte)
            );
            assert_eq!(caret.bottom(), marked.bounds.bottom());
            assert_eq!(caret.size.width, px(0.));
            let suffix = marked.range_bounds(index..text.encode_utf16().count());
            assert_eq!(suffix.left(), caret.left());
            // origin + width re-adds the caret offset; widths like 7.8 are not binary-exact.
            let end = marked.bounds.left() + marked.line.width();
            assert!(
                f32::from(suffix.right() - end).abs() < 1e-3,
                "{text}: {suffix:?}"
            );
        }
    }
    for text in ["实", "现", "反", "馈", "日", "語", "한", "글", "😀"] {
        let run = Run {
            col: 0,
            cols: 2,
            text: text.into(),
            fg: DEFAULT_FG,
            bg: None,
            attrs: Attrs::default(),
        };
        let (line, offset) = shape_cell(&run, font.clone(), metrics, window);
        let target = metrics.width * run.cols as f32;
        assert!(
            (f32::from(line.width() + offset * 2. - target)).abs() < 0.1,
            "wide cell {text}: advance={:?} inset={offset:?} two-columns={target:?}",
            line.width()
        );
        assert!(
            line.width() <= target + px(0.1),
            "wide glyph overflow {text}"
        );
        eprintln!(
            "wide-cell {text}: advance={:?}, two-columns={target:?}, inset={offset:?}",
            line.width()
        );
    }
    eprintln!("native IME geometry: overlay/ranges/UTF16 agree; measured cell={metrics:?}");
}
