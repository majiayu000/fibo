//! Terminal typography shared by settings, zoom shortcuts and the cell renderer.
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        OnceLock, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// What a split without focus shows where its cursor is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnfocusedCursor {
    /// Only the focused pane has a cursor, as in tmux.
    #[default]
    Hidden,
    /// An outline, as Ghostty draws it.
    Hollow,
}

/// The cursor's shape until a program asks for another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorDefault {
    #[default]
    Block,
    Bar,
    Underline,
}

/// Where ⌘-clicking `file.rs:42` in a terminal opens it, at that line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Editor {
    /// The first of Zed, VS Code and Cursor that is installed, else the default app.
    #[default]
    Auto,
    Zed,
    VsCode,
    Cursor,
    /// `$EDITOR` in a new terminal tab.
    Terminal,
    /// The file's default app, which ignores the line.
    System,
}

/// What Option does with a key that types something.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionKey {
    /// Types the character when the layout puts a plain ASCII one there (`@`, `[`, `|` on
    /// German, French or Nordic keyboards), else acts as Alt (US and ABC keep Alt-b, Alt-f).
    #[default]
    Auto,
    /// Always Alt: Esc before the key, as readline and Emacs expect.
    Alt,
    /// Always the character macOS composes, accents and dead keys included.
    Text,
}

/// Fields added later fall back to their defaults in an older file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalText {
    /// Empty selects the bundled default face.
    pub family: String,
    pub size: f32,
    /// Multiplies the face's natural cell height, keeping the glyph centered.
    pub line_height: f32,
    /// How much a split without focus fades toward its background, 0 for none.
    pub unfocused_dim: f32,
    pub unfocused_cursor: UnfocusedCursor,
    /// The focused cursor blinks unless the program asked for a steady one.
    pub cursor_blink: bool,
    pub editor: Editor,
    pub option_key: OptionKey,
    /// What everything past ASCII is drawn in; empty uses `family` and its fallbacks.
    pub non_ascii_family: String,
    /// Lets the font join `=>` and the like. Off, every cell can be redrawn on its own.
    pub ligatures: bool,
    pub cursor_shape: CursorDefault,
    /// A faint band along the focused terminal's cursor row.
    pub cursor_guide: bool,
    /// Text below this contrast with its background is moved until it reaches it; 1 is off.
    pub min_contrast: f32,
    /// The opacity of SGR 2 (faint) text.
    pub faint: f32,
    /// How much every terminal fades while the window is in the background, 0 for none.
    pub window_dim: f32,
    /// Empty points between a terminal's edge and its cells.
    pub padding: f32,
}
impl Default for TerminalText {
    fn default() -> Self {
        Self {
            family: String::new(),
            size: super::theme::Theme::TERMINAL_FONT_SIZE,
            line_height: 1.,
            // Off unless chosen: the focus ring and the lone cursor already mark the pane.
            unfocused_dim: 0.,
            unfocused_cursor: UnfocusedCursor::Hidden,
            cursor_blink: true,
            editor: Editor::Auto,
            option_key: OptionKey::Auto,
            non_ascii_family: String::new(),
            ligatures: false,
            cursor_shape: CursorDefault::Block,
            cursor_guide: false,
            min_contrast: 1.,
            // Ghostty's faint-opacity default.
            faint: 0.5,
            window_dim: 0.,
            padding: 0.,
        }
    }
}
impl TerminalText {
    fn parse(text: &str) -> Result<Self, String> {
        let value: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if !(8. ..=36.).contains(&value.size) || !(1. ..=2.).contains(&value.line_height) {
            return Err("字号须为 8–36，行距须为 1–2 倍".into());
        }
        if !(0. ..=0.5).contains(&value.unfocused_dim) {
            return Err("非当前分屏变暗须为 0–0.5".into());
        }
        if !(0. ..=0.5).contains(&value.window_dim) {
            return Err("后台窗口变暗须为 0–0.5".into());
        }
        if !(1. ..=21.).contains(&value.min_contrast) {
            return Err("最低对比度须为 1–21".into());
        }
        if !(0.1..=1.).contains(&value.faint) {
            return Err("弱化文字须为 0.1–1".into());
        }
        if !(0. ..=40.).contains(&value.padding) {
            return Err("终端边距须为 0–40".into());
        }
        Ok(value)
    }
    fn load() -> Self {
        let Some(path) = path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|e| {
                eprintln!("mooring: reading {}: {e}", path.display());
                Self::default()
            }),
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("mooring: reading {}: {e}", path.display());
                }
                Self::default()
            }
        }
    }
}
fn state() -> &'static RwLock<TerminalText> {
    static TEXT: OnceLock<RwLock<TerminalText>> = OnceLock::new();
    TEXT.get_or_init(|| RwLock::new(TerminalText::load()))
}
pub fn current() -> TerminalText {
    state().read().unwrap().clone()
}
pub fn size() -> f32 {
    state().read().unwrap().size
}
pub fn option_key() -> OptionKey {
    state().read().unwrap().option_key
}
pub fn line_height() -> f32 {
    state().read().unwrap().line_height
}
pub fn padding_of() -> f32 {
    state().read().unwrap().padding
}
pub fn faint() -> f32 {
    state().read().unwrap().faint
}
pub fn min_contrast() -> f32 {
    state().read().unwrap().min_contrast
}
pub fn cursor_shape() -> CursorDefault {
    state().read().unwrap().cursor_shape
}
/// Changes whenever a setting does, for what is laid out once from them.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}
/// The terminal's face from the default one. A family that is not installed (typed into
/// the file, or uninstalled since) would fall through to a proportional face whose letters
/// miss the cells; the monospace default stands in, and the setting stays as you wrote it.
pub fn face(mut face: gpui::Font, installed: impl Fn(&str) -> bool) -> gpui::Font {
    let text = state().read().unwrap();
    if !text.family.is_empty() && installed(&text.family) {
        face.family = text.family.clone().into();
    }
    if text.ligatures {
        face.features = Default::default();
    }
    face
}
/// The face for text past ASCII, given the terminal's own.
pub fn non_ascii_face(face: &gpui::Font) -> gpui::Font {
    let mut out = face.clone();
    let text = state().read().unwrap();
    if !text.non_ascii_family.is_empty() {
        out.family = text.non_ascii_family.clone().into();
    }
    out
}
pub fn path() -> Option<PathBuf> {
    super::status_colors::config_dir().map(|dir| dir.join("terminal-text.json"))
}
/// Save before applying, so a failed write never looks like a saved preference.
pub fn update(value: TerminalText) -> Result<(), String> {
    let path = path().ok_or("找不到配置目录")?;
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    let value = TerminalText::parse(&text)?;
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(temporary, &path)
    };
    write().map_err(|e| format!("无法保存 {}：{e}", path.display()))?;
    *state().write().unwrap() = value;
    GENERATION.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typography_round_trip_and_bounds() {
        let value = TerminalText {
            family: "Menlo".into(),
            size: 18.,
            line_height: 1.4,
            unfocused_dim: 0.3,
            unfocused_cursor: UnfocusedCursor::Hollow,
            cursor_blink: false,
            editor: Editor::Cursor,
            option_key: OptionKey::Text,
            non_ascii_family: "PingFang SC".into(),
            ligatures: true,
            cursor_shape: CursorDefault::Bar,
            cursor_guide: true,
            min_contrast: 4.5,
            faint: 0.7,
            window_dim: 0.15,
            padding: 8.,
        };
        assert_eq!(
            TerminalText::parse(&serde_json::to_string(&value).unwrap()).unwrap(),
            value
        );
        for size in [0., 37.] {
            let text = format!(r#"{{"family":"","size":{size},"line_height":1}}"#);
            assert!(TerminalText::parse(&text).is_err());
        }
        assert!(TerminalText::parse(r#"{"family":"","size":13,"line_height":0.8}"#).is_err());
        for bad in [
            r#"{"unfocused_dim":0.6}"#,
            r#"{"window_dim":0.6}"#,
            r#"{"min_contrast":0.5}"#,
            r#"{"faint":0}"#,
            r#"{"padding":41}"#,
        ] {
            assert!(TerminalText::parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn a_file_saved_before_the_split_options_keeps_its_typography() {
        let value =
            TerminalText::parse(r#"{"family":"Menlo","size":15,"line_height":1.2}"#).unwrap();
        assert_eq!(
            (value.family.as_str(), value.size, value.line_height),
            ("Menlo", 15., 1.2)
        );
        assert_eq!(value.unfocused_dim, 0.);
        assert_eq!(value.unfocused_cursor, UnfocusedCursor::Hidden);
        assert!(value.cursor_blink);
        assert_eq!(value.editor, Editor::Auto);
        assert_eq!(value.option_key, OptionKey::Auto);
        assert_eq!(value.cursor_shape, CursorDefault::Block);
        assert_eq!((value.min_contrast, value.faint), (1., 0.5));
        assert!(!value.ligatures && !value.cursor_guide);
        assert_eq!((value.window_dim, value.padding), (0., 0.));
    }
}
