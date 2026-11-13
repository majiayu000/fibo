//! Changing a model or permission mode on Claude Code and Codex, one key at a time, each checked
//! on the screen before the next. All of it stays under `WHOLE`: a client's request limit is
//! 10 s. The host says when it may start and keeps the result (`host/client/modes.rs`).
use super::modes::{self as read, Menu, Row};
use mooring_core::model::Agent;
use mooring_core::modes::{ModeKind, ModeOption, ModeOptions};
use std::io;
use std::thread;
use std::time::{Duration, Instant};

const WHOLE: Duration = Duration::from_secs(8);
const STEP: Duration = Duration::from_millis(60);
const MODEL_MENU: (&str, &str, &str) = ("/model", "Select model", "Select Model and Effort");
const PERMISSIONS_TITLE: &str = "Update Model Permissions";
const FULL_CONFIRM: &str = "Enable full access?";
const FULL_YES: &str = "Yes, continue anyway";
const EFFORT_TITLE: &str = "Select Reasoning Level";
const NOT_FOUND: &str = "菜单里没有这一项，请重新打开看一眼";

/// The terminal as the host gives it to this file.
pub trait Terminal {
    fn screen(&self) -> io::Result<String>;
    fn keys(&self, keys: &[&str]) -> io::Result<()>;
    /// As a paste, with no Enter.
    fn paste(&self, text: &str) -> io::Result<()>;
    /// The command lines of its shell and what runs below it.
    fn commands(&self) -> io::Result<Vec<String>>;
}

/// What a change set, in the words cards use.
#[derive(Debug, Default)]
pub struct Changed {
    pub model: Option<String>,
    pub permission: Option<String>,
    pub permission_full: Option<bool>,
    /// In the CLI's own word for it.
    pub effort: Option<String>,
}

/// Whether this Agent's modes can be changed from here.
pub fn switches(agent: Agent) -> bool {
    matches!(agent, Agent::Claude | Agent::Codex)
}

/// What `agent` offers. Says why not when it is not at its prompt.
pub fn options(term: &dyn Terminal, agent: Agent, kind: ModeKind) -> io::Result<ModeOptions> {
    let s = Steps::start(term, agent)?;
    match (kind, agent) {
        (ModeKind::Model, _) => s.model_options(),
        (ModeKind::Permission, Agent::Claude) => s.claude_options(),
        (ModeKind::Permission, _) => s.codex_options(),
        (ModeKind::Effort, Agent::Claude) => s.claude_effort_options(),
        (ModeKind::Effort, _) => s.codex_effort_options(),
    }
}

/// Picks `choice`. Nothing is pressed for the mode that stops asking without `allow_full`.
pub fn set(
    term: &dyn Terminal,
    agent: Agent,
    kind: ModeKind,
    choice: &str,
    allow_full: bool,
) -> io::Result<Changed> {
    let s = Steps::start(term, agent)?;
    match (kind, agent) {
        (ModeKind::Model, Agent::Claude) => s.set_claude_model(choice),
        (ModeKind::Model, _) => s.set_codex_model(choice),
        (ModeKind::Permission, Agent::Claude) => s.set_claude_mode(choice, allow_full),
        (ModeKind::Permission, _) => s.set_codex_mode(choice, allow_full),
        (ModeKind::Effort, Agent::Claude) => s.set_claude_effort(choice),
        (ModeKind::Effort, _) => s.set_codex_effort(choice),
    }
}

struct Steps<'a> {
    term: &'a dyn Terminal,
    agent: Agent,
    until: Instant,
}

impl<'a> Steps<'a> {
    fn start(term: &'a dyn Terminal, agent: Agent) -> io::Result<Self> {
        let s = Self {
            term,
            agent,
            until: Instant::now() + WHOLE,
        };
        let tail = s.screen()?;
        if read::working(&tail) {
            return Err(io::Error::other("这一轮还没结束，等它停下再换"));
        }
        if read::composer(agent, &tail).is_none() {
            return Err(io::Error::other("画面上有别的提示，先在终端里处理"));
        }
        Ok(s)
    }

    fn screen(&self) -> io::Result<String> {
        self.term.screen()
    }

    fn keys(&self, keys: &[&str]) -> io::Result<()> {
        self.term.keys(keys)
    }

    /// Looks until `seen` finds what it wants, at most `wait`.
    fn wait<T>(&self, wait: Duration, seen: impl Fn(&str) -> Option<T>) -> io::Result<Option<T>> {
        let until = (Instant::now() + wait).min(self.until);
        loop {
            if let Some(found) = seen(&self.screen()?) {
                return Ok(Some(found));
            }
            if Instant::now() >= until {
                return Ok(None);
            }
            thread::sleep(STEP);
        }
    }

    fn at_prompt(&self) -> io::Result<bool> {
        let agent = self.agent;
        Ok(self
            .wait(Duration::from_secs(2), |t| read::composer(agent, t))?
            .is_some())
    }

    /// Types `command` and takes it only once the box holds exactly that: text left in it would
    /// otherwise go out with Enter, as a message to the Agent.
    fn open(&self, command: &str, title: &str) -> io::Result<Menu> {
        self.run(command)?;
        let menu = self.wait(Duration::from_secs(3), |t| read::menu(t, title))?;
        menu.ok_or_else(|| io::Error::other("菜单没有打开，请打开终端看一眼"))
    }

    /// Types `command` and presses Enter, as `open` does, without waiting for a menu.
    fn run(&self, command: &str) -> io::Result<()> {
        self.term.paste(command)?;
        let agent = self.agent;
        let typed = self.wait(Duration::from_millis(1500), |t| {
            (read::composer(agent, t).as_deref() == Some(command)).then_some(())
        })?;
        if typed.is_none() {
            let erase = vec!["backspace"; command.chars().count()];
            self.keys(&erase)?;
            return Err(io::Error::other("输入框里有还没发的字，先在终端里处理"));
        }
        self.keys(&["enter"])
    }

    /// Esc out of whatever `levels` menus stand open.
    fn close(&self, levels: usize) -> io::Result<()> {
        for _ in 0..levels {
            self.keys(&["esc"])?;
            thread::sleep(Duration::from_millis(200));
        }
        Ok(())
    }

    fn model_title(&self) -> &'static str {
        match self.agent {
            Agent::Claude => MODEL_MENU.1,
            _ => MODEL_MENU.2,
        }
    }

    /// Every row of the open menu, walking down through one that scrolls until it wraps.
    fn all_rows(&self, title: &str, first: Menu) -> io::Result<Vec<Row>> {
        let mut rows = first.rows.clone();
        if first.more {
            for _ in 0..30 {
                self.keys(&["down"])?;
                thread::sleep(Duration::from_millis(120));
                let Some(m) = read::menu(&self.screen()?, title) else {
                    break;
                };
                let wrapped = read::pointed(&m).is_some_and(|r| r.number == 1);
                for r in m.rows {
                    if !rows.iter().any(|h| h.number == r.number) {
                        rows.push(r);
                    }
                }
                if wrapped || Instant::now() >= self.until {
                    break;
                }
            }
        }
        rows.sort_by_key(|r| r.number);
        Ok(rows)
    }

    fn model_options(&self) -> io::Result<ModeOptions> {
        let title = self.model_title();
        let first = self.open(MODEL_MENU.0, title)?;
        let rows = self.all_rows(title, first);
        self.close(1)?;
        let rows = rows?;
        let more = rows.iter().any(|r| r.label == "More models");
        let options = rows
            .into_iter()
            .filter(|r| r.label != "More models")
            .map(|r| ModeOption {
                key: r.label.clone(),
                label: r.label,
                detail: r.detail,
                current: r.current,
                full: false,
            })
            .collect();
        Ok(ModeOptions {
            options,
            note: if more {
                "只对这次会话有效，更早的型号在电脑上选。聊过之后换，下一句会重读历史，慢一些也多用 token。"
                    .into()
            } else {
                "只对这次会话有效。".into()
            },
        })
    }

    fn claude_options(&self) -> io::Result<ModeOptions> {
        let now = read::claude_mode(&self.screen()?).unwrap_or_default();
        let offered = self.offers_full(now)?;
        let options = read::CLAUDE_CYCLE
            .iter()
            .filter(|(key, _)| *key != read::CLAUDE_FULL || offered)
            .map(|(key, _)| ModeOption {
                key: (*key).to_owned(),
                label: read::card_words(Agent::Claude, key),
                detail: read::claude_option_detail(key).to_owned(),
                current: *key == now,
                full: *key == read::CLAUDE_FULL,
            })
            .collect();
        Ok(ModeOptions {
            options,
            note: if offered {
                String::new()
            } else {
                "这次启动没有带 --allow-dangerously-skip-permissions，Claude 这里没有「全部放行」。"
                    .into()
            },
        })
    }

    fn codex_options(&self) -> io::Result<ModeOptions> {
        let menu = self.open("/permissions", PERMISSIONS_TITLE)?;
        self.close(1)?;
        Ok(ModeOptions {
            options: (menu.rows.into_iter())
                .map(|r| ModeOption {
                    full: read::codex_is_full(&r.label),
                    key: r.label.clone(),
                    label: read::card_words(Agent::Codex, &r.label),
                    detail: r.detail,
                    current: r.current,
                })
                .collect(),
            note: String::new(),
        })
    }

    /// Moves the menu's pointer onto `choice`, looking after each key. Back where it started
    /// without seeing it, it is not on this menu.
    fn point(&self, title: &str, menu: &Menu, choice: &str) -> io::Result<Menu> {
        let start = read::pointed(menu).map(|r| r.number);
        let mut now = menu.clone();
        for _ in 0..40 {
            if read::pointed(&now).is_some_and(|r| r.label == choice) {
                return Ok(now);
            }
            if Instant::now() >= self.until {
                break;
            }
            let before = read::pointed(&now).map(|r| r.number);
            self.keys(&["down"])?;
            let moved = self.wait(Duration::from_millis(700), |t| {
                read::menu(t, title).filter(|m| read::pointed(m).map(|r| r.number) != before)
            })?;
            let Some(m) = moved else { break };
            if read::pointed(&m).map(|r| r.number) == start {
                break;
            }
            now = m;
        }
        self.close(1)?;
        Err(io::Error::other(NOT_FOUND))
    }

    fn set_claude_model(&self, choice: &str) -> io::Result<Changed> {
        let title = self.model_title();
        let menu = self.open(MODEL_MENU.0, title)?;
        if !menu.rows.iter().any(|r| r.label == choice) || choice == "More models" {
            self.close(1)?;
            return Err(io::Error::other(NOT_FOUND));
        }
        let menu = self.point(title, &menu, choice)?;
        if !read::offers_session(&menu.keys) {
            self.close(1)?;
            return Err(io::Error::other(
                "菜单没有只改这次会话的选项，没有换，请在终端里换",
            ));
        }
        self.keys(&["s"])?;
        let took = |t: &str| {
            read::claude_model_set(t)
                .filter(|m| read::claude_model_took(m, choice))
                .filter(|_| read::composer(Agent::Claude, t).is_some())
        };
        let asks = |t: &str| read::claude_switch_question(t, choice);
        let first = self.wait(Duration::from_secs(3), |t| {
            (took(t).is_some() || asks(t)).then(|| asks(t))
        })?;
        if first == Some(true) {
            // Its yes, for this model, under the pointer: the person chose it knowing the note.
            self.keys(&["enter"])?;
        }
        let Some(now) = self.wait(Duration::from_secs(3), took)? else {
            return Err(io::Error::other("按了，但画面上没说换好，请打开终端看一眼"));
        };
        Ok(Changed {
            model: Some(now),
            permission: None,
            permission_full: None,
            effort: None,
        })
    }

    fn set_codex_model(&self, choice: &str) -> io::Result<Changed> {
        let title = self.model_title();
        let menu = self.open(MODEL_MENU.0, title)?;
        self.point(title, &menu, choice)?;
        self.keys(&["enter"])?;
        let effort = self.wait(Duration::from_secs(2), |t| read::menu(t, EFFORT_TITLE))?;
        let Some(effort) = effort else {
            self.close(2)?;
            return Err(io::Error::other("没看到选档位的菜单，没有换，请在终端里换"));
        };
        if !read::offers_session(&effort.keys) {
            self.close(2)?;
            return Err(io::Error::other(
                "菜单没有只改这次会话的选项，没有换，请在终端里换",
            ));
        }
        self.keys(&["s"])?;
        let footer = self.wait(Duration::from_secs(3), |t| {
            read::codex_footer(t).filter(|f| f.starts_with(choice))
        })?;
        let Some(now) = footer else {
            let saw = read::codex_footer(&self.screen()?).unwrap_or_default();
            return Err(io::Error::other(format!("按了，但它现在写的是「{saw}」")));
        };
        Ok(Changed {
            model: Some(now),
            permission: None,
            permission_full: None,
            effort: None,
        })
    }

    /// Whether this session's Shift+Tab has the bypass mode: on it now, or started allowing it.
    fn offers_full(&self, now: &str) -> io::Result<bool> {
        if now == read::CLAUDE_FULL {
            return Ok(true);
        }
        Ok(self
            .term
            .commands()?
            .iter()
            .any(|c| read::claude_offers_full(c)))
    }

    fn set_claude_mode(&self, choice: &str, allow_full: bool) -> io::Result<Changed> {
        if !read::CLAUDE_CYCLE.iter().any(|(k, _)| *k == choice) {
            return Err(io::Error::other(NOT_FOUND));
        }
        let full = choice == read::CLAUDE_FULL;
        if full && !allow_full {
            return Err(io::Error::other("切到全部放行要先在手机上确认"));
        }
        let start = read::claude_mode(&self.screen()?)
            .ok_or_else(|| io::Error::other("读不出现在是哪一档"))?;
        let offered = self.offers_full(start)?;
        if full && !offered {
            return Err(io::Error::other(
                "这次启动没有带 --allow-dangerously-skip-permissions，没有这一档",
            ));
        }
        if offered && !full && read::claude_crosses_full(start, choice) {
            return Err(io::Error::other(format!(
                "从这一档到「{}」要经过全部放行，手机不替你经过，请在终端里切",
                read::card_words(Agent::Claude, choice)
            )));
        }
        let mut now = start;
        for _ in 0..read::CLAUDE_CYCLE.len() + 1 {
            if now == choice {
                return Ok(Changed {
                    model: None,
                    permission: Some(read::card_words(Agent::Claude, choice)),
                    permission_full: Some(full),
                    effort: None,
                });
            }
            self.keys(&["shift-tab"])?;
            let before = now;
            let landed = self.wait(Duration::from_millis(1200), |t| {
                read::claude_mode(t).filter(|m| *m != before)
            })?;
            let Some(landed) = landed else {
                return Err(io::Error::other(format!(
                    "按了，但它还在「{}」",
                    read::card_words(Agent::Claude, now)
                )));
            };
            now = landed;
            if now == start {
                return Err(io::Error::other("这个会话里没有这一档，已回到原来那档"));
            }
            // Started some other way than its flags say: nothing may rest here unasked.
            // The next key leaves it, and the next look says if it did not.
        }
        Err(io::Error::other(format!(
            "没切到，现在是「{}」",
            read::card_words(Agent::Claude, now)
        )))
    }

    fn set_codex_mode(&self, choice: &str, allow_full: bool) -> io::Result<Changed> {
        let full = read::codex_is_full(choice);
        if full && !allow_full {
            return Err(io::Error::other("切到全部放行要先在手机上确认"));
        }
        let menu = self.open("/permissions", PERMISSIONS_TITLE)?;
        if !menu.rows.iter().any(|r| r.label == choice) {
            self.close(1)?;
            return Err(io::Error::other(NOT_FOUND));
        }
        self.point(PERMISSIONS_TITLE, &menu, choice)?;
        self.keys(&["enter"])?;
        let agent = self.agent;
        // Its prompt back, or its own question first.
        enum Next {
            Prompt,
            Confirm(Menu),
        }
        let next = self.wait(Duration::from_secs(2), |t| {
            if let Some(m) = read::menu(t, FULL_CONFIRM) {
                return Some(Next::Confirm(m));
            }
            (read::menu(t, PERMISSIONS_TITLE).is_none() && read::composer(agent, t).is_some())
                .then_some(Next::Prompt)
        })?;
        match next {
            Some(Next::Prompt) => (),
            Some(Next::Confirm(m)) => {
                let on_yes = read::pointed(&m).is_some_and(|r| r.label == FULL_YES);
                if !(full && allow_full && on_yes) {
                    self.close(2)?;
                    return Err(io::Error::other(
                        "它问得和平时不一样，没有放行，请在终端里看",
                    ));
                }
                self.keys(&["enter"])?;
            }
            None => {
                self.close(2)?;
                return Err(io::Error::other("它弹了没见过的提示，请打开终端看一眼"));
            }
        }
        if !self.at_prompt()? {
            return Err(io::Error::other("按了，但没回到输入框，请打开终端看一眼"));
        }
        // What it says now, from its own menu.
        let again = self.open("/permissions", PERMISSIONS_TITLE)?;
        self.close(1)?;
        let on = again
            .rows
            .iter()
            .find(|r| r.current)
            .map(|r| r.label.clone());
        if on.as_deref() != Some(choice) {
            return Err(io::Error::other(format!(
                "按了，但它现在是「{}」",
                on.as_deref()
                    .map_or("读不出".into(), |l| read::card_words(agent, l))
            )));
        }
        Ok(Changed {
            model: None,
            permission: Some(read::card_words(agent, choice)),
            permission_full: super::permission::codex_option(choice).full,
            effort: None,
        })
    }
}

#[path = "modes_effort.rs"]
mod effort;
