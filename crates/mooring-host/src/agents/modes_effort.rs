//! How hard the Agent thinks, read off real runs (Claude Code 2.1.319, Codex, 11-09).
//!
//! Claude Code: `/effort` is a slider, `▲` over one of low · medium · high · xhigh · max; ←/→
//! move it, `s` sets it for this session only and it says `Set effort level to high (this
//! session only)`. Its footer's `● high · /effort` comes and goes, so what is on is read off the
//! slider. Codex: the second step of `/model` for the model in use, `(current)` on the one on,
//! with Max and Ultra one menu further under `More reasoning…`; `s` for this session, and its
//! footer then says `GPT-6.1-Sol xhigh`.
use super::{read, Changed, Steps, NOT_FOUND};
use crate::agents::modes::Menu;
use mooring_core::modes::{ModeOption, ModeOptions};
use std::io;
use std::time::Duration;

pub(crate) const CLAUDE_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
const CLAUDE_HINT: &str = "←/→ to adjust";
const CODEX_MORE: &str = "More reasoning…";
const CODEX_ADVANCED: &str = "Advanced Reasoning";
const ONLY_SESSION: &str = "只对这次会话有效。";

/// The level Claude Code's slider points at, `None` when the slider is not showing.
pub(crate) fn claude_slider(tail: &str) -> Option<&'static str> {
    let lines: Vec<&str> = tail.lines().collect();
    let hint = lines.iter().rposition(|l| l.contains(CLAUDE_HINT))?;
    let labels = lines[..hint].iter().rposition(|l| {
        let w: Vec<&str> = l.split_whitespace().collect();
        w.windows(5).any(|x| x == CLAUDE_LEVELS)
    })?;
    let arrow = lines[..labels].iter().rposition(|l| l.contains('▲'))?;
    let at = lines[arrow].chars().position(|c| c == '▲')?;
    let row: Vec<char> = lines[labels].chars().collect();
    // Each label's middle column; the arrow stands over the nearest.
    let mut from = 0;
    let mut best: Option<(usize, &'static str)> = None;
    for level in CLAUDE_LEVELS {
        let word: Vec<char> = level.chars().collect();
        let start = (from..row.len()).find(|&i| row[i..].starts_with(&word))?;
        from = start + word.len();
        let middle = start + word.len() / 2;
        let far = middle.abs_diff(at);
        if best.is_none_or(|(d, _)| far < d) {
            best = Some((far, level));
        }
    }
    best.map(|(_, l)| l)
}

/// What Claude Code said it set: `Set effort level to high (this session only)`.
pub(crate) fn claude_effort_set(tail: &str) -> Option<String> {
    tail.lines().rev().find_map(|l| {
        let l = l.trim_start_matches(['⎿', ' ']).trim();
        let rest = l.strip_prefix("Set effort level to ")?;
        let level = rest.split([' ', ':']).next()?;
        Some(level.to_owned())
    })
}

fn claude_detail(level: &str) -> &'static str {
    match level {
        "low" => "最快，想得最少",
        "medium" => "快和深之间",
        "high" => "想得更深，用量更多",
        "xhigh" => "比 high 再深一些",
        "max" => "最深，很费 token，只留给最难的事",
        _ => "",
    }
}

/// A Codex row's level without its `(default)`: the default moves from model to model.
pub(crate) fn codex_level(label: &str) -> &str {
    label.strip_suffix(" (default)").unwrap_or(label)
}

/// The word Codex's footer uses for a level: `GPT-6.1-Sol xhigh`.
pub(crate) fn codex_word(level: &str) -> String {
    match level {
        "Extra high" => "xhigh".into(),
        other => other.to_ascii_lowercase(),
    }
}

fn codex_options_of(menus: &[&Menu], said: Option<&str>) -> Vec<ModeOption> {
    let mut out: Vec<ModeOption> = vec![];
    for m in menus {
        for r in m.rows.iter().filter(|r| r.label != CODEX_MORE) {
            let key = codex_level(&r.label).to_owned();
            if out.iter().any(|o| o.key == key) {
                continue;
            }
            out.push(ModeOption {
                current: r.current,
                label: key.clone(),
                detail: r.detail.clone(),
                full: false,
                key,
            });
        }
    }
    // A menu that marks none: what its footer says.
    if !out.iter().any(|o| o.current) {
        if let Some(said) = said {
            for o in &mut out {
                o.current = codex_word(&o.key) == said;
            }
        }
    }
    out
}

/// The last word of Codex's footer model words: its level.
fn codex_footer_level(tail: &str) -> Option<String> {
    let words = read::codex_footer(tail)?;
    words.split_whitespace().last().map(str::to_owned)
}

impl Steps<'_> {
    /// Open, or closed again when it says why not.
    fn claude_slider_open(&self) -> io::Result<&'static str> {
        self.run("/effort")?;
        match self.wait(Duration::from_secs(3), claude_slider)? {
            Some(on) => Ok(on),
            None => {
                self.close(1)?;
                Err(io::Error::other("强度没有打开，请打开终端看一眼"))
            }
        }
    }

    pub(super) fn claude_effort_options(&self) -> io::Result<ModeOptions> {
        let on = self.claude_slider_open()?;
        self.close(1)?;
        Ok(ModeOptions {
            options: CLAUDE_LEVELS
                .iter()
                .map(|l| ModeOption {
                    key: (*l).to_owned(),
                    label: (*l).to_owned(),
                    detail: claude_detail(l).to_owned(),
                    current: *l == on,
                    full: false,
                })
                .collect(),
            note: ONLY_SESSION.into(),
        })
    }

    pub(super) fn set_claude_effort(&self, choice: &str) -> io::Result<Changed> {
        let Some(goal) = CLAUDE_LEVELS.iter().position(|l| *l == choice) else {
            return Err(io::Error::other(NOT_FOUND));
        };
        let mut on = self.claude_slider_open()?;
        for _ in 0..CLAUDE_LEVELS.len() {
            let at = CLAUDE_LEVELS.iter().position(|l| *l == on).unwrap_or(0);
            if at == goal {
                break;
            }
            self.keys(&[if at < goal { "right" } else { "left" }])?;
            let before = on;
            let moved = self.wait(Duration::from_millis(900), |t| {
                claude_slider(t).filter(|l| *l != before)
            })?;
            let Some(moved) = moved else {
                self.close(1)?;
                return Err(io::Error::other("按了，但滑杆没动，没有换"));
            };
            on = moved;
        }
        if on != choice {
            self.close(1)?;
            return Err(io::Error::other(format!("没移到 {choice}，没有换")));
        }
        let hint = self.screen()?;
        if !(hint.lines().rev())
            .find(|l| l.contains(CLAUDE_HINT))
            .is_some_and(read::offers_session)
        {
            self.close(1)?;
            return Err(io::Error::other(
                "强度没有只改这次会话的选项，没有换，请在终端里换",
            ));
        }
        self.keys(&["s"])?;
        let said = self.wait(Duration::from_secs(3), |t| {
            claude_effort_set(t)
                .filter(|l| l == choice)
                .filter(|_| read::composer(self.agent, t).is_some())
        })?;
        let Some(now) = said else {
            return Err(io::Error::other("按了，但画面上没说换好，请打开终端看一眼"));
        };
        Ok(Changed {
            effort: Some(now),
            ..Default::default()
        })
    }

    /// `/model`, Enter on the model in use: its levels' menu, one level of menus deep.
    fn codex_levels_open(&self) -> io::Result<Menu> {
        let title = self.model_title();
        let models = self.open(super::MODEL_MENU.0, title)?;
        let Some(using) = models
            .rows
            .iter()
            .find(|r| r.current)
            .map(|r| r.label.clone())
        else {
            self.close(1)?;
            return Err(io::Error::other("读不出现在用的模型"));
        };
        self.point(title, &models, &using)?;
        self.keys(&["enter"])?;
        let levels = self.wait(Duration::from_secs(2), |t| {
            read::menu(t, super::EFFORT_TITLE)
        })?;
        match levels {
            Some(m) => Ok(m),
            None => {
                self.close(2)?;
                Err(io::Error::other("没看到选档位的菜单，请在终端里换"))
            }
        }
    }

    /// From the levels' menu into Max and Ultra's. When it says why not, only the model menu
    /// is still open.
    fn codex_advanced_open(&self, levels: &Menu) -> io::Result<Menu> {
        self.point(super::EFFORT_TITLE, levels, CODEX_MORE)?;
        self.keys(&["enter"])?;
        match self.wait(Duration::from_secs(2), |t| read::menu(t, CODEX_ADVANCED))? {
            Some(m) => Ok(m),
            None => {
                self.close(1)?;
                Err(io::Error::other("没看到更多档位的菜单，请在终端里换"))
            }
        }
    }

    pub(super) fn codex_effort_options(&self) -> io::Result<ModeOptions> {
        let said = codex_footer_level(&self.screen()?);
        let levels = self.codex_levels_open()?;
        let advanced = if levels.rows.iter().any(|r| r.label == CODEX_MORE) {
            match self.codex_advanced_open(&levels) {
                Ok(m) => {
                    self.close(3)?;
                    Some(m)
                }
                Err(_) => {
                    self.close(1)?;
                    None
                }
            }
        } else {
            self.close(2)?;
            None
        };
        let menus: Vec<&Menu> = std::iter::once(&levels).chain(advanced.as_ref()).collect();
        Ok(ModeOptions {
            options: codex_options_of(&menus, said.as_deref()),
            note: ONLY_SESSION.into(),
        })
    }

    pub(super) fn set_codex_effort(&self, choice: &str) -> io::Result<Changed> {
        let levels = self.codex_levels_open()?;
        let row = |m: &Menu| {
            (m.rows.iter())
                .find(|r| codex_level(&r.label) == choice && r.label != CODEX_MORE)
                .map(|r| r.label.clone())
        };
        let (title, menu, label, deep) = match row(&levels) {
            Some(label) => (super::EFFORT_TITLE, levels, label, 2),
            None if levels.rows.iter().any(|r| r.label == CODEX_MORE) => {
                let more = self.codex_advanced_open(&levels).inspect_err(|_| {
                    let _ = self.close(1);
                })?;
                let Some(label) = row(&more) else {
                    self.close(3)?;
                    return Err(io::Error::other(NOT_FOUND));
                };
                (CODEX_ADVANCED, more, label, 3)
            }
            None => {
                self.close(2)?;
                return Err(io::Error::other(NOT_FOUND));
            }
        };
        // `point` closes the menu it gave up on; the ones above it are left.
        let menu = self.point(title, &menu, &label).inspect_err(|_| {
            let _ = self.close(deep - 1);
        })?;
        if !read::offers_session(&menu.keys) {
            self.close(deep)?;
            return Err(io::Error::other(
                "菜单没有只改这次会话的选项，没有换，请在终端里换",
            ));
        }
        self.keys(&["s"])?;
        let word = codex_word(choice);
        let took = self.wait(Duration::from_secs(3), |t| {
            codex_footer_level(t).filter(|w| *w == word)
        })?;
        if took.is_none() {
            let saw = read::codex_footer(&self.screen()?).unwrap_or_default();
            return Err(io::Error::other(format!("按了，但它现在写的是「{saw}」")));
        }
        Ok(Changed {
            effort: Some(choice.to_owned()),
            ..Default::default()
        })
    }
}

#[cfg(test)]
#[path = "modes_effort_tests.rs"]
mod tests;
