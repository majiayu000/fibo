//! `colors.json` used to sit on top of every theme, a second owner of the status colors. At
//! the first start that finds one its colors move into the themes: every theme file of
//! the user's, and a copy of each side's current theme when that is not theirs to write.
//! The old file stays as `colors.json.migrated`. One that cannot be read stays where it is
//! and 设置 › 外观 names it.
use super::appearance::Appearance;
use super::status_colors::{config_dir, write_atomic};
use super::themes::{Themes, dir};
use fibo_core::status::Status;
use fibo_core::theme::{SEEN_NAMES, SeenDot, TONE_NAMES, ThemeFile, file_stem, parse_hex};
use gpui::App;
use std::path::Path;

#[derive(Debug, Default, PartialEq, Eq)]
struct Overrides {
    colors: Vec<(&'static str, String, u32)>,
    dot: Option<SeenDot>,
}
impl Overrides {
    fn apply(&self, file: &mut ThemeFile) {
        for (section, key, color) in &self.colors {
            file.set(section, key, Some(*color));
        }
        if self.dot.is_some() {
            file.seen_dot = self.dot;
        }
    }
}
/// Keys a colors.json written before 2026-11-02 may still use.
fn current_status(key: &str) -> Option<&str> {
    match key {
        "working" => Some(Status::Running.key()),
        "done" | "ready" => Some(Status::Review.key()),
        key if Status::ALL.iter().any(|s| s.key() == key) => Some(key),
        _ => None,
    }
}
fn color(value: &serde_json::Value) -> Result<u32, String> {
    value
        .as_str()
        .and_then(parse_hex)
        .ok_or_else(|| t!("应写成 #rrggbb，写的是 {value}", value))
}
fn parse(text: &str) -> Result<Overrides, String> {
    let root: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut out = Overrides::default();
    let keys = |name: &str| root.get(name).and_then(|v| v.as_object()).cloned();
    for (key, value) in keys("status").unwrap_or_default() {
        let at = current_status(&key).ok_or_else(|| t!("不认识 status.{key}", key))?;
        out.colors.push(("status", at.to_owned(), color(&value)?));
    }
    for (key, value) in keys("tone").unwrap_or_default() {
        if !TONE_NAMES.contains(&key.as_str()) {
            return Err(t!("不认识 tone.{key}", key));
        }
        out.colors.push(("tone", key, color(&value)?));
    }
    for (key, value) in keys("seen").unwrap_or_default() {
        if key == "dot" {
            let named = SeenDot::ALL
                .into_iter()
                .find(|dot| value.as_str() == Some(dot.name()));
            out.dot = Some(named.ok_or_else(|| t!("seen.dot 写的是 {value}", value))?);
        } else if SEEN_NAMES.contains(&key.as_str()) {
            out.colors.push(("seen", key, color(&value)?));
        } else {
            return Err(t!("不认识 seen.{key}", key));
        }
    }
    Ok(out)
}
/// Moves the file's colors into the themes. `true` when there was a file to move; safe to
/// run again after a crash, since it only writes the same colors over the same names.
fn migrate(
    config: &Path,
    appearance: &mut Appearance,
    themes: &Themes,
    save: impl FnOnce(&Appearance) -> Result<(), String>,
) -> Result<bool, String> {
    let old = config.join("colors.json");
    let Ok(text) = std::fs::read_to_string(&old) else {
        return Ok(false);
    };
    let overrides = parse(&text)?;
    let folder = config.join("themes");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let write = |path: &Path, file: &ThemeFile| {
        write_atomic(path, file.to_json().as_bytes()).map_err(|e| e.to_string())
    };
    for entry in &themes.entries {
        if let Some(path) = entry.own_file() {
            let mut file = entry.as_file();
            overrides.apply(&mut file);
            write(&path, &file)?;
        }
    }
    for light in [true, false] {
        let Some(entry) = appearance.entry(themes, light) else {
            continue;
        };
        if entry.own_file().is_some() {
            continue;
        }
        let mut file = entry.as_file();
        file.name = t!("{name} 我的状态色", name = entry.name);
        overrides.apply(&mut file);
        let stem = file_stem(&file.name);
        write(&folder.join(format!("{stem}.json")), &file)?;
        let id = format!("file:{stem}");
        if light {
            appearance.light = id;
        } else {
            appearance.dark = id;
        }
    }
    save(appearance)?;
    std::fs::rename(&old, config.join("colors.json.migrated")).map_err(|e| e.to_string())?;
    Ok(true)
}
/// At start, before the first theme is applied.
pub fn run(cx: &mut App) {
    let Some(config) = config_dir() else { return };
    let mut appearance = cx.try_global::<Appearance>().cloned().unwrap_or_default();
    let Some(themes) = cx.try_global::<Themes>().cloned() else {
        return;
    };
    match migrate(&config, &mut appearance, &themes, Appearance::save) {
        Ok(true) => {
            cx.set_global(appearance);
            cx.set_global(Themes::read(dir().as_deref()));
        }
        Ok(false) => {}
        Err(error) => eprintln!("fibo: colors.json 没能并入主题：{error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fibo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        dir
    }
    #[test]
    fn reads_what_the_old_file_could_say() {
        let o = parse(
            r##"{"status":{"working":"#112233","done":"#445566","error":"#778899"},
                "tone":{"ok":"#223344"},"seen":{"title":"#445566","dot":"fade"}}"##,
        )
        .unwrap();
        let has = |section: &str, key: &str, color: u32| {
            o.colors
                .iter()
                .any(|(s, k, c)| *s == section && k == key && *c == color)
        };
        assert!(has("status", "running", 0x112233), "{o:?}");
        assert!(has("status", "review", 0x445566));
        assert!(has("status", "error", 0x778899));
        assert!(has("tone", "ok", 0x223344));
        assert!(has("seen", "title", 0x445566));
        assert_eq!(o.dot, Some(SeenDot::Fade));
        assert_eq!(parse("{}").unwrap(), Overrides::default());
    }
    #[test]
    fn typos_and_bad_colors_are_refused() {
        for bad in [
            r##"{"status":{"wroking":"#112233"}}"##,
            r##"{"status":{"running":"yellow"}}"##,
            r##"{"tone":{"good":"#112233"}}"##,
            r##"{"seen":{"dot":"square"}}"##,
            "{",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn colors_move_into_themes_and_the_old_file_is_kept_aside() {
        let config = folder("migrate");
        std::fs::write(
            config.join("themes/mine.json"),
            r##"{"name":"mine","bg":"#101820","fg":"#d0d8e0","status":{"running":"#000001"}}"##,
        )
        .unwrap();
        std::fs::write(
            config.join("colors.json"),
            r##"{"status":{"running":"#112233"},"seen":{"dot":"fill"}}"##,
        )
        .unwrap();
        let themes = Themes::read(Some(&config.join("themes")));
        assert!(themes.problems.iter().any(|(f, _)| f == "colors.json"));
        let mut appearance = Appearance::default();
        let mut saved = None;
        assert!(
            migrate(&config, &mut appearance, &themes, |a| {
                saved = Some(a.clone());
                Ok(())
            })
            .unwrap()
        );
        assert_eq!(saved.as_ref(), Some(&appearance));
        assert!(!config.join("colors.json").exists());
        assert!(config.join("colors.json.migrated").is_file());
        let after = Themes::read(Some(&config.join("themes")));
        assert!(after.problems.is_empty(), "{:?}", after.problems);
        // Over any theme, as the file used to sit: the user's own is overwritten.
        let (_, mine) = after.get("file:mine").unwrap().file.clone().unwrap();
        assert_eq!(mine.color("status", "running"), Some(0x112233));
        assert_eq!(mine.seen_dot, Some(SeenDot::Fill));
        // A built-in side gets a copy with the same colors and is chosen in its place.
        for id in [&appearance.light, &appearance.dark] {
            let entry = after.get(id).unwrap_or_else(|| panic!("{id}"));
            assert!(entry.name.ends_with("我的状态色"), "{}", entry.name);
            assert_eq!(entry.palette.status[0..].iter().flatten().count(), 1);
        }
        assert_eq!(
            after.get(&appearance.dark).unwrap().palette.term,
            themes.get("dark").unwrap().palette.term
        );
        // Nothing left to do the second time.
        let mut again = appearance.clone();
        assert!(!migrate(&config, &mut again, &after, |_| Ok(())).unwrap());
        assert_eq!(again, appearance);
        std::fs::remove_dir_all(&config).unwrap();
    }
    #[test]
    fn a_broken_file_is_left_where_it_is() {
        let config = folder("migrate-broken");
        std::fs::write(
            config.join("colors.json"),
            r##"{"status":{"oops":"#112233"}}"##,
        )
        .unwrap();
        let themes = Themes::read(Some(&config.join("themes")));
        let mut appearance = Appearance::default();
        assert!(migrate(&config, &mut appearance, &themes, |_| Ok(())).is_err());
        assert!(config.join("colors.json").is_file());
        assert_eq!(appearance, Appearance::default());
        std::fs::remove_dir_all(&config).unwrap();
    }
}
