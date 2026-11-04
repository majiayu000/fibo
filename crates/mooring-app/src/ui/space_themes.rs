//! A theme for one space: the whole window, terminals included, wears it while that space
//! is the current one. Spaces without one follow 外观. Saved beside the themes, since a
//! space's id and a theme's id are both this machine's.
use super::{
    status_colors::config_dir,
    themes::{Entry, Themes},
};
use gpui::{App, Global};
use std::collections::BTreeMap;

const FILE: &str = "space-themes.json";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpaceThemes {
    chosen: BTreeMap<String, String>,
    current: Option<String>,
    /// The list is unfolded in the space menu.
    pub menu_open: bool,
}
impl Global for SpaceThemes {}
impl SpaceThemes {
    fn parse(text: &str) -> BTreeMap<String, String> {
        serde_json::from_str(text).unwrap_or_else(|e| {
            eprintln!("mooring: reading {FILE}: {e}");
            BTreeMap::new()
        })
    }
    pub fn load() -> Self {
        let text = config_dir().and_then(|dir| std::fs::read_to_string(dir.join(FILE)).ok());
        Self {
            chosen: text.as_deref().map(Self::parse).unwrap_or_default(),
            ..Self::default()
        }
    }
    fn save(&self) -> Result<(), String> {
        let dir = config_dir().ok_or("找不到配置目录")?;
        let text = serde_json::to_string_pretty(&self.chosen).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("无法保存 {}：{e}", dir.display()))?;
        std::fs::write(dir.join(FILE), text)
            .map_err(|e| format!("无法保存 {}：{e}", dir.join(FILE).display()))
    }
    pub fn of(&self, space: &str) -> Option<&str> {
        self.chosen.get(space).map(String::as_str)
    }
}
/// The current space's theme, when it has one that still exists.
pub fn shown(cx: &App) -> Option<&Entry> {
    let spaces = cx.try_global::<SpaceThemes>()?;
    let id = spaces.of(spaces.current.as_deref()?)?;
    cx.try_global::<Themes>()?.get(id)
}
/// Whether the window should look again: a plain comparison, and none at all for someone
/// who never gave a space a theme, because the workbench asks on every render.
pub fn needs_follow(space: &str, cx: &App) -> bool {
    cx.try_global::<SpaceThemes>()
        .is_some_and(|spaces| !spaces.chosen.is_empty() && spaces.current.as_deref() != Some(space))
}
pub fn follow(space: &str, cx: &mut App) {
    let mut spaces = cx.try_global::<SpaceThemes>().cloned().unwrap_or_default();
    spaces.current = Some(space.to_owned());
    cx.set_global(spaces);
    super::appearance::apply(cx);
}
/// Give a space a theme, or `None` to have it follow 外观 again.
pub fn set(space: &str, theme: Option<&str>, cx: &mut App) -> Result<(), String> {
    let mut spaces = cx.try_global::<SpaceThemes>().cloned().unwrap_or_default();
    match theme {
        Some(id) => spaces.chosen.insert(space.to_owned(), id.to_owned()),
        None => spaces.chosen.remove(space),
    };
    spaces.current = Some(space.to_owned());
    spaces.save()?;
    cx.set_global(spaces);
    super::appearance::apply(cx);
    cx.refresh_windows();
    Ok(())
}
/// A theme the editor just saved, from a copy of what the current space wears: the space
/// keeps wearing it.
pub fn saved_as(id: &str, cx: &mut App) {
    let Some(mut spaces) = cx.try_global::<SpaceThemes>().cloned() else {
        return;
    };
    let Some(space) = spaces.current.clone().filter(|s| spaces.of(s).is_some()) else {
        return;
    };
    spaces.chosen.insert(space, id.to_owned());
    if let Err(error) = spaces.save() {
        eprintln!("mooring: {error}");
    }
    cx.set_global(spaces);
}
pub fn toggle_menu(cx: &mut App) {
    let mut spaces = cx.try_global::<SpaceThemes>().cloned().unwrap_or_default();
    spaces.menu_open = !spaces.menu_open;
    cx.set_global(spaces);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_broken_file_gives_no_space_a_theme() {
        assert!(SpaceThemes::parse("{").is_empty());
        let chosen = SpaceThemes::parse(r#"{"s1":"slate"}"#);
        let spaces = SpaceThemes {
            chosen,
            ..SpaceThemes::default()
        };
        assert_eq!(spaces.of("s1"), Some("slate"));
        assert_eq!(spaces.of("s2"), None);
    }
}
