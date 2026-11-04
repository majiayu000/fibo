//! The colors a terminal paints with: the built-in schemes and any theme's.

/// Default colors of every terminal. The display paints unset cells with them, and
/// programs read the same values back through OSC 10/11/12 (Codex tints its input
/// box from the background, so an unanswered query leaves it flat).
pub const DEFAULT_FG: u32 = 0xd4d4d8;
pub const DEFAULT_BG: u32 = 0x0e0e11;
/// The light scheme's defaults, the window's light theme being derived from them.
pub const LIGHT_FG: u32 = 0x1f1e1c;
/// White as in the v3 design: panes and the chrome around them are white, the sidebar
/// alone a step gray.
pub const LIGHT_BG: u32 = 0xffffff;
/// ANSI 0-15 for the light scheme: Ghostty's built-in palette is tuned for a dark
/// background, where its white and yellow would vanish on this one. The grays (0, 7,
/// 8, 15) lean warm like the background so the chrome and terminal share one neutral.
const LIGHT_ANSI: [u32; 16] = [
    0x2a2825, 0xcf222e, 0x116329, 0x8a5a00, 0x0969da, 0x8250df, 0x1b7c83, 0x6f6a64, 0x57534e,
    0xa40e26, 0x1a7f37, 0x9a6700, 0x218bff, 0xa475f9, 0x3192aa, 0x8c8680,
];
/// GitHub's Ghostty themes: a blue that reads as the cursor on either background.
const LIGHT_CURSOR: u32 = 0x0969da;
const DARK_CURSOR: u32 = 0x2f81f7;
/// Bit 24 of an ANSI slot: the theme sets this color; clear keeps the built-in one.
const SET: u32 = 1 << 24;

/// A built-in scheme, by the name programs and older hosts know it by.
///
/// Past dark and light, each scheme is a lifted dark: a ground a few steps above black
/// under slightly softened text (about 10:1), for displays where near-black under
/// neutral white reads as glare. The window's colors all follow from `fg` and `bg`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Scheme {
    #[default]
    Dark,
    Light,
    /// Blue-gray.
    Slate,
    /// Neutral gray.
    Graphite,
    /// Warm gray, like dark paper.
    Warm,
    /// Green-gray.
    Pine,
    /// Violet-gray.
    Plum,
}
impl Scheme {
    pub const ALL: [Self; 7] = [
        Self::Dark,
        Self::Light,
        Self::Slate,
        Self::Graphite,
        Self::Warm,
        Self::Pine,
        Self::Plum,
    ];
    pub const fn is_light(self) -> bool {
        matches!(self, Self::Light)
    }
    /// The id a choice of this theme is saved under.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Slate => "slate",
            Self::Graphite => "graphite",
            Self::Warm => "warm",
            Self::Pine => "pine",
            Self::Plum => "plum",
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dark => "曜黑",
            Self::Light => "纸白",
            Self::Slate => "蓝灰",
            Self::Graphite => "石墨",
            Self::Warm => "暖灰",
            Self::Pine => "松绿",
            Self::Plum => "紫灰",
        }
    }
    /// (foreground, background).
    const fn pair(self) -> (u32, u32) {
        match self {
            Self::Dark => (DEFAULT_FG, DEFAULT_BG),
            Self::Light => (LIGHT_FG, LIGHT_BG),
            Self::Slate => (0xcad3e1, 0x21252c),
            Self::Graphite => (0xd2d2d6, 0x232326),
            Self::Warm => (0xddd5c8, 0x262320),
            Self::Pine => (0xcbd9d2, 0x1d2624),
            Self::Plum => (0xd6cfe2, 0x25222c),
        }
    }
    pub const fn fg(self) -> u32 {
        self.pair().0
    }
    pub const fn bg(self) -> u32 {
        self.pair().1
    }
    pub const fn cursor(self) -> u32 {
        if self.is_light() {
            LIGHT_CURSOR
        } else {
            DARK_CURSOR
        }
    }
    pub const fn colors(self) -> TermColors {
        let mut ansi = [0; 16];
        if self.is_light() {
            let mut i = 0;
            while i < 16 {
                ansi[i] = LIGHT_ANSI[i] | SET;
                i += 1;
            }
        }
        TermColors {
            fg: self.fg(),
            bg: self.bg(),
            cursor: self.cursor(),
            light: self.is_light(),
            ansi,
        }
    }
    /// The built-in these colors are exactly, if any: a host built before themes
    /// only understands these.
    pub fn of(colors: &TermColors) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.colors() == *colors)
    }
}

/// What a terminal is told to paint with. Small and `Copy`: every pane compares it each
/// frame against what it last applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TermColors {
    pub fg: u32,
    pub bg: u32,
    pub cursor: u32,
    pub light: bool,
    /// ANSI 0-15; a slot without the set bit keeps the terminal's built-in color.
    ansi: [u32; 16],
}
impl Default for TermColors {
    fn default() -> Self {
        Scheme::Dark.colors()
    }
}
impl TermColors {
    /// Colors from a theme: whether it is light comes from its background, so a file
    /// cannot say one thing and look another.
    pub fn new(fg: u32, bg: u32, cursor: Option<u32>, ansi: [Option<u32>; 16]) -> Self {
        let light = is_light(bg);
        let base = if light { Scheme::Light } else { Scheme::Dark }.colors();
        let mut slots = base.ansi;
        for (slot, color) in slots.iter_mut().zip(ansi) {
            if let Some(color) = color {
                *slot = color | SET;
            }
        }
        Self {
            fg,
            bg,
            cursor: cursor.unwrap_or(base.cursor),
            light,
            ansi: slots,
        }
    }
    pub fn ansi(&self) -> [Option<u32>; 16] {
        self.ansi
            .map(|slot| (slot & SET != 0).then_some(slot & 0xff_ffff))
    }
    /// The nearest built-in, for what only understands those.
    pub fn base(&self) -> Scheme {
        Scheme::of(self).unwrap_or(if self.light {
            Scheme::Light
        } else {
            Scheme::Dark
        })
    }
    /// `COLORFGBG` for a program started under these colors ("fg;bg" in ANSI numbers),
    /// which some read before they query the terminal.
    pub const fn colorfgbg(&self) -> &'static str {
        if self.light { "0;15" } else { "15;0" }
    }
}

/// WCAG relative luminance.
/// WCAG contrast of two colors, 1 to 21.
pub fn contrast(a: u32, b: u32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
pub fn luminance(c: u32) -> f32 {
    let channel = |shift: u32| {
        let v = ((c >> shift) & 255) as f32 / 255.;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}
/// A background this bright is a light theme: black text out-contrasts white on it.
pub fn is_light(bg: u32) -> bool {
    luminance(bg) > 0.179
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_builtin_is_recognized_from_its_colors() {
        for scheme in Scheme::ALL {
            assert_eq!(Scheme::of(&scheme.colors()), Some(scheme));
            assert_eq!(scheme.colors().base(), scheme);
            assert_eq!(is_light(scheme.bg()), scheme.is_light(), "{scheme:?}");
        }
    }
    #[test]
    fn a_theme_sets_only_the_ansi_it_names() {
        let mut ansi = [None; 16];
        ansi[1] = Some(0x112233);
        let dark = TermColors::new(0xffffff, 0x101010, None, ansi);
        assert_eq!(dark.ansi()[1], Some(0x112233));
        assert_eq!(dark.ansi()[2], None);
        assert_eq!(dark.cursor, Scheme::Dark.cursor());
        assert_eq!(Scheme::of(&dark), None);
        assert_eq!(dark.base(), Scheme::Dark);
        // A light ground keeps the light palette for what it leaves out.
        let light = TermColors::new(0x000000, 0xf0f0f0, Some(0x123456), ansi);
        assert!(light.light);
        assert_eq!(light.ansi()[1], Some(0x112233));
        assert_eq!(light.ansi()[2], Scheme::Light.colors().ansi()[2]);
        assert_eq!(light.cursor, 0x123456);
    }
    #[test]
    fn the_wire_form_of_a_builtin_is_what_older_hosts_read() {
        assert_eq!(serde_json::to_string(&Scheme::Slate).unwrap(), "\"Slate\"");
    }
}
