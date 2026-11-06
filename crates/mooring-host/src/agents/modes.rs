//! How a model or permission change looks on each CLI's screen, read off real runs (Claude
//! Code 2.1.319, Codex 0.167, 11-06). Pure reading: `host/client/modes.rs` presses the keys.
//!
//! Claude Code: `/model` is a menu whose `s` picks for this session only; Shift+Tab cycles
//! manual, accept edits, plan, (bypass, if the session was started allowing it), auto, and the
//! footer says which. Codex: `/model`, then its effort, `s` for this session; `/permissions`
//! is a menu, and Full Access asks once more.
use mooring_core::model::Agent;

/// One row of a CLI's menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub number: u32,
    pub label: String,
    pub detail: String,
    pub pointer: bool,
    pub current: bool,
}

/// A menu on screen, by its title; `None` if that menu is not showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub rows: Vec<Row>,
    /// The hint line under it: which keys it takes.
    pub keys: String,
    /// It scrolls: more rows below those shown.
    pub more: bool,
}

const CURRENT: [&str; 2] = [" (current)", " ✔"];

fn row(line: &str) -> Option<Row> {
    let trimmed = line.trim_start();
    let (pointer, rest) = match trimmed.strip_prefix(['❯', '›']) {
        Some(r) => (true, r.trim_start()),
        None => (false, trimmed),
    };
    let dot = rest.find('.')?;
    let number: u32 = rest[..dot].parse().ok()?;
    let body = rest[dot + 1..].trim_start();
    // A label and its detail stand at least two spaces apart.
    let (mut label, detail) = match body.find("  ") {
        Some(at) => (&body[..at], body[at..].trim()),
        None => (body, ""),
    };
    let mut current = false;
    for mark in CURRENT {
        if let Some(l) = label.strip_suffix(mark) {
            label = l;
            current = true;
        }
    }
    (!label.is_empty()).then(|| Row {
        number,
        label: label.to_owned(),
        detail: detail.to_owned(),
        pointer,
        current,
    })
}

/// The last menu titled `title` on the screen, if nothing has come up below it.
pub fn menu(tail: &str, title: &str) -> Option<Menu> {
    let lines: Vec<&str> = tail.lines().collect();
    let top = lines.iter().rposition(|l| l.contains(title))?;
    let mut rows = vec![];
    let mut more = false;
    for l in &lines[top + 1..] {
        let t = l.trim();
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("enter ")
            || lower.contains(" · esc ")
            || lower.ends_with("esc to cancel")
        {
            return Some(Menu {
                rows,
                keys: t.to_owned(),
                more,
            });
        }
        more |= t.starts_with('↓');
        if let Some(r) = row(l) {
            rows.push(r);
        }
    }
    None
}

/// Whether a menu says which row a key goes to, and the row itself.
pub fn pointed(menu: &Menu) -> Option<&Row> {
    menu.rows.iter().find(|r| r.pointer)
}

/// The words `permission::codex`/`permission::claude` use for cards, for an option's key.
pub fn card_words(agent: Agent, key: &str) -> String {
    match agent {
        Agent::Claude => claude_words(key).to_owned(),
        _ => match key {
            "Ask for approval" => "逐个确认",
            "Approve for me" => "只问不安全的",
            "Full Access" => "全部放行",
            "Read Only" => "只读",
            other => other,
        }
        .to_owned(),
    }
}

/// Codex's `/permissions`: known ones are worded; anything new counts as full until read.
pub fn codex_is_full(label: &str) -> bool {
    !matches!(label, "Ask for approval" | "Approve for me" | "Read Only")
}

/// Claude Code's modes, in the order Shift+Tab goes, with what its footer says for each.
pub const CLAUDE_CYCLE: [(&str, &str); 5] = [
    ("manual", "manual mode on"),
    ("acceptEdits", "accept edits on"),
    ("plan", "plan mode on"),
    ("bypassPermissions", "bypass permissions on"),
    ("auto", "auto mode on"),
];
pub const CLAUDE_FULL: &str = "bypassPermissions";

fn claude_words(key: &str) -> &str {
    match key {
        "manual" | "default" => "逐个确认",
        "acceptEdits" => "自动接受编辑",
        "plan" => "只做计划",
        "bypassPermissions" => "全部放行",
        "auto" => "自动判断",
        other => other,
    }
}

/// What a card says for a Claude mode; a `key` is also what the host is asked to pick.
pub fn claude_option_detail(key: &str) -> &'static str {
    match key {
        "manual" => "跑命令和改文件前都问你",
        "acceptEdits" => "改文件不问，跑命令还问",
        "plan" => "只读和出方案，不动手",
        "auto" => "Claude 自己判断哪些要问",
        "bypassPermissions" => "跑命令、改文件都不再问你",
        _ => "",
    }
}

/// Which mode Claude Code's footer says, as a key. An old version shows nothing for manual.
pub fn claude_mode(tail: &str) -> Option<&'static str> {
    let bottom: Vec<&str> = tail
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(4)
        .collect();
    for (key, words) in CLAUDE_CYCLE {
        if bottom.iter().any(|l| l.contains(words)) {
            return Some(key);
        }
    }
    composer(Agent::Claude, tail).map(|_| "manual")
}

/// What Codex's footer says the model is: `GPT-6-Sol medium · /path`, the words before the dot.
pub fn codex_footer(tail: &str) -> Option<String> {
    let lines: Vec<&str> = tail.lines().filter(|l| !l.trim().is_empty()).collect();
    let bottom = &lines[lines.len().saturating_sub(3)..];
    bottom.iter().find_map(|l| {
        let (words, rest) = l.trim().split_once(" · ")?;
        rest.starts_with(['/', '~']).then(|| words.to_owned())
    })
}

/// What stands in the input box, `None` when the screen shows none: a menu or dialog over it,
/// or a CLI not at its prompt. The box's own faint hint reads as text here, which is why text
/// is checked by what it becomes, not assumed empty (`host/client/modes.rs`).
pub fn composer(agent: Agent, tail: &str) -> Option<String> {
    let filled: Vec<&str> = tail.lines().filter(|l| !l.trim().is_empty()).collect();
    let mark = match agent {
        Agent::Claude => '❯',
        _ => '›',
    };
    let at = filled.iter().rposition(|l| l.starts_with(mark))?;
    // A dialog's pointer also opens a row: what stands below it says which this is.
    let dialog = filled[at + 1..].iter().any(|l| {
        let l = l.trim().to_ascii_lowercase();
        l.starts_with("enter ") || l.contains("esc to cancel") || l.contains("esc back")
    });
    (!dialog).then(|| filled[at][mark.len_utf8()..].trim().to_owned())
}

/// Busy words a CLI writes while its turn runs, however the state has yet to say so.
pub fn working(tail: &str) -> bool {
    let filled: Vec<&str> = tail.lines().filter(|l| !l.trim().is_empty()).collect();
    filled[filled.len().saturating_sub(8)..]
        .iter()
        .any(|l| l.contains("esc to interrupt"))
}

/// What Claude Code says once `/model` took: its own words, `None` if it has not.
pub fn claude_model_set(tail: &str) -> Option<String> {
    tail.lines().rev().find_map(|l| {
        let l = l.trim_start_matches(['⎿', ' ']).trim();
        l.strip_prefix("Set model to ")
            .map(|r| r.trim_end_matches(" for this session only").to_owned())
    })
}

/// Whether what it says it set is the row picked: its `Default` row names the model behind it,
/// `Set model to Opus 5.5 (default)`.
pub fn claude_model_took(said: &str, choice: &str) -> bool {
    said == choice || (choice.starts_with("Default") && said.ends_with("(default)"))
}

/// Whether Claude Code asks `Switch model?` about `choice` with its yes under the pointer, as
/// it does once a conversation is cached on the model in use.
pub fn claude_switch_question(tail: &str, choice: &str) -> bool {
    let lines: Vec<&str> = tail.lines().filter(|l| !l.trim().is_empty()).collect();
    let bottom = &lines[lines.len().saturating_sub(10)..];
    let yes = format!("❯ 1. Yes, switch to {choice}");
    bottom.iter().any(|l| l.trim() == "Switch model?") && bottom.iter().any(|l| l.trim() == yes)
}

/// Which option a Claude session offers beyond the cycle every session has: a process started
/// with `--allow-dangerously-skip-permissions` or `--dangerously-skip-permissions`.
pub fn claude_offers_full(command: &str) -> bool {
    command.split_whitespace().any(|w| {
        w == "--dangerously-skip-permissions" || w == "--allow-dangerously-skip-permissions"
    }) || command.contains("--permission-mode bypassPermissions")
        || command.contains("--permission-mode=bypassPermissions")
}

/// Whether going Shift+Tab from `from` to `to` passes the bypass mode on the way.
pub fn claude_crosses_full(from: &str, to: &str) -> bool {
    let at = |k: &str| CLAUDE_CYCLE.iter().position(|(c, _)| *c == k);
    let (Some(mut i), Some(end), Some(full)) = (at(from), at(to), at(CLAUDE_FULL)) else {
        return false;
    };
    if end == full || i == full || i == end {
        return false;
    }
    loop {
        i = (i + 1) % CLAUDE_CYCLE.len();
        if i == end {
            return false;
        }
        if i == full {
            return true;
        }
    }
}

/// Whether a menu's hint offers picking for this session alone.
pub fn offers_session(keys: &str) -> bool {
    let k = keys.to_ascii_lowercase();
    k.contains("s to use this session") || k.contains("s session")
}

#[cfg(test)]
#[path = "modes_tests.rs"]
mod tests;
