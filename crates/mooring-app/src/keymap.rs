//! Shortcuts: which ⌘ chord runs which workbench or terminal action. The ones changed
//! from the defaults live in `~/.config/mooring/keys.json`; a chord without ⌘ always
//! goes to the program in the terminal (Keepline repo,
//! specs/terminal-workbench/keyboard-shortcuts.md, not moved here).
use gpui::{App, Global, Keystroke};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    NewTab,
    Close,
    SplitRight,
    SplitDown,
    PrevTab,
    NextTab,
    PrevPane,
    NextPane,
    /// Input to the pane on that side, by where panes sit (Ghostty's goto_split).
    PaneLeft,
    PaneRight,
    PaneUp,
    PaneDown,
    /// The nth tab of the space, 1 to 8: fixed on ⌘1–⌘8, not rebindable.
    Tab(u8),
    /// Fixed on ⌘9, like the browsers and terminal apps.
    LastTab,
    Search,
    Settings,
    /// The next session that waits on you, as the top bar's chip.
    NextWaiting,
    /// Fold the left column to a rail and back.
    ToggleSidebar,
    Find,
    Clear,
    SelectAll,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// Scrollback, as Ghostty's scroll_to_top / scroll_to_bottom / scroll_page_*.
    ScrollTop,
    ScrollBottom,
    PageUp,
    PageDown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Tabs,
    Terminal,
    Workbench,
}
impl Section {
    pub const ALL: [Self; 3] = [Self::Tabs, Self::Terminal, Self::Workbench];
    pub fn label(self) -> &'static str {
        match self {
            Self::Tabs => t!("标签与分屏"),
            Self::Terminal => t!("终端"),
            Self::Workbench => t!("工作台"),
        }
    }
}
impl Action {
    pub const ALL: [Self; 35] = [
        Self::NewTab,
        Self::Close,
        Self::SplitRight,
        Self::SplitDown,
        Self::PrevTab,
        Self::NextTab,
        Self::PrevPane,
        Self::NextPane,
        Self::PaneLeft,
        Self::PaneRight,
        Self::PaneUp,
        Self::PaneDown,
        Self::Tab(1),
        Self::Tab(2),
        Self::Tab(3),
        Self::Tab(4),
        Self::Tab(5),
        Self::Tab(6),
        Self::Tab(7),
        Self::Tab(8),
        Self::LastTab,
        Self::Find,
        Self::Clear,
        Self::SelectAll,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ZoomReset,
        Self::ScrollTop,
        Self::ScrollBottom,
        Self::PageUp,
        Self::PageDown,
        Self::Search,
        Self::Settings,
        Self::NextWaiting,
        Self::ToggleSidebar,
    ];
    pub fn name(self) -> String {
        match self {
            Self::NewTab => "new_tab".into(),
            Self::Close => "close".into(),
            Self::SplitRight => "split_right".into(),
            Self::SplitDown => "split_down".into(),
            Self::PrevTab => "prev_tab".into(),
            Self::NextTab => "next_tab".into(),
            Self::PrevPane => "prev_pane".into(),
            Self::NextPane => "next_pane".into(),
            Self::PaneLeft => "pane_left".into(),
            Self::PaneRight => "pane_right".into(),
            Self::PaneUp => "pane_up".into(),
            Self::PaneDown => "pane_down".into(),
            Self::Tab(n) => format!("tab_{n}"),
            Self::LastTab => "last_tab".into(),
            Self::Search => "search".into(),
            Self::Settings => "settings".into(),
            Self::NextWaiting => "next_waiting".into(),
            Self::ToggleSidebar => "toggle_sidebar".into(),
            Self::Find => "find".into(),
            Self::Clear => "clear".into(),
            Self::SelectAll => "select_all".into(),
            Self::ZoomIn => "zoom_in".into(),
            Self::ZoomOut => "zoom_out".into(),
            Self::ZoomReset => "zoom_reset".into(),
            Self::ScrollTop => "scroll_to_top".into(),
            Self::ScrollBottom => "scroll_to_bottom".into(),
            Self::PageUp => "scroll_page_up".into(),
            Self::PageDown => "scroll_page_down".into(),
        }
    }
    pub fn label(self) -> String {
        match self {
            Self::NewTab => t!("新建标签").into(),
            Self::Close => t!("关闭画面").into(),
            Self::SplitRight => t!("向右分屏").into(),
            Self::SplitDown => t!("向下分屏").into(),
            Self::PrevTab => t!("上一个标签").into(),
            Self::NextTab => t!("下一个标签").into(),
            Self::PrevPane => t!("上一个画面").into(),
            Self::NextPane => t!("下一个画面").into(),
            Self::PaneLeft => t!("左边的画面").into(),
            Self::PaneRight => t!("右边的画面").into(),
            Self::PaneUp => t!("上面的画面").into(),
            Self::PaneDown => t!("下面的画面").into(),
            Self::Tab(n) => t!("第 {n} 个标签", n),
            Self::LastTab => t!("最后一个标签").into(),
            Self::Search => t!("搜索").into(),
            Self::Settings => t!("设置").into(),
            Self::NextWaiting => t!("下一个等你的").into(),
            Self::ToggleSidebar => t!("收起 / 展开左栏").into(),
            Self::Find => t!("在终端里查找").into(),
            Self::Clear => t!("清屏").into(),
            Self::SelectAll => t!("全选").into(),
            Self::ZoomIn => t!("放大字号").into(),
            Self::ZoomOut => t!("缩小字号").into(),
            Self::ZoomReset => t!("还原字号").into(),
            Self::ScrollTop => t!("滚到最上面").into(),
            Self::ScrollBottom => t!("回到底部").into(),
            Self::PageUp => t!("向上翻一页").into(),
            Self::PageDown => t!("向下翻一页").into(),
        }
    }
    pub fn section(self) -> Section {
        match self {
            Self::Find | Self::Clear | Self::SelectAll => Section::Terminal,
            Self::ZoomIn | Self::ZoomOut | Self::ZoomReset => Section::Terminal,
            Self::ScrollTop | Self::ScrollBottom | Self::PageUp | Self::PageDown => {
                Section::Terminal
            }
            Self::Search | Self::Settings | Self::NextWaiting | Self::ToggleSidebar => {
                Section::Workbench
            }
            _ => Section::Tabs,
        }
    }
    /// Run by the focused terminal itself; the rest are the workbench's.
    pub fn in_terminal(self) -> bool {
        self.section() == Section::Terminal
    }
    /// ⌘1–⌘9 always pick a tab: settings shows them as one read-only row.
    pub fn fixed(self) -> bool {
        matches!(self, Self::Tab(_) | Self::LastTab)
    }
    fn defaults(self) -> &'static [&'static str] {
        match self {
            Self::NewTab => &["cmd-t"],
            Self::Close => &["cmd-w"],
            Self::SplitRight => &["cmd-d"],
            Self::SplitDown => &["cmd-shift-d"],
            Self::PrevTab => &["cmd-shift-["],
            Self::NextTab => &["cmd-shift-]"],
            Self::PrevPane => &["cmd-["],
            Self::NextPane => &["cmd-]"],
            Self::PaneLeft => &["cmd-alt-left"],
            Self::PaneRight => &["cmd-alt-right"],
            Self::PaneUp => &["cmd-alt-up"],
            Self::PaneDown => &["cmd-alt-down"],
            Self::Tab(1) => &["cmd-1"],
            Self::Tab(2) => &["cmd-2"],
            Self::Tab(3) => &["cmd-3"],
            Self::Tab(4) => &["cmd-4"],
            Self::Tab(5) => &["cmd-5"],
            Self::Tab(6) => &["cmd-6"],
            Self::Tab(7) => &["cmd-7"],
            Self::Tab(_) => &["cmd-8"],
            Self::LastTab => &["cmd-9"],
            Self::Search => &["cmd-p"],
            Self::Settings => &["cmd-,"],
            Self::NextWaiting => &["cmd-j"],
            Self::ToggleSidebar => &["cmd-b"],
            Self::Find => &["cmd-f"],
            Self::Clear => &["cmd-k"],
            Self::SelectAll => &["cmd-a"],
            Self::ZoomIn => &["cmd-=", "cmd-shift-="],
            Self::ZoomOut => &["cmd--"],
            Self::ZoomReset => &["cmd-0"],
            Self::ScrollTop => &["cmd-home"],
            Self::ScrollBottom => &["cmd-end"],
            Self::PageUp => &["cmd-pageup"],
            Self::PageDown => &["cmd-pagedown"],
        }
    }
    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name)
    }
}

/// One key with its modifiers; shifted symbols are kept as their base key plus ⇧.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}
const SHIFTED: [(&str, &str); 11] = [
    ("{", "["),
    ("}", "]"),
    ("+", "="),
    ("_", "-"),
    ("<", ","),
    (">", "."),
    ("?", "/"),
    (":", ";"),
    ("\"", "'"),
    ("|", "\\"),
    ("~", "`"),
];
const MODIFIER_KEYS: [&str; 7] = [
    "shift", "control", "alt", "platform", "cmd", "function", "fn",
];
/// Whether the keys held say "command". Ctrl alone stays the terminal's.
pub fn command_key(m: &gpui::Modifiers) -> bool {
    command_key_where(m, mooring_platform::desktop::HAS_COMMAND_KEY)
}
fn command_key_where(m: &gpui::Modifiers, own_key: bool) -> bool {
    if own_key {
        m.platform
    } else {
        m.control && m.shift
    }
}

impl Chord {
    pub fn from_keystroke(k: &Keystroke) -> Option<Self> {
        Self::from_keystroke_where(k, mooring_platform::desktop::HAS_COMMAND_KEY)
    }
    /// A chord is written the same everywhere, in terms of ⌘. Without that key, command is
    /// Ctrl+Shift, with Alt added for ⌘⇧ since Shift is taken, and Ctrl+Alt is ⌘⌥.
    fn from_keystroke_where(k: &Keystroke, own_key: bool) -> Option<Self> {
        if MODIFIER_KEYS.contains(&k.key.as_str()) || k.key.is_empty() {
            return None;
        }
        let m = &k.modifiers;
        let mut chord = Self {
            cmd: m.platform,
            ctrl: m.control,
            alt: m.alt,
            shift: m.shift,
            key: k.key.to_lowercase(),
        };
        if let Some((_, base)) = SHIFTED.iter().find(|(s, _)| *s == chord.key) {
            chord.key = (*base).into();
            chord.shift = true;
        }
        if !own_key {
            chord = match (chord.ctrl, chord.shift, chord.alt) {
                (true, true, false) => Self {
                    cmd: true,
                    ctrl: false,
                    shift: false,
                    ..chord
                },
                (true, true, true) => Self {
                    cmd: true,
                    ctrl: false,
                    alt: false,
                    ..chord
                },
                (true, false, true) => Self {
                    cmd: true,
                    ctrl: false,
                    ..chord
                },
                _ => chord,
            };
        }
        Some(chord)
    }
    /// `cmd-shift-t`, as gpui writes keystrokes; the key itself may be `-`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut chord = Self {
            cmd: false,
            ctrl: false,
            alt: false,
            shift: false,
            key: String::new(),
        };
        let mut rest = text.trim();
        while let Some((head, tail)) = rest.split_once('-').filter(|(_, t)| !t.is_empty()) {
            match head {
                "cmd" => chord.cmd = true,
                "ctrl" => chord.ctrl = true,
                "alt" => chord.alt = true,
                "shift" => chord.shift = true,
                _ => return None,
            }
            rest = tail;
        }
        if rest.is_empty() || (rest.contains('-') && rest != "-") || MODIFIER_KEYS.contains(&rest) {
            return None;
        }
        let mut keystroke = Keystroke::parse("a").ok()?;
        keystroke.key = rest.into();
        let base = Self::from_keystroke(&keystroke)?;
        chord.shift |= base.shift;
        chord.key = base.key;
        Some(chord)
    }
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (on, name) in [
            (self.cmd, "cmd-"),
            (self.ctrl, "ctrl-"),
            (self.alt, "alt-"),
            (self.shift, "shift-"),
        ] {
            if on {
                out.push_str(name);
            }
        }
        out + &self.key
    }
    pub fn display(&self) -> String {
        if mooring_platform::desktop::HAS_COMMAND_KEY {
            self.display_with_signs()
        } else {
            self.display_in_words()
        }
    }
    /// Ctrl+Shift+T, the keys to actually press where there is no ⌘.
    pub(crate) fn display_in_words(&self) -> String {
        let (ctrl, shift, alt) = match (self.cmd, self.shift, self.alt) {
            (true, true, _) => (true, true, true),
            (true, false, true) => (true, false, true),
            (true, false, false) => (true, true, false),
            (false, shift, alt) => (self.ctrl, shift, alt),
        };
        let mut out = String::new();
        for (on, word) in [(ctrl, "Ctrl+"), (shift, "Shift+"), (alt, "Alt+")] {
            if on {
                out.push_str(word);
            }
        }
        let key = match self.key.as_str() {
            "space" => t!("空格").into(),
            "enter" => "Enter".into(),
            "tab" => "Tab".into(),
            "backspace" => "Backspace".into(),
            "escape" => "Esc".into(),
            "left" => "←".into(),
            "right" => "→".into(),
            "up" => "↑".into(),
            "down" => "↓".into(),
            "home" => "Home".into(),
            "end" => "End".into(),
            "pageup" => "PageUp".into(),
            "pagedown" => "PageDown".into(),
            key => key.to_uppercase(),
        };
        out + &key
    }
    /// ⌃⌥⇧⌘T, in the order macOS menus use.
    fn display_with_signs(&self) -> String {
        let mut out = String::new();
        for (on, sign) in [
            (self.ctrl, "⌃"),
            (self.alt, "⌥"),
            (self.shift, "⇧"),
            (self.cmd, "⌘"),
        ] {
            if on {
                out.push_str(sign);
            }
        }
        let key = match self.key.as_str() {
            "space" => t!("空格").into(),
            "enter" => "↩".into(),
            "tab" => "⇥".into(),
            "backspace" => "⌫".into(),
            "escape" => "⎋".into(),
            "left" => "←".into(),
            "right" => "→".into(),
            "up" => "↑".into(),
            "down" => "↓".into(),
            "home" => "↖".into(),
            "end" => "↘".into(),
            "pageup" => "⇞".into(),
            "pagedown" => "⇟".into(),
            key => key.to_uppercase(),
        };
        out + &key
    }
    /// ⌘1–⌘9 alone, which always pick a tab.
    fn is_tab_number(&self) -> bool {
        self.cmd
            && !self.ctrl
            && !self.alt
            && !self.shift
            && matches!(self.key.as_bytes(), [b'1'..=b'9'])
    }
    /// Why this chord cannot be bound, if it cannot.
    pub fn refusal(&self) -> Option<&'static str> {
        if !self.cmd {
            return Some(t!("快捷键需要包含 ⌘；其他组合键留给终端里的程序"));
        }
        if !self.ctrl && !self.alt && !self.shift && matches!(self.key.as_str(), "c" | "v" | "q") {
            return Some(t!("⌘C、⌘V、⌘Q 保留给复制、粘贴和退出"));
        }
        if self.key == "escape" {
            return Some(t!("Esc 用来取消录制"));
        }
        if self.is_tab_number() {
            return Some(t!("⌘1–⌘9 固定用于切换标签"));
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<(Action, Vec<Chord>)>,
}
impl Global for Keymap {}
impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: Action::ALL
                .into_iter()
                .map(|a| {
                    (
                        a,
                        a.defaults()
                            .iter()
                            .filter_map(|t| Chord::parse(t))
                            .collect(),
                    )
                })
                .collect(),
        }
    }
}
impl Keymap {
    /// Every rebindable action, in the order the settings page lists them.
    pub fn actions() -> impl Iterator<Item = Action> {
        Action::ALL.into_iter().filter(|a| !a.fixed())
    }
    pub fn action_for(&self, k: &Keystroke) -> Option<Action> {
        let chord = Chord::from_keystroke(k)?;
        self.bindings
            .iter()
            .find(|(_, chords)| chords.contains(&chord))
            .map(|(a, _)| *a)
    }
    pub fn chords(&self, action: Action) -> &[Chord] {
        self.bindings
            .iter()
            .find(|(a, _)| *a == action)
            .map_or(&[], |(_, c)| c.as_slice())
    }
    pub fn is_default(&self, action: Action) -> bool {
        self.chords(action) == Self::default().chords(action)
    }
    /// The other action already on `chord`, if any.
    pub fn holder(&self, action: Action, chord: &Chord) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(a, c)| *a != action && c.contains(chord))
            .map(|(a, _)| *a)
    }
    /// Puts `chord` on `action` alone, taking it from whichever action had it.
    pub fn assign(&mut self, action: Action, chord: Chord) {
        for (a, chords) in &mut self.bindings {
            if *a == action {
                *chords = vec![chord.clone()];
            } else {
                chords.retain(|c| *c != chord);
            }
        }
    }
    /// Back to the default chords; one taken by another action is taken back too.
    pub fn reset(&mut self, action: Action) {
        let defaults = Self::default().chords(action).to_vec();
        for (a, chords) in &mut self.bindings {
            if *a == action {
                *chords = defaults.clone();
            } else {
                chords.retain(|c| !defaults.contains(c));
            }
        }
    }
    /// The changed actions only, `{}` when nothing differs.
    pub fn to_json(&self) -> String {
        let changed: serde_json::Map<_, _> = self
            .bindings
            .iter()
            .filter(|(a, _)| !a.fixed() && !self.is_default(*a))
            .map(|(a, chords)| {
                let list = chords.iter().map(|c| c.text().into()).collect();
                (a.name(), serde_json::Value::Array(list))
            })
            .collect();
        serde_json::to_string_pretty(&changed).unwrap_or_else(|_| "{}".into())
    }
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let object = value
            .as_object()
            .ok_or("expected an object of action: [chords]")?;
        let mut map = Self::default();
        // Entries an older Mooring wrote for the tab numbers, or that move ⌘1–⌘9, are
        // skipped: those chords are fixed now, and the rest of the file still applies.
        let mut named = vec![];
        'entries: for (name, list) in object {
            let action = Action::parse(name).ok_or_else(|| format!("unknown action \"{name}\""))?;
            let list = list
                .as_array()
                .ok_or_else(|| format!("\"{name}\": expected a list of chords"))?;
            let mut chords = vec![];
            for item in list {
                let chord = item
                    .as_str()
                    .and_then(Chord::parse)
                    .ok_or_else(|| format!("\"{name}\": bad chord {item}"))?;
                if chord.is_tab_number() {
                    continue 'entries;
                }
                if let Some(why) = chord.refusal() {
                    return Err(format!("\"{name}\": {}: {why}", chord.text()));
                }
                chords.push(chord);
            }
            if action.fixed() {
                continue;
            }
            if let Some((_, c)) = map.bindings.iter_mut().find(|(a, _)| *a == action) {
                *c = chords;
            }
            named.push(action);
        }
        // A file only names what changed; a default it now uses elsewhere moves with it.
        let claimed: Vec<Chord> = map
            .bindings
            .iter()
            .filter(|(a, _)| named.contains(a))
            .flat_map(|(_, c)| c.clone())
            .collect();
        for (a, chords) in &mut map.bindings {
            if !named.contains(a) {
                chords.retain(|c| !claimed.contains(c));
            }
        }
        let mut seen: Vec<&Chord> = vec![];
        for (a, chords) in &map.bindings {
            for c in chords {
                if seen.contains(&c) {
                    return Err(format!(
                        "{} is bound twice (again on \"{}\")",
                        c.text(),
                        a.name()
                    ));
                }
                seen.push(c);
            }
        }
        Ok(map)
    }
    pub fn load() -> Self {
        let Some(path) = path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|e| {
                eprintln!(
                    "mooring: {}: {e}; using the default shortcuts",
                    path.display()
                );
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }
    fn save(&self) {
        let Some(path) = path() else {
            return;
        };
        let json = self.to_json();
        let written = if json == "{}" {
            std::fs::remove_file(&path).or_else(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(e)
                }
            })
        } else {
            path.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                // Written beside and renamed over: a crash mid-write must not leave a
                // half file that loads as the defaults and is then saved over yours.
                .and_then(|()| {
                    let tmp = path.with_extension("json.tmp");
                    std::fs::write(&tmp, json).and_then(|()| std::fs::rename(&tmp, &path))
                })
        };
        if let Err(e) = written {
            eprintln!("mooring: saving {}: {e}", path.display());
        }
    }
}
/// The action bound to this keystroke in the current keymap.
pub fn action(k: &Keystroke, cx: &App) -> Option<Action> {
    match cx.try_global::<Keymap>() {
        Some(map) => map.action_for(k),
        None => Keymap::default().action_for(k),
    }
}
/// The key an action is on now, as a hint shows it (`⌘P`); empty when you unbound it.
/// Every hint that names a shortcut reads this, so a rebind in keys.json shows
/// everywhere at once.
pub fn keycap(cx: &App, action: Action) -> String {
    current(cx)
        .chords(action)
        .first()
        .map(|c| c.display())
        .unwrap_or_default()
}
/// The current keymap, or the defaults before one is loaded.
pub fn current(cx: &App) -> Keymap {
    cx.try_global::<Keymap>().cloned().unwrap_or_default()
}
/// Saves the changed keymap and makes it the one every pane reads.
pub fn update(map: Keymap, cx: &mut App) {
    map.save();
    cx.set_global(map);
    cx.refresh_windows();
}
pub fn path() -> Option<std::path::PathBuf> {
    crate::ui::status_colors::config_dir().map(|dir| dir.join("keys.json"))
}

#[cfg(test)]
#[path = "keymap_tests.rs"]
mod tests;
