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
        super::status_colors::write_atomic(&dir.join(FILE), text.as_bytes())
            .map_err(|e| format!("无法保存 {}：{e}", dir.join(FILE).display()))
    }
    pub fn of(&self, space: &str) -> Option<&str> {
        self.chosen.get(space).map(String::as_str)
    }
}
/// The current space's theme, when it has one that still exists.
pub fn entry(cx: &App) -> Option<&Entry> {
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
    crate::set_menus(cx);
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
    crate::set_menus(cx);
    cx.refresh_windows();
    Ok(())
}
/// The current space follows 外观 again.
pub fn release_current(cx: &mut App) -> Result<(), String> {
    let current = cx
        .try_global::<SpaceThemes>()
        .and_then(|s| s.current.clone());
    current.map_or(Ok(()), |space| set(&space, None, cx))
}
/// A theme the editor just saved, from what the current space wears: this space alone
/// wears the result, and what 外观 chose stays as it was.
pub fn saved_as(id: &str, cx: &mut App) -> Result<(), String> {
    let current = cx
        .try_global::<SpaceThemes>()
        .and_then(|s| s.current.clone());
    current.map_or(Ok(()), |space| set(&space, Some(id), cx))
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
