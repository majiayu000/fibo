pub use libghostty_vt;
use libghostty_vt::{
    Terminal,
    snapshot::Decoder,
    style::RgbColor,
    terminal::{
        ColorScheme, ConformanceLevel, DeviceAttributeFeature, DeviceAttributes, DeviceType,
        PrimaryDeviceAttributes, SecondaryDeviceAttributes, SizeReportSize,
        TertiaryDeviceAttributes,
    },
};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

pub use crate::scheme::*;

/// ANSI 0-15 as the terminal has them when no theme says otherwise.
pub fn builtin_ansi() -> &'static [u32; 16] {
    static COLORS: std::sync::OnceLock<[u32; 16]> = std::sync::OnceLock::new();
    COLORS.get_or_init(|| {
        let mut out = [0; 16];
        if let Ok(vt) = Vt::new(1, 1)
            && let Ok(palette) = vt.0.default_color_palette()
        {
            for (slot, c) in out.iter_mut().zip(palette.0) {
                *slot = u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b);
            }
        }
        out
    })
}
/// Reported for XTVERSION (CSI > q), like "ghostty 1.2.3".
pub const XTVERSION: &str = concat!("fibo ", env!("CARGO_PKG_VERSION"));
/// Matches the TERM the daemon gives programs; reported for XTGETTCAP TN.
pub const TERMINFO: &str = "xterm-256color";
// DA1 "CSI ? 62 ; 22 c" and DA2 "CSI > 1 ; 10 ; 0 c": what Ghostty and xterm-like
// terminals answer, so capability probes finish instead of waiting for a timeout.
const DEVICE_ATTRIBUTES: DeviceAttributes = DeviceAttributes {
    primary: PrimaryDeviceAttributes::new(
        ConformanceLevel::LEVEL_2,
        &[DeviceAttributeFeature::ANSI_COLOR],
    ),
    secondary: SecondaryDeviceAttributes {
        device_type: DeviceType::VT220,
        firmware_version: 10,
        rom_cartridge: 0,
    },
    tertiary: TertiaryDeviceAttributes { unit_id: 0 },
};

fn rgb(value: u32) -> RgbColor {
    RgbColor {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    }
}

/// Bytes that clear scrollback and the rows above the cursor, keeping the cursor row
/// (usually the prompt) at the top: what Cmd+K does in Ghostty and Terminal.app. Empty
/// in the alternate screen, where a full-screen program owns every row.
/// The leading CAN ends any sequence the program left half-written, so the clear
/// never fuses with its output.
pub fn clear_history(t: &Terminal) -> io::Result<Vec<u8>> {
    use libghostty_vt::screen::Screen;
    if t.active_screen().map_err(io::Error::other)? == Screen::Alternate {
        return Ok(Vec::new());
    }
    let y = t.cursor_y().map_err(io::Error::other)?;
    let mut bytes = b"\x18".to_vec();
    if y > 0 {
        bytes.extend(format!("\x1b[{y}S\x1b[{y}A").bytes());
    }
    bytes.extend(b"\x1b[3J");
    Ok(bytes)
}

/// One cell's size in device pixels, as the display last measured it; 0 until known.
/// Shared with the size-report callback, which runs inside `feed`.
#[derive(Clone, Default)]
pub struct CellPixels(Arc<AtomicU64>);
impl CellPixels {
    pub fn set(&self, width: u32, height: u32) {
        self.0.store(
            u64::from(width) << 32 | u64::from(height),
            Ordering::Relaxed,
        );
    }
    pub fn get(&self) -> (u32, u32) {
        let v = self.0.load(Ordering::Relaxed);
        ((v >> 32) as u32, v as u32)
    }
}

/// One occurrence found by `Vt::search`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    /// Row counted from the top of history, as `Vt::viewport` counts.
    pub row: usize,
    pub col: u16,
    /// Width in cells; a wide character counts two.
    pub cols: u16,
    /// Shared by every occurrence on this row.
    pub line: Arc<str>,
}
/// A terminal may move between threads, but every access must be exclusive.
/// Ghostty callbacks capture only Send values; no platform/UI handles live here.
pub struct Vt(Terminal<'static, 'static>);
// SAFETY: owned native state, no concurrent calls; callbacks are installed only by
// the daemon with Send captures. Vt is not Sync and is held behind a Mutex.
unsafe impl Send for Vt {}

impl Vt {
    pub fn terminal(&self) -> &Terminal<'static, 'static> {
        &self.0
    }
    pub fn scroll(&mut self, scroll: libghostty_vt::terminal::ScrollViewport) {
        self.0.scroll_viewport(scroll);
    }
    /// Explicit user search only: reads the bounded VT history, never the transcript directory.
    /// Every occurrence, as its history row, first column and width in cells, plus the row.
    pub fn search(&self, query: &str) -> io::Result<Vec<SearchMatch>> {
        if query.is_empty() {
            return Ok(vec![]);
        }
        let mut matches = Vec::new();
        for y in 0..self.0.total_rows().map_err(io::Error::other)? {
            let cells = self.row_cells(y)?;
            let line: String = cells.iter().map(|(_, s)| s.as_str()).collect();
            if !line.contains(query) {
                continue;
            }
            let text: Arc<str> = line.trim_end().into();
            // Byte offset of each cell's text, to turn a byte match back into columns.
            let mut starts = Vec::with_capacity(cells.len());
            let mut at = 0;
            for (_, s) in &cells {
                starts.push(at);
                at += s.len();
            }
            for (byte, _) in line.match_indices(query) {
                let first = starts.partition_point(|&b| b <= byte).saturating_sub(1);
                let last = starts
                    .partition_point(|&b| b < byte + query.len())
                    .saturating_sub(1);
                let col = cells[first].0;
                // Spacer cells are left out, so a wide character ends where the next begins.
                let end = cells.get(last + 1).map_or(cells[last].0 + 1, |c| c.0);
                matches.push(SearchMatch {
                    row: y,
                    col,
                    cols: end.saturating_sub(col),
                    line: Arc::clone(&text),
                });
            }
        }
        Ok(matches)
    }
    /// Where the viewport sits in the scrollable rows: (first shown row, total rows, shown
    /// rows). Rows count from the top of history, as `search` does.
    pub fn viewport(&self) -> io::Result<(usize, usize, usize)> {
        let s = self.0.scrollbar().map_err(io::Error::other)?;
        Ok((s.offset as usize, s.total as usize, s.len as usize))
    }
    /// Each ask's history row, looked for from the bottom up over the left of each row
    /// (see `prompt_pin::locate`). Reads at most `PREFIX_COLS` cells of `MAX_ROWS` rows.
    pub fn find_prompts(
        &self,
        keys: &[Option<String>],
        prompt: &crate::agent::PromptLine,
    ) -> io::Result<Vec<Option<usize>>> {
        use crate::prompt_pin::{MAX_ROWS, PREFIX_COLS};
        let total = self.0.total_rows().map_err(io::Error::other)?;
        let mut error = None;
        let rows = (total.saturating_sub(MAX_ROWS)..total)
            .rev()
            .map_while(|y| match self.row_cells_upto(y, PREFIX_COLS) {
                Ok(cells) => Some((y, cells.into_iter().map(|(_, s)| s).collect::<String>())),
                Err(e) => {
                    error = Some(e);
                    None
                }
            });
        let found = crate::prompt_pin::locate(keys, prompt, rows);
        match error {
            Some(e) => Err(e),
            None => Ok(found),
        }
    }
    /// The last `rows` rows of history plus screen, one line per row.
    pub fn tail_text(&self, rows: usize) -> io::Result<String> {
        let total = self.0.total_rows().map_err(io::Error::other)?;
        let mut text = String::new();
        for y in total.saturating_sub(rows)..total {
            text.push_str(self.row(y)?.trim_end());
            text.push('\n');
        }
        Ok(text)
    }
    fn row(&self, y: usize) -> io::Result<String> {
        Ok(self.row_cells(y)?.into_iter().map(|(_, s)| s).collect())
    }
    /// Each cell's column and text; a wide character's spacer cell is left out.
    fn row_cells(&self, y: usize) -> io::Result<Vec<(u16, String)>> {
        self.row_cells_upto(y, u16::MAX)
    }
    /// The same for the first `limit` columns only.
    fn row_cells_upto(&self, y: usize, limit: u16) -> io::Result<Vec<(u16, String)>> {
        use libghostty_vt::{
            screen::CellWide,
            terminal::{Point, PointCoordinate},
        };
        let cols = self.0.cols().map_err(io::Error::other)?.min(limit);
        let mut line = Vec::with_capacity(cols as usize);
        for x in 0..cols {
            let cell = self
                .0
                .grid_ref(Point::Screen(PointCoordinate { x, y: y as u32 }))
                .map_err(io::Error::other)?;
            if matches!(
                cell.cell()
                    .and_then(|c| c.wide())
                    .map_err(io::Error::other)?,
                CellWide::SpacerHead | CellWide::SpacerTail
            ) {
                continue;
            }
            let mut chars = vec!['\0'; 32];
            let len = match cell.graphemes(&mut chars) {
                Ok(n) => n,
                Err(libghostty_vt::Error::OutOfSpace { required }) => {
                    chars.resize(required, '\0');
                    cell.graphemes(&mut chars).map_err(io::Error::other)?
                }
                Err(e) => return Err(io::Error::other(e)),
            };
            line.push((
                x,
                if len == 0 {
                    " ".to_owned()
                } else {
                    chars[..len].iter().collect()
                },
            ));
        }
        Ok(line)
    }
    pub fn on_reply(&mut self, mut callback: impl FnMut(&[u8]) + Send + 'static) -> io::Result<()> {
        self.0
            .on_pty_write(move |_, bytes| callback(bytes))
            .map_err(io::Error::other)?;
        Ok(())
    }
    /// BEL (0x07) from the program; a BEL ending an OSC string is not a bell.
    pub fn on_bell(&mut self, mut callback: impl FnMut() + Send + 'static) -> io::Result<()> {
        self.0
            .on_bell(move |_| callback())
            .map_err(io::Error::other)?;
        Ok(())
    }
    /// OSC 52 / 5522 clipboard writes from the program, as plain text. The display client
    /// owns the clipboard, so only it installs this; an empty write clears the clipboard.
    pub fn on_clipboard_text(
        &mut self,
        mut callback: impl FnMut(String) + Send + 'static,
    ) -> io::Result<()> {
        self.0
            .on_clipboard_write(move |_, write| {
                let text = write
                    .contents()
                    .find(|c| c.mime.starts_with("text/plain"))
                    .map(|c| String::from_utf8_lossy(c.data).into_owned());
                if let Some(text) =
                    text.or_else(|| write.contents().next().is_none().then(String::new))
                {
                    callback(text);
                    write.reply(Ok(()), false);
                }
            })
            .map_err(io::Error::other)?;
        Ok(())
    }
    /// The active selection as copied text (Ghostty's selectionString: unwrapped, trimmed).
    /// The selection lives on the terminal, so it stays on its text as output scrolls.
    pub fn selection_text(&self) -> io::Result<Option<String>> {
        use libghostty_vt::{fmt::Format, selection::FormatOptions};
        let options = FormatOptions::new()
            .with_emit_format(Format::Plain)
            .with_unwrap(true)
            .with_trim(true);
        Ok(self
            .0
            .format_selection_alloc(None, options)
            .map_err(io::Error::other)?
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()))
    }
    /// The title the program set with OSC 0/2, if any.
    pub fn title(&self) -> Option<String> {
        self.0
            .title()
            .ok()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    }
    /// The directory the shell last reported with OSC 7, when it is on `host` (this
    /// machine); a shell over ssh reports the remote one, which no local tab can open.
    pub fn pwd(&self, host: &str) -> Option<String> {
        local_path(self.0.pwd().ok()?, host)
    }
    /// Answer the queries programs send every terminal: device attributes, XTVERSION,
    /// window/cell size and the color scheme its background implies. Colors are answered from the
    /// defaults set in `new`. Only the daemon installs this; display copies stay silent.
    pub fn answer_queries(&mut self, cell: CellPixels) -> io::Result<()> {
        self.0
            .on_device_attributes(|_| Some(DEVICE_ATTRIBUTES))
            .and_then(|t| t.on_xtversion(|_| Some(XTVERSION)))
            .and_then(|t| {
                t.on_color_scheme(|t| {
                    let bg = t.bg_color().ok().flatten()?;
                    // Rec. 601 luma: a light background reads as the light scheme.
                    let luma = 299 * bg.r as u32 + 587 * bg.g as u32 + 114 * bg.b as u32;
                    Some(if luma > 128_000 {
                        ColorScheme::Light
                    } else {
                        ColorScheme::Dark
                    })
                })
            })
            .and_then(|t| {
                t.on_size(move |t| {
                    let (cell_width, cell_height) = cell.get();
                    Some(SizeReportSize {
                        rows: t.rows().ok()?,
                        columns: t.cols().ok()?,
                        cell_width,
                        cell_height,
                    })
                })
            })
            .and_then(|t| t.set_terminfo_name(TERMINFO))
            .map_err(io::Error::other)?;
        Ok(())
    }
    pub fn new(cols: u16, rows: u16) -> io::Result<Self> {
        let mut terminal = Terminal::new(cols, rows).map_err(io::Error::other)?;
        terminal
            .set_scrollback_max_lines(Some(5_000))
            .map_err(io::Error::other)?;
        terminal
            .set_scrollback_max_bytes(Some(16 * 1024 * 1024))
            .map_err(io::Error::other)?;
        terminal
            .set_continuation_max_bytes(1024 * 1024)
            .map_err(io::Error::other)?;
        terminal
            .set_mode(libghostty_vt::terminal::Mode::GRAPHEME_CLUSTER, true)
            .map_err(io::Error::other)?;
        // As Ghostty: the cursor blinks until a program asks for a steady one, and a
        // DECSCUSR reset (CSI 0 q) returns to blinking.
        terminal
            .set_mode(libghostty_vt::terminal::Mode::CURSOR_BLINKING, true)
            .and_then(|t| t.set_default_cursor_blink(Some(true)))
            .map_err(io::Error::other)?;
        let mut vt = Self(terminal);
        vt.set_scheme(Scheme::Dark)?;
        Ok(vt)
    }
    /// Switch the defaults; colors a program set itself (OSC, explicit SGR) are kept.
    pub fn set_scheme(&mut self, scheme: Scheme) -> io::Result<()> {
        self.set_colors(scheme.colors())
    }
    /// The cursor's shape until a program asks for one, and again after it resets (`CSI 0 q`).
    pub fn set_default_cursor(
        &mut self,
        style: libghostty_vt::terminal::CursorStyle,
    ) -> io::Result<()> {
        self.0
            .set_default_cursor_style(Some(style))
            .map(|_| ())
            .map_err(io::Error::other)
    }
    /// The same for any colors, a user's theme included.
    pub fn set_colors(&mut self, colors: TermColors) -> io::Result<()> {
        // Back to the built-in palette first, so a theme only replaces ANSI 0-15.
        let t = self
            .0
            .set_default_fg_color(Some(rgb(colors.fg)))
            .and_then(|t| t.set_default_bg_color(Some(rgb(colors.bg))))
            .and_then(|t| t.set_default_cursor_color(Some(rgb(colors.cursor))))
            .and_then(|t| t.set_default_color_palette(None))
            .map_err(io::Error::other)?;
        let ansi = colors.ansi();
        if ansi.iter().any(Option::is_some) {
            let mut palette = t.default_color_palette().map_err(io::Error::other)?;
            for (slot, color) in palette.0.iter_mut().zip(ansi) {
                if let Some(color) = color {
                    *slot = rgb(color);
                }
            }
            t.set_default_color_palette(Some(palette))
                .map_err(io::Error::other)?;
        }
        Ok(())
    }
    pub fn snapshot(&mut self) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.0
            .encode_snapshot(&mut bytes)
            .map_err(io::Error::other)?;
        Ok(bytes)
    }
    /// The screen as escape sequences any terminal emulator replays (xterm.js on a
    /// phone, another Ghostty): modes, colors, cursor and the active screen's cells. Unlike
    /// `snapshot`, which only a Ghostty of the same build can read back.
    pub fn replay(&self) -> io::Result<Vec<u8>> {
        use libghostty_vt::fmt::{Format, Formatter, FormatterOptions};
        let options = FormatterOptions::new()
            .with_format(Format::Vt)
            .with_palette(true)
            .with_modes(true)
            .with_scrolling_region(true)
            .with_tabstops(true)
            .with_pwd(true)
            .with_keyboard(true)
            .with_cursor(true)
            .with_style(true)
            .with_hyperlink(true)
            .with_protection(true)
            .with_kitty_keyboard(true)
            .with_charsets(true);
        let mut formatter = Formatter::new(&self.0, options).map_err(io::Error::other)?;
        Ok(formatter
            .format_alloc(None)
            .map_err(io::Error::other)?
            .to_vec())
    }
    pub fn restore(bytes: &[u8]) -> io::Result<Self> {
        Decoder::new_buf(bytes)
            .and_then(|d| d.decode())
            .map(Self)
            .map_err(io::Error::other)
    }
    pub fn feed(&mut self, bytes: &[u8]) {
        self.0.vt_write(bytes);
    }
    pub fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        self.0.resize(cols, rows, 0, 0).map_err(io::Error::other)
    }
}

/// Both supported CLIs mark their input with an arrow. Bracketed paste alone
/// is not readiness: Claude also enables it on theme and security notice pages.
/// Use the last marker so an old menu above a new input does not block placement.
pub fn agent_input_ready(tail: &str) -> bool {
    let mut menu_footer = false;
    let selected = tail.lines().rev().find_map(|l| {
        let lower = l.to_ascii_lowercase();
        menu_footer |= lower.contains("enter to confirm")
            || lower.contains("enter to select")
            || lower.contains("enter to continue");
        let l = l.trim_start_matches(['│', ' ']);
        ["❯", "›", ">"].iter().find_map(|m| {
            l.strip_prefix(m)
                .filter(|s| s.is_empty() || s.starts_with(char::is_whitespace))
                .map(str::trim_start)
        })
    });
    // Claude 2.1.302 also uses unnumbered trust choices. Inspect only below
    // the current marker so a previous menu cannot poison a later input box.
    if menu_footer {
        return false;
    }
    let Some(selected) = selected else {
        return false;
    };
    if selected.is_empty() {
        return true;
    }
    let digits = selected.bytes().take_while(u8::is_ascii_digit).count();
    !(tail.contains("Choose the text style") || digits > 0 && selected[digits..].starts_with('.'))
}
/// zsh, bash and fish wording for a command that is not on PATH, near the prompt.
pub fn command_missing(tail: &str) -> bool {
    tail.lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(4)
        .any(|l| l.contains("command not found") || l.contains("Unknown command"))
}
/// A program's window title as a name: Claude Code and Codex lead theirs with a spinner
/// or activity glyph that changes many times a second ("✳ fix login" -> "fix login").
pub fn shown_title(raw: &str) -> Option<String> {
    let title = raw.trim();
    let mut chars = title.chars();
    let first = chars.next()?;
    let rest = chars.as_str();
    let glyph = matches!(first, '\u{2800}'..='\u{28ff}') || "·✢✳✶✻✽◐◓◑◒".contains(first);
    let title = if glyph && rest.chars().next().is_none_or(char::is_whitespace) {
        rest.trim()
    } else {
        title
    };
    (!title.is_empty()).then(|| title.to_owned())
}

/// The path of an OSC 7 `file://host/path` report, percent-decoded, when `host` names
/// this machine (empty, localhost, or `this` with or without its `.local` suffix).
pub fn local_path(url: &str, this: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let at = rest.find('/')?;
    let (host, path) = rest.split_at(at);
    let bare = |h: &str| h.trim_end_matches(".local").to_ascii_lowercase();
    if !(host.is_empty() || host.eq_ignore_ascii_case("localhost") || bare(host) == bare(this)) {
        return None;
    }
    let mut bytes = Vec::with_capacity(path.len());
    let mut raw = path.bytes();
    while let Some(b) = raw.next() {
        if b == b'%' {
            let hex = [raw.next()?, raw.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
