//! Which theme shows: light or dark or following macOS, and one theme for each side.
//! `appearance` keeps the mode; `theme-light` and `theme-dark` each keep a theme's id.
use super::{
    palette::Palette,
    style::Style,
    theme::Theme,
    themes::{Entry, Themes},
};
use gpui::{App, Global, WindowAppearance};
use mooring_core::terminal::Scheme;
use mooring_core::theme::ThemeFile;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Follows macOS, switching when the system does.
    System,
    Light,
    /// Until one is picked: the dark workbench of docs/sidebar-redesign/flat.html.
    #[default]
    Dark,
}
impl Mode {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "跟随系统",
            Self::Light => "亮色",
            Self::Dark => "暗色",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub mode: Mode,
    pub light: String,
    pub dark: String,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            light: Scheme::Light.name().to_owned(),
            dark: Scheme::Dark.name().to_owned(),
        }
    }
}
impl Global for Appearance {}
impl Appearance {
    /// What `appearance` held: the mode, or from before themes a dark scheme's name.
    fn with_saved_mode(mut self, text: &str) -> Self {
        let text = text.trim();
        if let Some(mode) = Mode::ALL.into_iter().find(|m| m.name() == text) {
            self.mode = mode;
        } else if let Some(scheme) = Scheme::ALL.into_iter().find(|s| s.name() == text) {
            self.mode = Mode::Dark;
            self.dark = scheme.name().to_owned();
        }
        self
    }
    pub fn load() -> Self {
        let read = |name: &str| {
            let path = super::status_colors::config_dir()?.join(name);
            let text = std::fs::read_to_string(path).ok()?;
            Some(text.trim().to_owned()).filter(|t| !t.is_empty())
        };
        let mut out = read("appearance")
            .map_or_else(Self::default, |text| Self::default().with_saved_mode(&text));
        if let Some(id) = read("theme-light") {
            out.light = id;
        }
        if let Some(id) = read("theme-dark") {
            out.dark = id;
        }
        out
    }
    fn save(&self) -> Result<(), String> {
        let dir = super::status_colors::config_dir().ok_or("找不到配置目录")?;
        let write = |name: &str, text: &str| {
            std::fs::write(dir.join(name), text)
                .map_err(|e| format!("无法保存 {}：{e}", dir.join(name).display()))
        };
        std::fs::create_dir_all(&dir).map_err(|e| format!("无法保存 {}：{e}", dir.display()))?;
        write("appearance", self.mode.name())?;
        write("theme-light", &self.light)?;
        write("theme-dark", &self.dark)
    }
    /// Which side shows. The terminal-native style is dark unless light was picked
    /// outright (specs/terminal-workbench/design/main-directions.html).
    pub fn shows_light(&self, system: WindowAppearance, style: Style) -> bool {
        match (self.mode, system) {
            (Mode::Light, _) => true,
            (Mode::Dark, _) => false,
            (Mode::System, _) if style == Style::Terminal => false,
            (Mode::System, WindowAppearance::Light | WindowAppearance::VibrantLight) => true,
            (Mode::System, WindowAppearance::Dark | WindowAppearance::VibrantDark) => false,
        }
    }
    /// The theme a side uses; one that is gone, or now on the other side, gives way to
    /// that side's default.
    pub fn entry<'a>(&self, themes: &'a Themes, light: bool) -> Option<&'a Entry> {
        let id = if light { &self.light } else { &self.dark };
        themes
            .get(id)
            .filter(|entry| entry.light() == light)
            .or_else(|| {
                let fallback = if light { Scheme::Light } else { Scheme::Dark };
                themes.get(fallback.name())
            })
    }
}
/// A draft being edited: shown everywhere, saved nowhere, until 主题编辑器 says.
struct Preview(Option<Palette>);
impl Global for Preview {}
pub fn preview(file: Option<&ThemeFile>, cx: &mut App) {
    cx.set_global(Preview(file.map(Palette::of_file)));
    apply(cx);
    cx.refresh_windows();
}
/// Re-read the choice against the system appearance and repaint every window.
pub fn apply(cx: &mut App) {
    let wanted = cx.try_global::<Preview>().and_then(|p| p.0).or_else(|| {
        let chosen = cx.try_global::<Appearance>().cloned().unwrap_or_default();
        let light = chosen.shows_light(cx.window_appearance(), super::style::current(cx));
        let themes = cx.try_global::<Themes>()?;
        chosen.entry(themes, light).map(|entry| entry.palette)
    });
    if let Some(palette) = wanted
        && Theme::install(palette)
    {
        cx.refresh_windows();
    }
}
pub fn choose(appearance: Appearance, cx: &mut App) -> Result<(), String> {
    appearance.save()?;
    cx.set_global(appearance);
    apply(cx);
    cx.refresh_windows();
    Ok(())
}
pub fn set_mode(mode: Mode, cx: &mut App) -> Result<(), String> {
    let mut next = cx.try_global::<Appearance>().cloned().unwrap_or_default();
    next.mode = mode;
    choose(next, cx)
}
/// Use a theme on its side, leaving the mode alone: under 跟随系统 the other side only
/// remembers it. `force` also switches to that side.
pub fn set_theme(id: &str, light: bool, force: bool, cx: &mut App) -> Result<(), String> {
    let mut next = cx.try_global::<Appearance>().cloned().unwrap_or_default();
    if light {
        next.light = id.to_owned();
    } else {
        next.dark = id.to_owned();
    }
    if force && next.mode != Mode::System {
        next.mode = if light { Mode::Light } else { Mode::Dark };
    }
    choose(next, cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_mode(mode: Mode) -> Appearance {
        Appearance {
            mode,
            ..Appearance::default()
        }
    }
    #[test]
    fn every_older_choice_keeps_its_colors() {
        for (text, mode, dark) in [
            ("system", Mode::System, "dark"),
            ("light", Mode::Light, "dark"),
            ("dark", Mode::Dark, "dark"),
            ("slate", Mode::Dark, "slate"),
            ("graphite", Mode::Dark, "graphite"),
            ("warm", Mode::Dark, "warm"),
            ("pine", Mode::Dark, "pine"),
            ("plum\n", Mode::Dark, "plum"),
            ("sepia", Mode::Dark, "dark"),
        ] {
            let a = Appearance::default().with_saved_mode(text);
            assert_eq!((a.mode, a.dark.as_str()), (mode, dark), "{text}");
            assert_eq!(a.light, "light");
        }
    }
    #[test]
    fn system_follows_macos() {
        let standard = Style::Standard;
        let system = with_mode(Mode::System);
        assert!(system.shows_light(WindowAppearance::VibrantLight, standard));
        assert!(!system.shows_light(WindowAppearance::Dark, standard));
        assert!(!with_mode(Mode::Dark).shows_light(WindowAppearance::Light, standard));
    }
    #[test]
    fn terminal_style_is_dark_unless_light_was_picked() {
        let terminal = Style::Terminal;
        assert!(!with_mode(Mode::System).shows_light(WindowAppearance::Light, terminal));
        assert!(with_mode(Mode::Light).shows_light(WindowAppearance::Dark, terminal));
    }
    #[test]
    fn a_theme_that_is_gone_gives_way_to_its_sides_default() {
        let themes = Themes::read(None);
        let a = Appearance {
            mode: Mode::System,
            light: "file:gone".into(),
            dark: "slate".into(),
        };
        assert_eq!(a.entry(&themes, true).unwrap().id, "light");
        assert_eq!(a.entry(&themes, false).unwrap().id, "slate");
        // A dark theme named on the light side, as a hand edit could.
        let wrong = Appearance {
            light: "slate".into(),
            ..a
        };
        assert_eq!(wrong.entry(&themes, true).unwrap().id, "light");
    }
}
