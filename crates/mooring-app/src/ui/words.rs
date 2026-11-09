//! Labels are written once, the way a Mac shows them: `⌘P`, `⌘ 点击`, 访达. These say them in
//! this system's words, so no label has to know which system it is on.
use crate::keymap::Chord;
use mooring_core::i18n::{self, Lang};
use mooring_platform::desktop::{FILE_MANAGER, FILE_MANAGER_EN, HAS_COMMAND_KEY};

/// A label naming workbench shortcuts. `⌘P` is the command key and a letter (Ctrl+Shift+P
/// without one), `⌘ 点击` a click with the system's own modifier (Ctrl+点击).
pub fn local(text: &str) -> String {
    say(text, Where::Workbench, HAS_COMMAND_KEY, file_manager())
}

/// A label for a field, a form or a dialog, where nothing runs in a terminal and ⌘ is the
/// system's own modifier: `⌘⏎` is Ctrl+Enter without a command key.
pub fn local_field(text: &str) -> String {
    say(text, Where::Field, HAS_COMMAND_KEY, file_manager())
}

/// The file manager's name in the window's language; a label says 访达 or Finder.
fn file_manager() -> &'static str {
    match i18n::lang() {
        Lang::Zh => FILE_MANAGER,
        Lang::En => FILE_MANAGER_EN,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Where {
    Workbench,
    Field,
}

fn say(text: &str, at: Where, own_key: bool, file_manager: &str) -> String {
    let text = text
        .replace("访达", file_manager)
        .replace("Finder", file_manager);
    if own_key {
        return text;
    }
    let mut out = String::with_capacity(text.len() + 16);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if !is_sign(c) {
            out.push(c);
            continue;
        }
        let mut chord = Chord {
            cmd: false,
            ctrl: false,
            alt: false,
            shift: false,
            key: String::new(),
        };
        mark(&mut chord, c);
        while let Some(&next) = chars.peek().filter(|n| is_sign(**n)) {
            mark(&mut chord, next);
            chars.next();
        }
        let clicked = chars.peek().is_none_or(|n| n.is_whitespace());
        // In a field, or with the mouse, ⌘ is the system's main modifier; on the keyboard
        // of the workbench it is the command chord the keymap reads.
        if clicked || at == Where::Field {
            chord.ctrl |= chord.cmd;
            chord.cmd = false;
        }
        match chars.peek().and_then(|k| key_name(*k)) {
            Some(key) => {
                chars.next();
                chord.key = key;
                out.push_str(&chord.display_in_words());
            }
            None => {
                chord.key = "a".into();
                let words = chord.display_in_words();
                out.push_str(words.trim_end_matches("+A"));
            }
        }
    }
    out
}

fn is_sign(c: char) -> bool {
    matches!(c, '⌘' | '⌃' | '⌥' | '⇧')
}

fn mark(chord: &mut Chord, sign: char) {
    match sign {
        '⌘' => chord.cmd = true,
        '⌃' => chord.ctrl = true,
        '⌥' => chord.alt = true,
        _ => chord.shift = true,
    }
}

/// The key a sign is written before, as a keystroke names it; `None` when what follows is
/// words, so the signs stood alone.
fn key_name(c: char) -> Option<String> {
    Some(match c {
        '⏎' | '↵' | '↩' => "enter".into(),
        '←' => "left".into(),
        '→' => "right".into(),
        '↑' => "up".into(),
        '↓' => "down".into(),
        '−' => "-".into(),
        // ⌘+ is how the zoom key is said; the key itself is =.
        '+' => "=".into(),
        c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase().to_string(),
        c if "[]=-,./\\;'`".contains(c) => c.to_string(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pc(text: &str) -> String {
        say(text, Where::Workbench, false, "资源管理器")
    }

    #[test]
    fn a_mac_keeps_every_label_as_written() {
        let text = "⌘ 点击在访达中打开 · ⌘⌥ 在访达中显示 · ⌘P";
        assert_eq!(say(text, Where::Workbench, true, "访达"), text);
        assert_eq!(say("⌘⏎", Where::Field, true, "访达"), "⌘⏎");
    }

    #[test]
    fn workbench_shortcuts_are_the_keys_to_press() {
        assert_eq!(pc("右键菜单和 ⌘P：3 个"), "右键菜单和 Ctrl+Shift+P：3 个");
        assert_eq!(pc("⌘1 – ⌘9"), "Ctrl+Shift+1 – Ctrl+Shift+9");
        assert_eq!(
            pc("⌘C、⌘V、⌘Q 保留"),
            "Ctrl+Shift+C、Ctrl+Shift+V、Ctrl+Shift+Q 保留"
        );
        assert_eq!(pc("与 ⌘+ / ⌘− 缩放"), "与 Ctrl+Shift+= / Ctrl+Shift+- 缩放");
        assert_eq!(pc("⌘⇧D"), "Ctrl+Shift+Alt+D");
        assert_eq!(pc("⌘⌥←"), "Ctrl+Alt+←");
        assert_eq!(pc("需包含 ⌘；Esc 取消"), "需包含 Ctrl+Shift；Esc 取消");
    }

    #[test]
    fn a_click_and_a_field_use_the_systems_own_modifier() {
        assert_eq!(pc("⌘ 点击在浏览器打开"), "Ctrl 点击在浏览器打开");
        assert_eq!(
            pc("⌘ 点击打开 · ⌘⌥ 在访达中显示"),
            "Ctrl 点击打开 · Ctrl+Alt 在资源管理器中显示"
        );
        assert_eq!(say("开始 ⌘↵", Where::Field, false, "x"), "开始 Ctrl+Enter");
        assert_eq!(say("复制 · ⌘C", Where::Field, false, "x"), "复制 · Ctrl+C");
    }

    #[test]
    fn the_file_manager_is_named_by_the_system() {
        assert_eq!(pc("在访达中显示"), "在资源管理器中显示");
        assert_eq!(
            say("无法打开访达", Where::Workbench, true, "访达"),
            "无法打开访达"
        );
    }
}
