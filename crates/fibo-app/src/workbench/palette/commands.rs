//! ⌘P finds what the app can do, not only what it holds: the workbench's shortcut
//! actions (鸟瞰、新建待办、分屏、设置…) and the settings pages, by their names in either
//! language and a few words people type for them. Before, 「设置」「鸟瞰」「检查更新」
//! all said 没有找到.
use super::*;
use crate::keymap::Action;

/// The shortcut actions worth running from here: the workbench's, not a terminal's own
/// (查找、清屏、字号 act on the terminal that has focus, which the palette took), and not
/// ⌘P itself or the fixed ⌘1–⌘9.
pub(super) fn offered() -> impl Iterator<Item = Action> {
    Action::ALL
        .into_iter()
        .filter(|a| !a.in_terminal() && !a.fixed() && *a != Action::Search)
}

/// Words besides the page's name that find it; Chinese always, as the views do.
fn page_words(page: settings::Page) -> &'static str {
    use settings::Page;
    match page {
        Page::General => not_ui!("通用 默认 Agent hook 钩子 语言 编辑器 general language"),
        Page::Look => not_ui!("外观 主题 颜色 透明 背景 图标 字体 theme color appearance"),
        Page::Terminal => not_ui!("终端 字号 字体 光标 对比度 terminal font cursor"),
        Page::Keys => not_ui!("快捷键 键位 按键 shortcut keys keybinding"),
        Page::Notifications => not_ui!("通知 提示音 提醒 声音 notification sound"),
        Page::Phones => not_ui!("连接手机 连手机 远程 中继 配对 二维码 phone remote relay pair"),
        Page::Plugins => not_ui!("插件 市场 安装 plugin market"),
        Page::About => not_ui!("关于 更新 检查更新 版本 隐私 about update version"),
    }
}

/// The commands and settings pages `needle` finds, in the palette's order.
fn finds(needle: &SearchQuery) -> (Vec<Action>, Vec<settings::Page>) {
    let commands = offered().filter(|a| needle.matches(&a.label())).collect();
    let pages = settings::Page::ALL
        .into_iter()
        .filter(|p| needle.matches(&format!("{} {} {}", t!("设置"), p.label(), page_words(*p))))
        .collect();
    (commands, pages)
}

impl Workbench {
    pub(super) fn command_hits(&self, needle: &SearchQuery) -> Vec<Hit> {
        let (commands, pages) = finds(needle);
        commands
            .into_iter()
            .map(Hit::Command)
            .chain(pages.into_iter().map(Hit::Settings))
            .collect()
    }
    /// The row's title and the keys that run the same, as bound now.
    pub(super) fn command_row(&self, action: Action, cx: &gpui::App) -> (String, String) {
        let keys = crate::keymap::current(cx)
            .chords(action)
            .first()
            .map(|c| c.display())
            .unwrap_or_default();
        (action.label(), keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use settings::Page;

    #[test]
    fn what_people_type_for_the_apps_own_things_finds_them() {
        let find = |q: &str| finds(&SearchQuery::new(q));
        assert!(find("鸟瞰").0.contains(&Action::Overview));
        assert!(find("待办").0.contains(&Action::NewTodo));
        assert!(find("分屏").0.contains(&Action::SplitRight));
        let (commands, pages) = find("设置");
        assert!(commands.contains(&Action::Settings));
        assert_eq!(pages.len(), Page::ALL.len(), "设置 lists every page");
        assert_eq!(find("检查更新").1, vec![Page::About]);
        assert_eq!(find("快捷键").1, vec![Page::Keys]);
        assert_eq!(find("连接手机").1, vec![Page::Phones]);
        assert_eq!(find("提示音").1, vec![Page::Notifications]);
        assert!(find("theme").1.contains(&Page::Look));
        // A terminal's own action needs the terminal the palette took focus from.
        assert!(!find("清屏").0.contains(&Action::Clear));
    }
}
