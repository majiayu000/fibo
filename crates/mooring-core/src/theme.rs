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
const SECTIONS: [&str; 5] = ["terminal", "ansi", "ui", "status", "tone"];

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
    /// Section → key → color. `ui`, `status` and `tone` keys are checked by the window,
    /// which owns those names; `terminal` and `ansi` here.
    pub colors: BTreeMap<String, BTreeMap<String, u32>>,
    pub space_gradients: Vec<[u32; 2]>,
    pub material: Material,
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
        let root = root.as_object().ok_or("主题文件应是一个 JSON 对象")?;
        let required = |key: &str| {
            root.get(key)
                .ok_or_else(|| format!("缺少 {key}"))
                .and_then(parse_color)
                .map_err(|e| format!("{key}：{e}"))
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
                "space_gradients" => out.read_gradients(value),
                "window" => out.read_window(value),
                "background" => out.read_background(value),
                section if SECTIONS.contains(&section) => out.read_section(section, value),
                other => out.problems.push(format!("不认识 {other}，已跳过")),
            }
        }
        Ok(out)
    }
    fn read_section(&mut self, section: &str, value: &serde_json::Value) {
        let Some(keys) = value.as_object() else {
            self.problems
                .push(format!("{section} 应是一个对象，已跳过"));
            return;
        };
        for (key, value) in keys {
            let known = match section {
                "ansi" => ANSI_NAMES.contains(&key.as_str()),
                "terminal" => TERMINAL_NAMES.contains(&key.as_str()),
                _ => true,
            };
            if !known {
                self.problems
                    .push(format!("不认识 {section}.{key}，已跳过"));
                continue;
            }
            match parse_color(value) {
                Ok(color) => self.set(section, key, Some(color)),
                Err(e) => self.problems.push(format!("{section}.{key}：{e}，已跳过")),
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
                .push("space_gradients 应是若干对颜色，已跳过".to_owned()),
        }
    }
    fn read_window(&mut self, value: &serde_json::Value) {
        let Some(keys) = value.as_object() else {
            self.problems.push("window 应是一个对象，已跳过".to_owned());
            return;
        };
        for (key, value) in keys {
            match (key.as_str(), value) {
                ("opacity", value) => match value.as_f64().filter(|v| (0.3..=1.).contains(v)) {
                    Some(v) => self.material.opacity = (v * 100.).round() as u8,
                    None => self
                        .problems
                        .push(format!("window.opacity 应在 0.3–1，写的是 {value}，已跳过")),
                },
                ("blur", serde_json::Value::Bool(blur)) => self.material.blur = *blur,
                ("blur", value) => self.problems.push(format!(
                    "window.blur 应是 true 或 false，写的是 {value}，已跳过"
                )),
                (other, _) => self.problems.push(format!("不认识 window.{other}，已跳过")),
            }
        }
    }
    fn read_background(&mut self, value: &serde_json::Value) {
        let Some(keys) = value.as_object() else {
            self.problems
                .push("background 应是一个对象，已跳过".to_owned());
            return;
        };
        let Some(path) = keys.get("image").and_then(|v| v.as_str()) else {
            self.problems
                .push("background 缺少 image，已跳过".to_owned());
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
                        None => self.problems.push(format!(
                            "background.mode 应是 fill、fit、stretch 或 tile，写的是 {value}，已跳过"
                        )),
                    }
                }
                "strength" => match value.as_f64().filter(|v| (0.05..=1.).contains(v)) {
                    Some(v) => image.strength = (v * 100.).round() as u8,
                    None => self.problems.push(format!(
                        "background.strength 应在 0.05–1，写的是 {value}，已跳过"
                    )),
                },
                other => self
                    .problems
                    .push(format!("不认识 background.{other}，已跳过")),
            }
        }
        self.material.image = Some(image);
    }
    /// The file form, with only what the theme sets.
    pub fn to_json(&self) -> String {
        let mut root = serde_json::Map::new();
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
            return Err("不是 iTerm2 配色文件：找不到前景或背景颜色".to_owned());
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
/// A plugin's theme files, for `mooring plugins check`. The window checks them again with
/// the names of its own colors.
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
                    false => Err(theme.problems.join("；")),
                });
            crate::agent::Loaded { file, result }
        })
        .collect()
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
        .ok_or_else(|| format!("应写成 #rrggbb，写的是 {value}"))
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
