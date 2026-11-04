//! The window's colors for one theme: worked out once, when the theme changes, so that
//! drawing only ever reads a finished value.
use super::theme::{Status, mix};
use mooring_core::{
    terminal::{Scheme, TermColors},
    theme::ThemeFile,
};

const BLACK: u32 = 0x000000;
const WHITE: u32 = 0xffffff;
/// Chrome colors for one terminal scheme. Every surface and text tone is a mix of the
/// terminal's default colors (plus black or white), so the window reads as one surface
/// with the terminal in either scheme. A theme may then replace any of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// What terminals are told to paint with.
    pub term: TermColors,
    /// The 会话优先 column's own colors (`Flat`).
    pub sidebar_bg: u32,
    pub sidebar_line: u32,
    pub sidebar_field_line: u32,
    pub sidebar_chip_line: u32,
    pub sidebar_chip_on: u32,
    pub sidebar_chip_on_line: u32,
    pub sidebar_hover: u32,
    pub sidebar_selected: u32,
    pub sidebar_selected_line: u32,
    pub sidebar_bright: u32,
    pub sidebar_title: u32,
    pub sidebar_quiet: u32,
    pub sidebar_second: u32,
    pub sidebar_third: u32,
    pub sidebar_done: u32,
    pub sidebar_idle: u32,
    pub sidebar_scope: u32,
    pub sidebar_key_line: u32,
    /// A theme's colors for what the terminal paints past its defaults.
    pub term_cursor_text: Option<u32>,
    pub term_selection: Option<u32>,
    pub term_bold: Option<u32>,
    pub term_link: Option<u32>,
    pub term_underline: Option<u32>,
    pub term_selected_text: Option<u32>,
    pub term_cursor_guide: Option<u32>,
    /// A theme's status and tone hues, when `colors.json` does not set them.
    pub status: [Option<u32>; Status::ALL.len()],
    pub tones: [Option<u32>; 3],
    pub space_tiles: [[u32; 2]; 6],

    pub terminal_bg: u32,
    pub terminal_fg: u32,
    /// White on dark, black on light: past the text, for ink and the one solid control.
    pub(super) vivid: u32,
    pub(super) ink_share: u32,
    pub(super) wash_share: u32,
    /// Top bar, panes and headers share the terminal's background and are told apart by
    /// hairlines; the sidebar alone sits one gray step below, as the v3 design has it.
    pub app: u32,
    pub sidebar: u32,
    pub panel: u32,
    pub pane: u32,
    pub header: u32,
    pub todo: u32,
    pub raised: u32,
    pub border: u32,
    pub card_border: u32,
    pub control: u32,
    /// Text field outlines: 3:1 against the pane so an empty field is still findable.
    pub field_edge: u32,
    /// A quiet fill for ordinary buttons; they carry no outline.
    pub button: u32,
    pub button_hover: u32,
    pub primary: u32,
    pub body: u32,
    pub secondary: u32,
    pub muted: u32,
    /// The one solid control per view: near-white with dark text on dark, the reverse
    /// on light; color is left to status.
    pub accent: u32,
    pub on_accent: u32,
    pub focus: u32,
    pub link: u32,
    /// Selected text in a field.
    pub selection: u32,
    pub chip: u32,
    /// In-window popovers (「你在这里」, cleanup): one step above the pane on dark; on
    /// light the pane is already white, so they are white too and float on their shadow.
    pub popover: u32,
    pub palette_row: u32,
    /// Matched text in search results: a quiet amber wash, the text keeps its own color.
    pub matched: u32,
    /// Dims the workbench behind the palette, as rgba.
    pub scrim: u32,
    /// Data charts only (the diagnostics panel): app identity, never status. Validated as
    /// adjacent pairs for color-vision deficiency on the popover in each scheme.
    pub series: [u32; 5],
    /// Everything ranked past the colored apps.
    pub series_other: u32,
    /// Busy time no app accounts for, drawn as a hatch.
    pub hatch: u32,
    pub chart_grid: u32,
    /// A meaningful threshold on an axis, such as one full core.
    pub chart_ref: u32,
}
impl Palette {
    /// Light shares are set against the contrast floors (text 4.5:1, focus and field edges
    /// 3:1) and a surface ladder whose steps stay visible; dark keeps its tuned values.
    /// `tuned`: the near-black scheme, whose sidebar grays are the design's own.
    pub const fn new(term: TermColors, tuned: bool) -> Self {
        let (bg, fg, light) = (term.bg, term.fg, term.light);
        let vivid = if light { BLACK } else { WHITE };
        let primary = mix(fg, vivid, 300);
        let border = mix(bg, fg, by(light, 170, 90));
        let control = mix(bg, fg, by(light, 300, 150));
        // Popovers sit one step above the pane: on light that is white, like the pane.
        let popover = if light {
            mix(bg, WHITE, 700)
        } else {
            mix(bg, fg, 55)
        };
        let mut out = Self {
            term,
            sidebar_bg: 0,
            sidebar_line: 0,
            sidebar_field_line: 0,
            sidebar_chip_line: 0,
            sidebar_chip_on: 0,
            sidebar_chip_on_line: 0,
            sidebar_hover: 0,
            sidebar_selected: 0,
            sidebar_selected_line: 0,
            sidebar_bright: 0,
            sidebar_title: 0,
            sidebar_quiet: 0,
            sidebar_second: 0,
            sidebar_third: 0,
            sidebar_done: 0,
            sidebar_idle: 0,
            sidebar_scope: 0,
            sidebar_key_line: 0,
            term_cursor_text: None,
            term_selection: None,
            term_bold: None,
            term_link: None,
            term_underline: None,
            term_selected_text: None,
            term_cursor_guide: None,
            status: [None; Status::ALL.len()],
            tones: [None; 3],
            space_tiles: SPACE_TILES,
            terminal_bg: bg,
            terminal_fg: fg,
            vivid,
            ink_share: if light { 450 } else { 620 },
            wash_share: if light { 150 } else { 220 },
            // On dark the window around the panes is near black (#08080A), as flat.html has it.
            app: mix(bg, BLACK, by(light, 70, 430)),
            panel: bg,
            // On dark the column sits a step below the terminal's ground, near black
            // (docs/sidebar-redesign/flat.html), so the panes read as the lit surface.
            sidebar: if light {
                mix(bg, fg, 45)
            } else if !tuned {
                // A lifted ground's column is a softer step down, so the ground still reads.
                mix(bg, BLACK, 330)
            } else {
                mix(bg, BLACK, 450)
            },
            pane: bg,
            header: bg,
            todo: mix(bg, fg, by(light, 45, 30)),
            raised: mix(bg, fg, by(light, 105, 70)),
            border,
            card_border: mix(bg, fg, by(light, 260, 100)),
            control,
            field_edge: mix(bg, fg, by(light, 490, 150)),
            button: mix(bg, fg, by(light, 90, 60)),
            button_hover: mix(bg, fg, by(light, 180, 120)),
            primary,
            body: fg,
            secondary: mix(bg, fg, by(light, 720, 580)),
            muted: mix(bg, fg, by(light, 640, 420)),
            accent: primary,
            on_accent: bg,
            focus: mix(bg, fg, by(light, 760, 380)),
            link: mix(bg, fg, 860),
            selection: mix(bg, fg, by(light, 180, 260)),
            chip: mix(bg, fg, by(light, 120, 80)),
            popover,
            palette_row: mix(popover, fg, by(light, 110, 75)),
            matched: mix(popover, 0xe8a33a, by(light, 350, 280)),
            scrim: if light { 0x0000_0040 } else { 0x0000_0099 },
            series: if light {
                [0x2a78d6, 0xeb6834, 0x1baf7a, 0xeda100, 0xe87ba4]
            } else {
                [0x3987e5, 0xd95926, 0x199e70, 0xc98500, 0xd55181]
            },
            series_other: mix(bg, fg, by(light, 300, 380)),
            hatch: mix(bg, fg, by(light, 260, 240)),
            chart_grid: mix(popover, fg, by(light, 90, 70)),
            chart_ref: mix(bg, fg, by(light, 450, 420)),
        };
        out.sidebar_bg = if tuned { 0x0c0c0f } else { out.sidebar };
        out.sidebar_line = ink(0x1c1c21, out.border, light, tuned, bg, fg);
        out.sidebar_field_line = ink(0x1f1f25, out.border, light, tuned, bg, fg);
        out.sidebar_chip_line = ink(0x23232a, out.border, light, tuned, bg, fg);
        out.sidebar_chip_on = ink(0x1c1c22, out.raised, light, tuned, bg, fg);
        out.sidebar_chip_on_line = ink(0x4a4a54, out.control, light, tuned, bg, fg);
        out.sidebar_hover = ink(0x131317, out.raised, light, tuned, bg, fg);
        out.sidebar_selected = ink(0x17171c, out.raised, light, tuned, bg, fg);
        out.sidebar_selected_line = ink(0x2a2a31, out.control, light, tuned, bg, fg);
        out.sidebar_bright = ink(0xececee, out.primary, light, tuned, bg, fg);
        out.sidebar_title = ink(0xe4e4e7, out.primary, light, tuned, bg, fg);
        out.sidebar_quiet = ink(0xc9c9cf, out.body, light, tuned, bg, fg);
        out.sidebar_second = ink(0xa2a2aa, out.secondary, light, tuned, bg, fg);
        out.sidebar_third = ink(0x7e7e87, out.muted, light, tuned, bg, fg);
        out.sidebar_done = ink(0x6a6a72, out.muted, light, tuned, bg, fg);
        out.sidebar_idle = ink(0x45454d, out.control, light, tuned, bg, fg);
        out.sidebar_scope = ink(0x121216, out.raised, light, tuned, bg, fg);
        out.sidebar_key_line = ink(0x2a2a31, out.control, light, tuned, bg, fg);
        out
    }
    pub const fn of_scheme(scheme: Scheme) -> Self {
        Self::new(scheme.colors(), matches!(scheme, Scheme::Dark))
    }
    /// A theme file over what its `bg` and `fg` derive. `explicit` says which hues
    /// `colors.json` already sets: that file stays on top of any theme.
    pub fn of_file(file: &ThemeFile) -> Self {
        let term = file.term_colors();
        let mut out = Self::new(term, Scheme::of(&term) == Some(Scheme::Dark));
        if let Some(keys) = file.colors.get("ui") {
            for (key, value) in keys {
                out.set_ui(key, *value);
            }
        }
        let terminal = |key| file.color("terminal", key);
        out.term_cursor_text = terminal("cursor_text");
        out.term_selection = terminal("selection");
        out.term_bold = terminal("bold");
        out.term_link = terminal("link");
        out.term_underline = terminal("underline");
        out.term_selected_text = terminal("selected_text");
        out.term_cursor_guide = terminal("cursor_guide");
        for (i, status) in Status::ALL.iter().enumerate() {
            out.status[i] = file
                .color("status", status.key())
                .filter(|_| !super::status_colors::StatusColors::get().sets_status(*status));
        }
        for tone in super::tone::Tone::ALL {
            out.tones[tone as usize] = file
                .color("tone", tone.key())
                .filter(|_| !super::status_colors::StatusColors::get().sets_tone(tone));
        }
        if !file.space_gradients.is_empty() {
            for (i, slot) in out.space_tiles.iter_mut().enumerate() {
                *slot = file.space_gradients[i % file.space_gradients.len()];
            }
        }
        out
    }
    /// Names in a file's `ui`, `status` and `tone` no window color answers to.
    pub fn unknown_keys(file: &ThemeFile) -> Vec<String> {
        let mut out = vec![];
        let mut check = |section: &str, known: &dyn Fn(&str) -> bool| {
            for key in file.colors.get(section).into_iter().flat_map(|k| k.keys()) {
                if !known(key) {
                    out.push(format!("不认识 {section}.{key}，已跳过"));
                }
            }
        };
        check("ui", &|key| UI_TOKENS.iter().any(|(name, _)| *name == key));
        check("status", &|key| Status::ALL.iter().any(|s| s.key() == key));
        check("tone", &|key| {
            super::tone::Tone::ALL.iter().any(|t| t.key() == key)
        });
        out
    }
}
/// `dark` on the near-black design, carried onto any other dark ground, and `fallback`
/// on a light one.
const fn ink(dark: u32, fallback: u32, light: bool, tuned: bool, bg: u32, fg: u32) -> u32 {
    if light {
        fallback
    } else if tuned {
        dark
    } else {
        restate(dark, bg, fg)
    }
}
/// The sidebar's own dark grays are tuned against the dark terminal; on another dark
/// ground each keeps its place on the ladder below, between or above that terminal's
/// colors, taken onto the ground's.
pub(super) const fn restate(dark: u32, to_bg: u32, to_fg: u32) -> u32 {
    let (bg, fg) = (Scheme::Dark.bg(), Scheme::Dark.fg());
    let (v, b, f) = (gray(dark), gray(bg), gray(fg));
    if v <= b {
        mix(to_bg, BLACK, (b - v) * 1000 / b)
    } else if v <= f {
        mix(to_bg, to_fg, (v - b) * 1000 / (f - b))
    } else {
        mix(to_fg, WHITE, (v - f) * 1000 / (255 - f))
    }
}
pub(super) const fn gray(c: u32) -> u32 {
    (((c >> 16) & 255) + ((c >> 8) & 255) + (c & 255)) / 3
}
const fn by(light: bool, on_light: u32, on_dark: u32) -> u32 {
    if light { on_light } else { on_dark }
}
/// The space tiles' gradients, first color to second.
pub const SPACE_TILES: [[u32; 2]; 6] = [
    [0xf59e6b, 0xd9467a],
    [0x5ab0f0, 0x4b4fd6],
    [0x7bd389, 0x2f9e66],
    [0xc04f9c, 0x8a5ad8],
    [0x3d62c9, 0x7b8cf0],
    [0x8a6a2a, 0xb38a3a],
];
/// Every window color a theme can replace: the file key, its field, and what 主题编辑器
/// calls it. The keys are a public contract: rename one and old theme files lose it.
macro_rules! ui_tokens {
    ($($field:ident $label:literal),* $(,)?) => {
        pub const UI_TOKENS: &[(&str, &str)] = &[$((stringify!($field), $label)),*];
        impl Palette {
            pub fn ui(&self, key: &str) -> Option<u32> {
                match key {
                    $(stringify!($field) => Some(self.$field),)*
                    _ => None,
                }
            }
            /// `sidebar` paints two surfaces: the column and its 会话优先 form.
            fn set_ui(&mut self, key: &str, value: u32) {
                if key == "sidebar" {
                    self.sidebar_bg = value;
                }
                match key {
                    $(stringify!($field) => self.$field = value,)*
                    _ => {}
                }
            }
        }
    };
}
ui_tokens!(
    app "窗口外框", sidebar "侧栏", panel "面板", pane "窗格", header "窗格标题",
    todo "待办底色", raised "凸起面", border "分隔线", card_border "卡片边线",
    control "控件边线", field_edge "输入框边线", button "按钮", button_hover "按钮悬停",
    primary "主要文字", body "正文", secondary "次要文字", muted "弱文字",
    accent "强调色", on_accent "强调色上的文字", focus "焦点", link "界面链接",
    selection "界面选区", chip "标签底色", popover "浮层", palette_row "命令面板选中行",
    matched "搜索命中", series_other "图表其他", hatch "图表阴影线", chart_grid "图表网格",
    chart_ref "图表参考线",
    sidebar_line "侧栏分隔线", sidebar_field_line "侧栏搜索框边线",
    sidebar_chip_line "空间标签边线", sidebar_chip_on "选中空间底色",
    sidebar_chip_on_line "选中空间边线", sidebar_hover "会话行悬停",
    sidebar_selected "选中会话底色", sidebar_selected_line "选中会话边线",
    sidebar_bright "品牌和选中文字", sidebar_title "会话标题", sidebar_quiet "停着的标题",
    sidebar_second "侧栏次要文字", sidebar_third "侧栏弱文字", sidebar_done "做完的 ✓",
    sidebar_idle "停着的圆圈", sidebar_scope "空间筛选条", sidebar_key_line "按键提示边线",
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_replaces_only_what_it_names() {
        let mut file = ThemeFile {
            bg: 0x101820,
            fg: 0xd0d8e0,
            ..ThemeFile::default()
        };
        let plain = Palette::of_file(&file);
        assert_eq!(plain.border, Palette::new(file.term_colors(), false).border);
        file.set("ui", "border", Some(0x123456));
        file.set("ui", "sidebar", Some(0x654321));
        file.set("terminal", "bold", Some(0xabcdef));
        let over = Palette::of_file(&file);
        assert_eq!(over.border, 0x123456);
        assert_eq!((over.sidebar, over.sidebar_bg), (0x654321, 0x654321));
        assert_eq!(over.term_bold, Some(0xabcdef));
        assert_eq!(over.muted, plain.muted);
        assert_eq!(plain.term_bold, None);
    }
    #[test]
    fn a_builtin_written_as_a_file_looks_the_same() {
        for scheme in Scheme::ALL {
            let file = ThemeFile {
                bg: scheme.bg(),
                fg: scheme.fg(),
                ..ThemeFile::default()
            };
            let (made, built) = (Palette::of_file(&file), Palette::of_scheme(scheme));
            assert_eq!(made, built, "{scheme:?}");
        }
    }
    #[test]
    fn unknown_names_are_reported() {
        let mut file = ThemeFile {
            bg: 0,
            fg: 0xffffff,
            ..ThemeFile::default()
        };
        file.set("ui", "sidebar", Some(1));
        file.set("ui", "wobble", Some(1));
        file.set("status", "running", Some(1));
        file.set("tone", "good", Some(1));
        assert_eq!(Palette::unknown_keys(&file).len(), 2);
    }
    #[test]
    fn every_token_can_be_read_and_replaced() {
        for (key, label) in UI_TOKENS {
            assert!(!label.is_empty());
            let mut file = ThemeFile {
                bg: 0x101820,
                fg: 0xd0d8e0,
                ..ThemeFile::default()
            };
            file.set("ui", key, Some(0x010203));
            assert_eq!(Palette::of_file(&file).ui(key), Some(0x010203), "{key}");
        }
    }
}
