#[path = "screen_extras.rs"]
mod extras;
#[path = "sprites.rs"]
pub(super) mod sprites;
use extras::paint_decorations;
#[allow(unused_imports)]
pub use extras::{paint_guide, paint_selected_text};
#[path = "screen_ascii.rs"]
mod ascii;
#[cfg(feature = "visual-test")]
#[path = "screen_native.rs"]
mod native;
#[path = "screen_paint.rs"]
mod paint;
#[path = "screen_capture.rs"]
mod vt_capture;
use ascii::AsciiGlyphs;
#[cfg(feature = "visual-test")]
#[allow(unused_imports)]
pub use native::verify_native_geometry;
use paint::printable_cells;
#[allow(unused_imports)]
pub use paint::{CursorPaint, LayoutCache, paint_cached};
pub(super) use paint::{run_font, shape_cell, text_color};
#[allow(unused_imports)]
pub use vt_capture::Capture;
#[cfg(any(test, feature = "visual-test"))]
#[allow(unused_imports)]
pub use vt_capture::capture;
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

#[cfg(test)]
#[path = "screen_tests.rs"]
mod tests;
