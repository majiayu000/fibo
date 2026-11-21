//! The menu bar: Fibo, 文件, 编辑, 视图, 转到, 窗口, 帮助. A shortcut still runs through
//! `crate::keymap` as the key reaches the window; an item shows the chord the user has on
//! that action now, and runs the same action when clicked or when the window leaves a key
//! unhandled (a key the window takes never reaches the menu).
use crate::keymap::{self, Action as Key, Chord};
use crate::ui;
use gpui::{App, KeyBinding, Menu, MenuItem, SystemMenuType};

gpui::actions!(mooring, [Quit, ShowMenuPanel, ShowSettings]);
gpui::actions!(
    fibo,
    [
        About,
        Hide,
        HideOthers,
        ShowAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Minimize,
        Zoom,
        ShowKeys,
        OpenWebsite
    ]
);
/// Fibo → 外观: one item per choice, the current one checked.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = mooring, no_json)]
pub(crate) struct ChooseAppearance(pub usize);
/// A keymap action from the menu bar: the focused terminal runs its own, the workbench
/// the rest, as when the key is pressed.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = fibo, no_json)]
pub(crate) struct RunKey(pub Key);

/// No element has this key context, so a binding under it never runs on a key press; GPUI
/// still writes the binding's chord on the menu item.
const SHOWN_ONLY: &str = "FiboMenuShortcut";
/// Where 窗口 and 帮助 sit in the bar, for AppKit's window list and help search.
const WINDOW_MENU: usize = 5;
const HELP_MENU: usize = 6;
const WEBSITE: &str = "https://fibo.page";

/// The global handlers: those that need no window, and the window's for when nothing in it
/// has focus.
pub(crate) fn install(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &OpenWebsite, cx| cx.open_url(WEBSITE));
    crate::workbench::install_menu_bar(cx);
}

/// Builds the bar again: after a rebind, a theme change or a language change.
pub(crate) fn set_menus(cx: &mut App) {
    let keys = keymap::current(cx);
    cx.clear_key_bindings();
    cx.bind_keys(bindings(&keys, fibo_platform::desktop::HAS_COMMAND_KEY));
    let bar = bar(looks(cx));
    cx.set_menus(bar);
    crate::desktop::mark_menus(WINDOW_MENU, HELP_MENU);
}

fn bar(looks: Vec<MenuItem>) -> Vec<Menu> {
    let key = |action: Key| MenuItem::action(action.label(), RunKey(action));
    let tabs = (1..=8)
        .map(|n| key(Key::Tab(n)))
        .chain([key(Key::LastTab)])
        .collect::<Vec<_>>();
    vec![
        Menu::new("Fibo").items([
            MenuItem::action(t!("关于 Fibo"), About),
            MenuItem::separator(),
            MenuItem::action(t!("设置…"), RunKey(Key::Settings)),
            MenuItem::action(t!("菜单栏面板"), ShowMenuPanel),
            MenuItem::submenu(Menu::new(t!("外观")).items(looks)),
            MenuItem::separator(),
            MenuItem::os_submenu(t!("服务#menu"), SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action(t!("隐藏 Fibo"), Hide),
            MenuItem::action(t!("隐藏其他"), HideOthers),
            MenuItem::action(t!("全部显示"), ShowAll),
            MenuItem::separator(),
            MenuItem::action(t!("退出 Fibo"), Quit),
        ]),
        Menu::new(t!("文件")).items([
            key(Key::NewTab),
            key(Key::NewTodo),
            MenuItem::separator(),
            key(Key::SplitRight),
            key(Key::SplitDown),
            MenuItem::separator(),
            key(Key::Close),
        ]),
        Menu::new(t!("编辑")).items([
            MenuItem::action(t!("撤销"), Undo),
            MenuItem::separator(),
            MenuItem::action(t!("剪切"), Cut),
            MenuItem::action(t!("复制"), Copy),
            MenuItem::action(t!("粘贴"), Paste),
            key(Key::SelectAll),
            MenuItem::separator(),
            key(Key::Find),
            key(Key::Clear),
        ]),
        Menu::new(t!("视图")).items([
            key(Key::ToggleSidebar),
            MenuItem::separator(),
            key(Key::ZoomIn),
            key(Key::ZoomOut),
            key(Key::ZoomReset),
            MenuItem::separator(),
            key(Key::ScrollTop),
            key(Key::ScrollBottom),
            key(Key::PageUp),
            key(Key::PageDown),
        ]),
        Menu::new(t!("转到")).items([
            key(Key::Search),
            key(Key::NextWaiting),
            key(Key::Overview),
            MenuItem::separator(),
            key(Key::PrevTab),
            key(Key::NextTab),
            MenuItem::submenu(Menu::new(t!("切换到标签")).items(tabs)),
            MenuItem::separator(),
            key(Key::PrevPane),
            key(Key::NextPane),
            key(Key::PaneLeft),
            key(Key::PaneRight),
            key(Key::PaneUp),
            key(Key::PaneDown),
        ]),
        Menu::new(t!("窗口")).items([
            MenuItem::action(t!("最小化"), Minimize),
            MenuItem::action(t!("缩放#window"), Zoom),
        ]),
        Menu::new(t!("帮助")).items([
            MenuItem::action(t!("Fibo 网站"), OpenWebsite),
            MenuItem::action(t!("快捷键一览"), ShowKeys),
        ]),
    ]
}

/// GPUI's own bindings, rebuilt with the menu: ⌘Q, macOS's ⌘H ⌥⌘H ⌘M, and the chords the
/// menu shows. The keymap's chords are bound only for show (they keep running through
/// `keymap::action`), and a system chord the user put on an action stays theirs.
fn bindings(keys: &keymap::Keymap, command_key: bool) -> Vec<KeyBinding> {
    let mut bindings = vec![KeyBinding::new("cmd-q", Quit, None)];
    if !command_key {
        return bindings;
    }
    let taken = |text: &str| {
        Chord::parse(text)
            .is_some_and(|chord| Key::ALL.iter().any(|a| keys.chords(*a).contains(&chord)))
    };
    // ⌘C and ⌘V the keymap refuses; the rest an older keys.json may already use.
    for (text, binding) in [
        ("cmd-h", KeyBinding::new("cmd-h", Hide, None)),
        ("cmd-alt-h", KeyBinding::new("cmd-alt-h", HideOthers, None)),
        ("cmd-m", KeyBinding::new("cmd-m", Minimize, None)),
        ("cmd-z", KeyBinding::new("cmd-z", Undo, Some(SHOWN_ONLY))),
        ("cmd-x", KeyBinding::new("cmd-x", Cut, Some(SHOWN_ONLY))),
        ("cmd-c", KeyBinding::new("cmd-c", Copy, Some(SHOWN_ONLY))),
        ("cmd-v", KeyBinding::new("cmd-v", Paste, Some(SHOWN_ONLY))),
    ] {
        if !taken(text) {
            bindings.push(binding);
        }
    }
    for action in Key::ALL {
        if let Some(chord) = keys.chords(action).first()
            && gpui::Keystroke::parse(&chord.text()).is_ok()
        {
            bindings.push(KeyBinding::new(
                &chord.text(),
                RunKey(action),
                Some(SHOWN_ONLY),
            ));
        }
    }
    bindings
}

fn looks(cx: &App) -> Vec<MenuItem> {
    use ui::appearance::{Appearance, Mode};
    let chosen = cx.try_global::<Appearance>().cloned().unwrap_or_default();
    let mut looks = Mode::ALL
        .iter()
        .enumerate()
        .map(|(i, m)| MenuItem::action(m.label(), ChooseAppearance(i)).checked(*m == chosen.mode))
        .collect::<Vec<_>>();
    // Choosing below would change nothing in this space, so the menu says why.
    if let Some(shown) = ui::appearance::shown(cx)
        && shown.source == ui::appearance::Source::Space
        && let Some(entry) = shown.entry
    {
        looks.push(MenuItem::separator());
        looks.push(MenuItem::action(
            t!("当前空间单独用着「{name}」", name = entry.name),
            ShowSettings,
        ));
    }
    // Past the modes, an entry's place in `Themes`: each side has its own check mark.
    if let Some(themes) = cx.try_global::<ui::themes::Themes>() {
        for light in [true, false] {
            let on = chosen.entry(themes, light).map(|entry| entry.id.as_str());
            looks.push(MenuItem::separator());
            for (i, entry) in themes.entries.iter().enumerate() {
                if entry.light() == light {
                    looks.push(
                        MenuItem::action(
                            entry.name.as_str(),
                            ChooseAppearance(Mode::ALL.len() + i),
                        )
                        .checked(on == Some(entry.id.as_str())),
                    );
                }
            }
        }
    }
    looks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(items: &[MenuItem], out: &mut Vec<Key>) {
        for item in items {
            match item {
                MenuItem::Action { action, .. } => {
                    out.extend(action.as_any().downcast_ref::<RunKey>().map(|r| r.0));
                }
                MenuItem::Submenu(menu) => runs(&menu.items, out),
                _ => (),
            }
        }
    }
    fn shown(bindings: &[KeyBinding], action: &dyn gpui::Action) -> Vec<String> {
        bindings
            .iter()
            .filter(|b| b.action().partial_eq(action))
            .map(|b| b.keystrokes().iter().map(|k| k.inner().unparse()).collect())
            .collect()
    }

    fn bindings_for(keys: &keymap::Keymap) -> Vec<KeyBinding> {
        bindings(keys, true)
    }

    #[test]
    fn every_keymap_action_has_a_menu_item() {
        let mut found = vec![];
        for menu in bar(vec![]) {
            runs(&menu.items, &mut found);
        }
        for action in Key::ALL {
            assert!(
                found.contains(&action),
                "{} has no menu item",
                action.name()
            );
        }
        assert_eq!(
            found.len(),
            Key::ALL.len(),
            "an action is in the menu twice"
        );
    }

    #[test]
    fn menu_shows_the_chord_the_user_has_now() {
        let mut keys = keymap::Keymap::default();
        keys.assign(Key::NewTab, Chord::parse("cmd-n").unwrap());
        let bindings = bindings(&keys, true);
        assert_eq!(shown(&bindings, &RunKey(Key::NewTab)), ["cmd-n"]);
        assert_eq!(shown(&bindings, &RunKey(Key::Close)), ["cmd-w"]);
        // Shown only: no element has the context, so the keymap alone runs the key.
        let binding = bindings
            .iter()
            .find(|b| b.action().partial_eq(&RunKey(Key::NewTab)))
            .unwrap();
        assert!(binding.predicate().is_some());
    }

    #[test]
    fn a_system_chord_the_user_took_stays_theirs() {
        let defaults = bindings(&keymap::Keymap::default(), true);
        assert_eq!(shown(&defaults, &Hide), ["cmd-h"]);
        assert_eq!(shown(&defaults, &Minimize), ["cmd-m"]);
        let mut keys = keymap::Keymap::default();
        keys.assign(Key::ToggleSidebar, Chord::parse("cmd-h").unwrap());
        let bindings = bindings(&keys, true);
        assert!(shown(&bindings, &Hide).is_empty());
        assert_eq!(shown(&bindings, &RunKey(Key::ToggleSidebar)), ["cmd-h"]);
        // An older keys.json with an action on ⌘X: 剪切 shows no key rather than the same one.
        let mut keys = keymap::Keymap::default();
        keys.assign(Key::Clear, Chord::parse("cmd-x").unwrap());
        assert!(shown(&bindings_for(&keys), &Cut).is_empty());
        assert_eq!(shown(&bindings_for(&keys), &Copy), ["cmd-c"]);
    }

    #[test]
    fn without_a_command_key_only_quit_is_bound() {
        let bindings = bindings(&keymap::Keymap::default(), false);
        assert_eq!(bindings.len(), 1);
        assert_eq!(shown(&bindings, &Quit), ["cmd-q"]);
    }
}
