//! A user's theme: one file in `~/.config/mooring/themes/`. Only `bg` and `fg` are
//! needed; everything else follows from them unless the file names it. Names unknown
//! to this build are reported and skipped, so a theme written for a newer one still
//! loads here.
use crate::scheme::TermColors;
use std::collections::BTreeMap;

pub const ANSI_NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright_black",
    "bright_red",
    "bright_green",
    "bright_yellow",
    "bright_blue",
    "bright_magenta",
    "bright_cyan",
    "bright_white",
];
/// Terminal colors past the defaults, painted by the window.
pub const TERMINAL_NAMES: [&str; 8] = [
    "cursor",
    "cursor_text",
    "selection",
    "selected_text",
    "bold",
    "link",
    "underline",
    "cursor_guide",
];
const SECTIONS: [&str; 6] = ["terminal", "ansi", "ui", "status", "tone", "seen"];
/// The file format this build writes. Written into every theme, so a later Fibo can
/// tell an old file; a newer one still loads here, what is unknown being skipped.
pub const FORMAT: u64 = 1;
/// Meaning colors for everything that is not a session.
pub const TONE_NAMES: [&str; 3] = ["ok", "warn", "danger"];
/// Colors of a session row you have looked at; `dot` sets how its status dot is drawn.
pub const SEEN_NAMES: [&str; 2] = ["title", "status"];
/// Every window color a theme can replace, and what 主题编辑器 calls it, handed to `$then`:
/// the window turns this list into its palette's fields, and core checks files against it
/// without knowing those. The names are a public contract: rename one and old files lose it.
#[macro_export]
macro_rules! with_ui_tokens {
    ($then:ident) => {
        $then!(
            app "", sidebar "", panel "", pane "", header "",
            todo "", raised "", border "", card_border "",
            control "", field_edge "", button "", button_hover "",
            primary "", body "", secondary "", muted "",
            accent "", on_accent "", focus "", link "",
            selection "", chip "", popover "", palette_row "",
            matched "", series_other "", hatch "", chart_grid "",
            chart_ref "",
            sidebar_line "", sidebar_field_line "",
            sidebar_chip_line "", sidebar_chip_on "",
            sidebar_chip_on_line "", sidebar_hover "",
            sidebar_selected "", sidebar_selected_line "",
            sidebar_bright "", sidebar_title "", sidebar_quiet "",
            sidebar_second "", sidebar_third "", sidebar_done "",
            sidebar_idle "", sidebar_key_line "",
        );
    };
}
/// `ui` keys, with what 主题编辑器 calls each (say with `i18n::say` where shown).
pub const UI_NAMES: &[(&str, &str)] = &[
    ("app", tk!("窗口外框")),
    ("sidebar", tk!("侧栏")),
    ("panel", tk!("面板")),
    ("pane", tk!("窗格")),
    ("header", tk!("窗格标题")),
    ("todo", tk!("待办底色")),
    ("raised", tk!("凸起面")),
    ("border", tk!("分隔线")),
    ("card_border", tk!("卡片边线")),
    ("control", tk!("控件边线")),
    ("field_edge", tk!("输入框边线")),
    ("button", tk!("按钮")),
    ("button_hover", tk!("按钮悬停")),
    ("primary", tk!("主要文字")),
    ("body", tk!("正文")),
    ("secondary", tk!("次要文字")),
    ("muted", tk!("弱文字")),
    ("accent", tk!("强调色")),
    ("on_accent", tk!("强调色上的文字")),
    ("focus", tk!("焦点")),
    ("link", tk!("界面链接")),
    ("selection", tk!("界面选区")),
    ("chip", tk!("标签底色")),
    ("popover", tk!("浮层")),
    ("palette_row", tk!("命令面板选中行")),
    ("matched", tk!("搜索命中")),
    ("series_other", tk!("图表其他")),
    ("hatch", tk!("图表阴影线")),
    ("chart_grid", tk!("图表网格")),
    ("chart_ref", tk!("图表参考线")),
    ("sidebar_line", tk!("侧栏分隔线")),
    ("sidebar_field_line", tk!("侧栏搜索框边线")),
    ("sidebar_chip_line", tk!("空间标签边线")),
    ("sidebar_chip_on", tk!("选中空间底色")),
    ("sidebar_chip_on_line", tk!("选中空间边线")),
    ("sidebar_hover", tk!("会话行悬停")),
    ("sidebar_selected", tk!("选中会话底色")),
    ("sidebar_selected_line", tk!("选中会话边线")),
    ("sidebar_bright", tk!("品牌和选中文字")),
    ("sidebar_title", tk!("会话标题")),
    ("sidebar_quiet", tk!("停着的标题")),
    ("sidebar_second", tk!("侧栏次要文字")),
    ("sidebar_third", tk!("侧栏弱文字")),
    ("sidebar_done", tk!("做完的 ✓")),
    ("sidebar_idle", tk!("停着的圆圈")),
    ("sidebar_key_line", tk!("按键提示边线")),
];
/// How the dot of a session you have looked at is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeenDot {
    /// An outline in the status hue: still says what it is doing, without shouting.
    #[default]
    Ring,
    Fill,
    /// The filled dot washed toward the panel.
    Fade,
}
impl SeenDot {
    pub const ALL: [Self; 3] = [Self::Ring, Self::Fill, Self::Fade];
    pub fn name(self) -> &'static str {
        match self {
            Self::Ring => "ring",
            Self::Fill => "fill",
            Self::Fade => "fade",
        }
    }
}

/// How a picture fills a terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageMode {
    /// Covers the pane, cropped to its shape.
    #[default]
    Fill,
    /// All of it shows, with the terminal's ground beside it.
    Fit,
    Stretch,
    /// Repeated at its own size.
    Tile,
}
impl ImageMode {
    pub const ALL: [Self; 4] = [Self::Fill, Self::Fit, Self::Stretch, Self::Tile];
    pub fn name(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Fit => "fit",
            Self::Stretch => "stretch",
            Self::Tile => "tile",
        }
    }
}
/// A picture behind a theme's terminals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundImage {
    /// Absolute, or relative to the themes folder.
    pub path: String,
    pub mode: ImageMode,
    /// How much of it shows through the terminal's ground, in percent.
    pub strength: u8,
}
pub const DEFAULT_IMAGE_STRENGTH: u8 = 20;
/// The window's material: what a theme sets past its colors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Material {
    /// How much of the window's ground is painted, in percent; 100 hides what is behind.
    pub opacity: u8,
    /// What shows behind a window that is not fully opaque is blurred.
    pub blur: bool,
    pub image: Option<BackgroundImage>,
}
impl Default for Material {
    fn default() -> Self {
        Self {
            opacity: 100,
            blur: false,
            image: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeFile {
    pub name: String,
    pub bg: u32,
    pub fg: u32,
    /// Section → key → color.
    pub colors: BTreeMap<String, BTreeMap<String, u32>>,
    pub space_gradients: Vec<[u32; 2]>,
    pub material: Material,
    pub seen_dot: Option<SeenDot>,
    /// What was skipped while reading, for 设置 to show.
    pub problems: Vec<String>,
}
impl ThemeFile {
    pub fn color(&self, section: &str, key: &str) -> Option<u32> {
        self.colors.get(section)?.get(key).copied()
    }
    pub fn set(&mut self, section: &str, key: &str, value: Option<u32>) {
        match value {
            Some(value) => {
                self.colors
                    .entry(section.to_owned())
                    .or_default()
                    .insert(key.to_owned(), value);
            }
            None => {
                if let Some(keys) = self.colors.get_mut(section) {
                    keys.remove(key);
                    if keys.is_empty() {
                        self.colors.remove(section);
                    }
                }
            }
        }
    }
    pub fn term_colors(&self) -> TermColors {
        let mut ansi = [None; 16];
        for (slot, name) in ansi.iter_mut().zip(ANSI_NAMES) {
            *slot = self.color("ansi", name);
        }
        TermColors::new(self.fg, self.bg, self.color("terminal", "cursor"), ansi)
    }
    /// `fallback_name` is the file's stem, used when it names no theme.
    pub fn parse(text: &str, fallback_name: &str) -> Result<Self, String> {
        let root: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let root = root.as_object().ok_or(t!("主题文件应是一个 JSON 对象"))?;
        let required = |key: &str| {
            root.get(key)
                .ok_or_else(|| t!("缺少 {key}", key))
                .and_then(parse_color)
                .map_err(|e| t!("{key}：{e}", key, e))
        };
        let mut out = Self {
            name: root
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|n| !n.trim().is_empty())
                .unwrap_or(fallback_name)
                .to_owned(),
            bg: required("bg")?,
            fg: required("fg")?,
            ..Self::default()
        };
        for (key, value) in root {
            match key.as_str() {
                "name" | "bg" | "fg" => {}
                "format" => match value.as_u64() {
                    Some(format) if format <= FORMAT => {}
                    Some(format) => out.problems.push(t!(
                        "这个主题是为更新的 Fibo 写的（格式 {format}），不认识的部分已跳过",
                        format
                    )),
                    None => out
                        .problems
                        .push(t!("format 应是一个数字，写的是 {value}，已跳过", value)),
                },
                "space_gradients" => out.read_gradients(value),
                "window" => out.read_window(value),
                "background" => out.read_background(value),
                section if SECTIONS.contains(&section) => out.read_section(section, value),
                other => out.problems.push(t!("不认识 {other}，已跳过", other)),
            }
        }
        Ok(out)
    }
    fn read_section(&mut self, section: &str, value: &serde_json::Value) {
        let Some(keys) = value.as_object() else {
            self.problems
                .push(t!("{section} 应是一个对象，已跳过", section));
            return;
        };
        for (key, value) in keys {
            if section == "seen" && key == "dot" {
                let named = SeenDot::ALL
                    .into_iter()
                    .find(|dot| value.as_str() == Some(dot.name()));
                match named {
                    Some(dot) => self.seen_dot = Some(dot),
                    None => self.problems.push(t!(
                        "seen.dot 应是 ring、fill 或 fade，写的是 {value}，已跳过",
                        value
                    )),
                }
                continue;
            }
            let known = known_key(section, key);
            if !known {
                self.problems
                    .push(t!("不认识 {section}.{key}，已跳过", section, key));
                continue;
            }
            match parse_color(value) {
                Ok(color) => self.set(section, key, Some(color)),
                Err(e) => self
                    .problems
                    .push(t!("{section}.{key}：{e}，已跳过", section, key, e)),
            }
        }
    }
    fn read_gradients(&mut self, value: &serde_json::Value) {
        let pair = |v: &serde_json::Value| -> Option<[u32; 2]> {
            let [a, b] = v.as_array()?.as_slice() else {
                return None;
            };
            Some([parse_color(a).ok()?, parse_color(b).ok()?])
        };
        match value.as_array().map(|l| l.iter().map(pair).collect()) {
            Some(Some(pairs)) => self.space_gradients = pairs,
            _ => self
                .problems
                .push(t!("space_gradients 应是若干对颜色，已跳过").to_owned()),
        }
    }
    fn read_window(&mut self, value: &serde_json::Value) {
        let Some(keys) = value.as_object() else {
            self.problems
                .push(t!("window 应是一个对象，已跳过").to_owned());
            return;
        };
        for (key, value) in keys {
            match (key.as_str(), value) {
                ("opacity", value) => match value.as_f64().filter(|v| (0.3..=1.).contains(v)) {
                    Some(v) => self.material.opacity = (v * 100.).round() as u8,
                    None => self.problems.push(t!(
                        "window.opacity 应在 0.3–1，写的是 {value}，已跳过",
                        value
                    )),
                },
                ("blur", serde_json::Value::Bool(blur)) => self.material.blur = *blur,
                ("blur", value) => self.problems.push(t!(
                    "window.blur 应是 true 或 false，写的是 {value}，已跳过",
                    value
                )),
                (other, _) => self
                    .problems
                    .push(t!("不认识 window.{other}，已跳过", other)),
            }
        }
    }
    fn read_background(&mut self, value: &serde_json::Value) {
        let Some(keys) = value.as_object() else {
            self.problems
                .push(t!("background 应是一个对象，已跳过").to_owned());
            return;
        };
        let Some(path) = keys.get("image").and_then(|v| v.as_str()) else {
            self.problems
                .push(t!("background 缺少 image，已跳过").to_owned());
            return;
        };
        let mut image = BackgroundImage {
            path: path.to_owned(),
            mode: ImageMode::default(),
            strength: DEFAULT_IMAGE_STRENGTH,
        };
        for (key, value) in keys {
            match key.as_str() {
                "image" => {}
                "mode" => {
                    let named = ImageMode::ALL
                        .into_iter()
                        .find(|m| value.as_str() == Some(m.name()));
                    match named {
                        Some(mode) => image.mode = mode,
                        None => self.problems.push(t!(
                            "background.mode 应是 fill、fit、stretch 或 tile，写的是 {value}，已跳过",
                            value
                        )),
                    }
                }
                "strength" => match value.as_f64().filter(|v| (0.05..=1.).contains(v)) {
                    Some(v) => image.strength = (v * 100.).round() as u8,
                    None => self.problems.push(t!(
                        "background.strength 应在 0.05–1，写的是 {value}，已跳过",
                        value
                    )),
                },
                other => self
                    .problems
                    .push(t!("不认识 background.{other}，已跳过", other)),
            }
        }
        self.material.image = Some(image);
    }
    /// The file form, with only what the theme sets.
    pub fn to_json(&self) -> String {
        let mut root = serde_json::Map::new();
        root.insert("format".into(), FORMAT.into());
        root.insert("name".into(), self.name.clone().into());
        root.insert("bg".into(), hex(self.bg).into());
        root.insert("fg".into(), hex(self.fg).into());
        for (section, keys) in &self.colors {
            let keys: serde_json::Map<_, _> = keys
                .iter()
                .map(|(k, v)| (k.clone(), hex(*v).into()))
                .collect();
            root.insert(section.clone(), keys.into());
        }
        if !self.space_gradients.is_empty() {
            let pairs: Vec<serde_json::Value> = self
                .space_gradients
                .iter()
                .map(|[a, b]| vec![hex(*a), hex(*b)].into())
                .collect();
            root.insert("space_gradients".into(), pairs.into());
        }
        if let Some(dot) = self.seen_dot {
            let seen = root
                .entry("seen")
                .or_insert_with(|| serde_json::Map::new().into());
            if let Some(seen) = seen.as_object_mut() {
                seen.insert("dot".into(), dot.name().into());
            }
        }
        let material = &self.material;
        if material.opacity != 100 || material.blur {
            let mut window = serde_json::Map::new();
            if material.opacity != 100 {
                window.insert(
                    "opacity".into(),
                    (f64::from(material.opacity) / 100.).into(),
                );
            }
            if material.blur {
                window.insert("blur".into(), true.into());
            }
            root.insert("window".into(), window.into());
        }
        if let Some(image) = &material.image {
            let mut background = serde_json::Map::new();
            background.insert("image".into(), image.path.clone().into());
            background.insert("mode".into(), image.mode.name().into());
            background.insert("strength".into(), (f64::from(image.strength) / 100.).into());
            root.insert("background".into(), background.into());
        }
        serde_json::to_string_pretty(&serde_json::Value::Object(root)).unwrap_or_default()
    }
    /// An iTerm2 color preset (`.itermcolors`, an XML property list). Components are
    /// taken as sRGB; a preset in another color space comes out slightly off.
    pub fn from_itermcolors(text: &str, name: &str) -> Result<Self, String> {
        let read = |key: &str| iterm_color(text, key);
        let (Some(bg), Some(fg)) = (read("Background Color"), read("Foreground Color")) else {
            return Err(t!("不是 iTerm2 配色文件：找不到前景或背景颜色").to_owned());
        };
        let mut out = Self {
            name: name.to_owned(),
            bg,
            fg,
            ..Self::default()
        };
        for (i, ansi) in ANSI_NAMES.iter().enumerate() {
            out.set("ansi", ansi, read(&format!("Ansi {i} Color")));
        }
        for (key, name) in [
            ("Cursor Color", "cursor"),
            ("Cursor Text Color", "cursor_text"),
            ("Selection Color", "selection"),
            ("Selected Text Color", "selected_text"),
            ("Cursor Guide Color", "cursor_guide"),
            ("Bold Color", "bold"),
            ("Link Color", "link"),
            ("Underline Color", "underline"),
        ] {
            out.set("terminal", name, read(key));
        }
        Ok(out)
    }
}
/// Where a theme's relative picture is looked for: the plugin it ships in, else beside it.
pub fn base_of(file: &std::path::Path) -> std::path::PathBuf {
    let beside = file.parent().unwrap_or(std::path::Path::new("."));
    match beside.parent() {
        Some(plugin) if plugin.join("plugin.json").is_file() => plugin.to_owned(),
        _ => beside.to_owned(),
    }
}
/// A plugin's theme files, for `fibo plugins`.
pub fn load(files: Vec<std::path::PathBuf>) -> Vec<crate::agent::Loaded> {
    files
        .into_iter()
        .map(|file| {
            let stem = file
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let result = std::fs::read_to_string(&file)
                .map_err(|e| e.to_string())
                .and_then(|text| ThemeFile::parse(&text, &stem))
                .and_then(|theme| match theme.problems.is_empty() {
                    true => Ok(theme.name),
                    false => Err(theme.problems.join(t!("；"))),
                });
            crate::agent::Loaded { file, result }
        })
        .collect()
}
fn known_key(section: &str, key: &str) -> bool {
    match section {
        "ansi" => ANSI_NAMES.contains(&key),
        "terminal" => TERMINAL_NAMES.contains(&key),
        "ui" => UI_NAMES.iter().any(|(name, _)| *name == key),
        "status" => crate::status::Status::ALL.iter().any(|s| s.key() == key),
        "tone" => TONE_NAMES.contains(&key),
        "seen" => SEEN_NAMES.contains(&key),
        _ => false,
    }
}
/// Past what parsing skipped: what a theme can only get wrong against the disk or the eye.
/// A relative picture is looked for under `base`.
pub struct Check {
    pub theme: ThemeFile,
    /// Worth telling the author; the theme still works.
    pub warnings: Vec<String>,
}
const READABLE: f32 = 4.5;
pub fn check_file(path: &std::path::Path, base: &std::path::Path) -> Result<Check, String> {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut theme = ThemeFile::parse(&text, &stem)?;
    if let Some(problem) = theme.missing_image(base) {
        theme.problems.push(problem);
    }
    let contrast = crate::scheme::contrast(theme.fg, theme.bg);
    let warnings = (contrast < READABLE)
        .then(|| {
            t!(
                "前景和背景的对比度只有 {contrast}:1，低于 {readable}:1，文字可能看不清",
                contrast = format!("{contrast:.1}"),
                readable = READABLE
            )
        })
        .into_iter()
        .collect();
    Ok(Check { theme, warnings })
}
impl ThemeFile {
    /// Why its picture cannot be found, if it names one.
    pub fn missing_image(&self, base: &std::path::Path) -> Option<String> {
        let image = self.material.image.as_ref()?;
        (!base.join(&image.path).is_file()).then(|| t!("找不到背景图 {path}", path = image.path))
    }
}
pub fn hex(color: u32) -> String {
    format!("#{color:06x}")
}
/// `#rrggbb`, the one form a theme's colors are written in.
pub fn parse_hex(text: &str) -> Option<u32> {
    let digits = text.trim().strip_prefix('#')?;
    (digits.len() == 6 && digits.is_ascii())
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}
fn parse_color(value: &serde_json::Value) -> Result<u32, String> {
    value
        .as_str()
        .and_then(parse_hex)
        .ok_or_else(|| t!("应写成 #rrggbb，写的是 {value}", value))
}
/// One `<key>name</key><dict>…</dict>` of a preset. Property lists from iTerm2 are this
/// regular, which keeps a whole plist parser out of the build for one import.
fn iterm_color(text: &str, key: &str) -> Option<u32> {
    let start = text.find(&format!("<key>{key}</key>"))?;
    let rest = &text[start..];
    let dict = &rest[rest.find("<dict>")?..rest.find("</dict>")?];
    let component = |name: &str| -> Option<u32> {
        let at = dict.find(&format!("<key>{name} Component</key>"))?;
        let after = &dict[at..];
        let open = after.find("<real>")? + "<real>".len();
        let value: f64 = after[open..open + after[open..].find("</real>")?]
            .trim()
            .parse()
            .ok()?;
        Some((value.clamp(0., 1.) * 255.).round() as u32)
    };
    Some(component("Red")? << 16 | component("Green")? << 8 | component("Blue")?)
}
/// A file name for a theme's name: letters and digits of any script, the rest dashes.
pub fn file_stem(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        "theme".to_owned()
    } else {
        stem.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bg_and_fg_are_needed() {
        let t = ThemeFile::parse(r##"{"bg":"#0f1c2e","fg":"#d5dfef"}"##, "深海").unwrap();
        assert_eq!((t.name.as_str(), t.bg, t.fg), ("深海", 0x0f1c2e, 0xd5dfef));
        assert!(t.problems.is_empty());
        assert!(ThemeFile::parse(r##"{"bg":"#0f1c2e"}"##, "x").is_err());
        assert!(ThemeFile::parse(r##"{"bg":"blue","fg":"#ffffff"}"##, "x").is_err());
        assert!(ThemeFile::parse("not json", "x").is_err());
    }
    #[test]
    fn what_it_does_not_know_is_reported_and_skipped() {
        let t = ThemeFile::parse(
            r##"{"bg":"#000000","fg":"#ffffff","window":{"blur":3},
                "ansi":{"red":"#ff0000","purple":"#123456","green":"nope"},
                "terminal":{"cursor":"#00ff00","wobble":"#111111"}}"##,
            "x",
        )
        .unwrap();
        assert_eq!(t.color("ansi", "red"), Some(0xff0000));
        assert_eq!(t.color("ansi", "purple"), None);
        assert_eq!(t.color("ansi", "green"), None);
        assert_eq!(t.color("terminal", "cursor"), Some(0x00ff00));
        assert_eq!(t.problems.len(), 4, "{:?}", t.problems);
    }
    #[test]
    fn core_knows_every_name_the_window_does() {
        let t = ThemeFile::parse(
            r##"{"bg":"#000000","fg":"#ffffff",
                "ui":{"sidebar":"#111111","wobble":"#000000"},
                "status":{"running":"#222222","nope":"#000000"},
                "tone":{"ok":"#333333","good":"#000000"},
                "seen":{"title":"#444444","dot":"fade","colour":"#000000"}}"##,
            "x",
        )
        .unwrap();
        assert_eq!(t.color("ui", "sidebar"), Some(0x111111));
        assert_eq!(t.color("status", "running"), Some(0x222222));
        assert_eq!(t.color("tone", "ok"), Some(0x333333));
        assert_eq!(t.color("seen", "title"), Some(0x444444));
        assert_eq!(t.seen_dot, Some(SeenDot::Fade));
        assert_eq!(t.problems.len(), 4, "{:?}", t.problems);
        assert!(UI_NAMES.iter().any(|(name, _)| *name == "sidebar_key_line"));
    }
    #[test]
    fn a_newer_format_still_loads_and_says_so() {
        let t = ThemeFile::parse(r##"{"format":1,"bg":"#000000","fg":"#ffffff"}"##, "x").unwrap();
        assert!(t.problems.is_empty());
        assert!(t.to_json().contains("\"format\": 1"));
        let later =
            ThemeFile::parse(r##"{"format":99,"bg":"#000000","fg":"#ffffff"}"##, "x").unwrap();
        assert_eq!((later.bg, later.problems.len()), (0, 1));
        let odd = ThemeFile::parse(r##"{"format":"x","bg":"#000000","fg":"#ffffff"}"##, "x");
        assert_eq!(odd.unwrap().problems.len(), 1);
    }
    #[test]
    fn a_check_finds_what_parsing_cannot() {
        let dir = std::env::temp_dir().join(format!("fibo-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.json");
        std::fs::write(
            &file,
            r##"{"bg":"#101010","fg":"#202020","background":{"image":"gone.png"}}"##,
        )
        .unwrap();
        let check = check_file(&file, &dir).unwrap();
        assert_eq!(check.theme.problems.len(), 1, "{:?}", check.theme.problems);
        assert_eq!(check.warnings.len(), 1, "{:?}", check.warnings);
        std::fs::write(dir.join("gone.png"), "x").unwrap();
        assert!(check_file(&file, &dir).unwrap().theme.problems.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn survives_being_written_and_read_again() {
        let mut t = ThemeFile {
            name: "深海".into(),
            bg: 0x0f1c2e,
            fg: 0xd5dfef,
            ..ThemeFile::default()
        };
        t.set("ui", "sidebar", Some(0x0b1522));
        t.set("ansi", "red", Some(0xe5534b));
        t.space_gradients = vec![[0x111111, 0x222222]];
        t.set("seen", "title", Some(0x333333));
        t.seen_dot = Some(SeenDot::Fill);
        assert_eq!(ThemeFile::parse(&t.to_json(), "x").unwrap(), t);
        t.set("ui", "sidebar", None);
        assert_eq!(t.color("ui", "sidebar"), None);
        assert!(!t.colors.contains_key("ui"));
    }
    #[test]
    fn a_material_survives_and_what_is_out_of_range_is_named() {
        let t = ThemeFile::parse(
            r##"{"bg":"#000000","fg":"#ffffff","window":{"opacity":0.85,"blur":true},
                "background":{"image":"images/sea.jpg","mode":"tile","strength":0.4}}"##,
            "x",
        )
        .unwrap();
        assert!(t.problems.is_empty(), "{:?}", t.problems);
        assert_eq!((t.material.opacity, t.material.blur), (85, true));
        let image = t.material.image.as_ref().unwrap();
        assert_eq!(
            (image.path.as_str(), image.mode, image.strength),
            ("images/sea.jpg", ImageMode::Tile, 40)
        );
        assert_eq!(ThemeFile::parse(&t.to_json(), "x").unwrap(), t);
        let bad = ThemeFile::parse(
            r##"{"bg":"#000000","fg":"#ffffff","window":{"opacity":0.1,"glow":1},
                "background":{"image":"a.png","mode":"zoom","strength":3}}"##,
            "x",
        )
        .unwrap();
        assert_eq!(bad.material.opacity, 100);
        let image = bad.material.image.unwrap();
        assert_eq!(
            (image.mode, image.strength),
            (ImageMode::Fill, DEFAULT_IMAGE_STRENGTH)
        );
        assert_eq!(bad.problems.len(), 4, "{:?}", bad.problems);
        let none = ThemeFile::parse(r##"{"bg":"#000000","fg":"#ffffff"}"##, "x").unwrap();
        assert!(!none.to_json().contains("window") && !none.to_json().contains("background"));
    }
    #[test]
    fn colors_reach_the_terminal() {
        let t = ThemeFile::parse(
            r##"{"bg":"#101010","fg":"#eeeeee","ansi":{"bright_white":"#fefefe"},"terminal":{"cursor":"#abcdef"}}"##,
            "x",
        )
        .unwrap();
        let c = t.term_colors();
        assert_eq!((c.bg, c.fg, c.cursor), (0x101010, 0xeeeeee, 0xabcdef));
        assert_eq!(c.ansi()[15], Some(0xfefefe));
        assert_eq!(c.ansi()[0], None);
    }
    #[test]
    fn imports_an_itermcolors_preset() {
        let entry = |key: &str, r: f64, g: f64, b: f64| {
            format!(
                "<key>{key}</key><dict><key>Alpha Component</key><real>1</real><key>Blue Component</key><real>{b}</real><key>Color Space</key><string>sRGB</string><key>Green Component</key><real>{g}</real><key>Red Component</key><real>{r}</real></dict>"
            )
        };
        let text = format!(
            "<plist version=\"1.0\"><dict>{}{}{}{}</dict></plist>",
            entry("Ansi 1 Color", 1., 0., 0.),
            entry("Background Color", 0., 0., 0.2),
            entry("Foreground Color", 1., 1., 1.),
            entry("Selection Color", 0.5, 0.5, 0.5),
        );
        let t = ThemeFile::from_itermcolors(&text, "Preset").unwrap();
        assert_eq!((t.bg, t.fg), (0x000033, 0xffffff));
        assert_eq!(t.color("ansi", "red"), Some(0xff0000));
        assert_eq!(t.color("ansi", "green"), None);
        assert_eq!(t.color("terminal", "selection"), Some(0x808080));
        assert!(ThemeFile::from_itermcolors("<plist></plist>", "x").is_err());
    }
    #[test]
    fn a_file_stem_keeps_chinese_and_drops_separators() {
        assert_eq!(file_stem("深海 / 夜"), "深海---夜");
        assert_eq!(file_stem("../.."), "theme");
    }
}
