//! A workspace with one conversation in every state and every naming case, for the
//! tests that check each surface (the window, the menu bar, `mooring list`, the phone,
//! plugin events) shows a conversation the way core decides it. Not used at runtime.
use crate::agent::Agent;
use crate::content::{Background, Content, Turn, now};
use crate::layout::{Axis, Layout};
use crate::model::*;

/// What each sample conversation is for; `id` is the session id and, prefixed with
/// `pty-` / `tab-`, its terminal and tab.
pub const CASES: &[&str] = &[
    "waiting",
    "error",
    "running",
    "background",
    "review",
    "fresh",
    "deferred",
    "closed",
    "renamed",
    "tasked",
    "tab-named",
    "split",
    "split-two",
    "exited",
    "hidden",
    "subdir",
];

fn content(turn: Turn, at: u64) -> Content {
    Content {
        interactive: true,
        title: "第一句话 帮我看看登录".into(),
        turn,
        turn_started: at.saturating_sub(120),
        updated: at,
        waiting: if turn == Turn::Waiting {
            "继续吗？".into()
        } else {
            String::new()
        },
        error: if turn == Turn::Error {
            "测试失败".into()
        } else {
            String::new()
        },
        ..Default::default()
    }
}

/// Every case of `CASES` in one workspace, live in Mooring terminals open in tabs unless
/// the case says otherwise.
pub fn every_state() -> Workspace {
    let at = now();
    let mut w = Workspace::default();
    w.spaces.push(Space {
        id: "space".into(),
        name: "工作空间".into(),
        cwd: "/project".into(),
    });
    w.tasks.push(Task {
        id: "task".into(),
        space: "space".into(),
        title: "修登录".into(),
        description: String::new(),
        acceptance: vec![],
        agent: Agent::Claude,
    });
    for case in CASES.iter().copied() {
        let pty = format!("pty-{case}");
        let mut terminals = vec![pty.clone()];
        if case == "split" {
            // One conversation beside a plain shell.
            terminals.push("pty-split-shell".into());
        }
        for id in &terminals {
            w.terminals.push(Terminal {
                id: id.clone(),
                space: "space".into(),
                argv: vec!["/bin/zsh".into()],
                cwd: "/project".into(),
                pid: None,
                state: if case == "exited" {
                    ProcessState::Exited
                } else {
                    ProcessState::Running
                },
                exit_code: None,
                error: None,
            });
        }
        let layout = match (case, terminals.as_slice()) {
            // Two conversations in one tab: `split-two` joins `split`'s tab below.
            ("split-two", _) => None,
            (_, [one]) => Some(Layout::Leaf(one.clone())),
            (_, [a, b]) => Some(Layout::Split {
                axis: Axis::Right,
                ratio: 500,
                first: Box::new(Layout::Leaf(a.clone())),
                second: Box::new(Layout::Leaf(b.clone())),
            }),
            _ => None,
        };
        if let Some(layout) = layout {
            w.tabs.push(Tab {
                id: format!("tab-{case}"),
                space: "space".into(),
                default_task: None,
                title: if case == "tab-named" {
                    "部署".into()
                } else {
                    String::new()
                },
                layout: Some(layout),
            });
        }
        let turn = match case {
            "waiting" | "deferred" | "hidden" => Turn::Waiting,
            "error" | "exited" => Turn::Error,
            "running" | "split" | "split-two" => Turn::Running,
            "fresh" => Turn::Unknown,
            _ => Turn::Completed,
        };
        w.agent_sessions.push(AgentSession {
            id: case.into(),
            runtime: if case == "split-two" {
                Agent::Codex
            } else {
                Agent::Claude
            },
            cli_session_id: format!("cli-{case}"),
            terminal: Some(pty.clone()),
            current: true,
            task: (case == "tasked").then(|| "task".into()),
            outcome: match case {
                "deferred" => Outcome::Deferred {
                    reason: String::new(),
                    until: at + 3600,
                },
                "closed" => Outcome::Completed,
                _ => Outcome::Open,
            },
            updated: at.saturating_sub(600),
            cwd: if case == "subdir" {
                "/project/web".into()
            } else {
                "/project".into()
            },
            transcript: String::new(),
            binding: None,
            name: match case {
                "renamed" => "我起的名".into(),
                _ => String::new(),
            },
        });
        if case != "fresh" {
            let mut c = content(turn, at);
            if case == "background" {
                c.background = Some(Background {
                    crons: 1,
                    ..Default::default()
                });
            }
            if case == "hidden" {
                c.interactive = false;
            }
            w.content.insert(case.into(), c);
        }
    }
    // `split-two` runs in the second pane of `split`'s tab.
    if let Some(tab) = w.tabs.iter_mut().find(|t| t.id == "tab-split")
        && let Some(Layout::Split { second, .. }) = tab.layout.as_mut()
    {
        **second = Layout::Leaf("pty-split-two".into());
    }
    w.terminals.retain(|t| t.id != "pty-split-shell");
    w.adopt_tab_names();
    w
}
