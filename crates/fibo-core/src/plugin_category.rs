//! What a plugin is for, in the words of 设置 › 插件. It follows from what the plugin
//! provides, so someone else's plugin lands in the right place without declaring anything.
use crate::plugin::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Effects,
    Look,
    Sidebar,
    Status,
    Commands,
    Agents,
}
impl Category {
    pub const ALL: [Self; 6] = [
        Self::Effects,
        Self::Look,
        Self::Sidebar,
        Self::Status,
        Self::Commands,
        Self::Agents,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Effects => crate::t!("完成动画"),
            Self::Look => crate::t!("外观"),
            Self::Sidebar => crate::t!("左栏"),
            Self::Status => crate::t!("状态和提醒"),
            Self::Commands => crate::t!("命令和检查"),
            Self::Agents => crate::t!("接入 Agent"),
        }
    }
    /// A plugin with several kinds goes by what you would look for it as: what it shows
    /// before what it runs. `setup` alone says nothing about its purpose.
    pub fn of(provides: &[Kind]) -> Self {
        let has = |k: Kind| provides.contains(&k);
        if has(Kind::Effects) {
            Self::Effects
        } else if has(Kind::Themes) {
            Self::Look
        } else if has(Kind::Agent) {
            Self::Agents
        } else if has(Kind::Sidebar) {
            Self::Sidebar
        } else if has(Kind::Quota) || has(Kind::Hooks) || has(Kind::Decorations) {
            Self::Status
        } else {
            Self::Commands
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every plugin we ship, by the `provides` in its manifest.
    #[test]
    fn the_shipped_plugins_land_where_people_look_for_them() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let mut seen = 0;
        for dir in std::fs::read_dir(&root).unwrap().flatten() {
            let Ok(text) = std::fs::read_to_string(dir.path().join("plugin.json")) else {
                continue;
            };
            let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
            let provides: Vec<Kind> = manifest["provides"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|k| Kind::from_name(k.as_str().unwrap()))
                .collect();
            let id = dir.file_name().to_string_lossy().into_owned();
            let want = match id.as_str() {
                x if x.starts_with("complete-") => Category::Effects,
                x if x.starts_with("theme-") => Category::Look,
                "focus-sidebar" | "sidebar-status" => Category::Sidebar,
                "claude-quota" | "notify-macos" | "pane-facts" => Category::Status,
                "common-actions" | "project-checks" => Category::Commands,
                "opencode" => Category::Agents,
                other => panic!("say where the new plugin {other} belongs"),
            };
            assert_eq!(Category::of(&provides), want, "{id}");
            seen += 1;
        }
        assert!(seen >= 25, "found only {seen} plugins");
    }

    #[test]
    fn setup_alone_is_a_command_and_never_wins_over_what_it_shows() {
        assert_eq!(Category::of(&[Kind::Setup]), Category::Commands);
        assert_eq!(Category::of(&[Kind::Setup, Kind::Quota]), Category::Status);
    }
}
