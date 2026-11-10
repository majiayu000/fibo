//! The Agent CLIs' hooks as the window knows them. 设置 › 通用 is where they are installed
//! and removed; the status bar and ⌘P lead there, and the welcome page installs the hook
//! of the CLI it starts while its box stays ticked. Read when the window opens, when an
//! Agent is started from it, and after 设置 changed them.
use super::statusbar::item;
use super::*;
use crate::agent_hooks::{Hook, State};

#[derive(Default)]
pub(super) struct Hooks {
    /// As `mooring hooks status` last read them; None until it first answers.
    pub(super) read: Option<Result<Vec<Hook>, String>>,
    /// The welcome page is installing one, with the Agent's start waiting behind it.
    pub(super) installing: bool,
}

/// What the status bar says: the CLI running here whose hook is missing or old.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Hint {
    pub(super) name: String,
    pub(super) outdated: bool,
}
/// Only for a CLI that `runs_here` (by its executable): someone who never runs it in our
/// terminals is not told about its hook. A settings file that cannot be read is said in
/// 设置, where the reason fits.
pub(super) fn hint(
    hooks: &[Hook],
    runs_here: impl Fn(&str) -> bool,
    reminded: bool,
) -> Option<Hint> {
    if !reminded {
        return None;
    }
    hooks
        .iter()
        .filter(|h| matches!(h.state, State::Missing | State::Outdated))
        .find(|h| runs_here(&h.agent))
        .map(|h| Hint {
            name: h.name.clone(),
            outdated: h.state == State::Outdated,
        })
}
/// Whether starting `executable` from the welcome page writes its hook first: only with
/// the box ticked, and never into a settings file that could not be read.
pub(super) fn installs_with_start(hooks: &[Hook], executable: &str, ticked: bool) -> bool {
    ticked
        && hooks
            .iter()
            .any(|h| h.agent == executable && matches!(h.state, State::Missing | State::Outdated))
}
/// A box to tick with its words beside it.
pub(super) fn tick_box(id: &'static str, on: bool, text: String) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::accesskit::Role::CheckBox)
        .aria_label(text.clone())
        .h(px(T::ROW))
        .flex()
        .items_center()
        .gap(px(T::S8))
        .cursor_pointer()
        .text_size(px(T::SMALL))
        .text_color(T::color(T::primary()))
        .child(
            div()
                .size(px(T::S14))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(T::S4))
                .text_size(px(T::CHIP_SIZE))
                .font_weight(gpui::FontWeight::BOLD)
                .when(on, |d| {
                    d.bg(T::color(T::accent()))
                        .text_color(T::color(T::on_accent()))
                        .child("✓")
                })
                .when(!on, |d| {
                    d.border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::secondary()))
                }),
        )
        .child(text)
}

impl Workbench {
    /// Reads the hooks in the background. The first answer also tells an install that was
    /// set up before from a new one: with hooks already there, the welcome page is over and
    /// the window opens the shell it always opened with.
    pub(super) fn read_hooks(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async { crate::agent_hooks::check() })
                .await
                .map_err(|e| e.to_string());
            let _ = this.update(cx, |this, cx| {
                let first = this.hooks.read.is_none();
                let old = first && result.as_deref().is_ok_and(first_run::hooks_installed);
                this.hooks.read = Some(result);
                if old && this.first_run.on {
                    this.finish_first_run(cx);
                    if this.workspace.terminals.is_empty() && !this.nav.overview {
                        this.launch(false, None, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// The welcome page's start: with its box ticked, the CLI's hook is written first so
    /// the conversation about to begin already reports through it. The hooks are read anew
    /// for this, so a click that comes before the page's own read still installs. A hook
    /// that cannot be written does not keep the Agent from starting.
    pub(super) fn first_run_start(&mut self, agent: Agent, cx: &mut Context<Self>) {
        if self.hooks.installing {
            return;
        }
        if !self.first_run.with_hook {
            self.run_agent_here(agent, cx);
            return;
        }
        self.hooks.installing = true;
        cx.notify();
        let executable = agent.executable().to_owned();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let hooks = crate::agent_hooks::check()?;
                    if !installs_with_start(&hooks, &executable, true) {
                        return Ok(hooks);
                    }
                    crate::agent_hooks::install(&executable)?;
                    crate::agent_hooks::check()
                })
                .await
                .map_err(|e| e.to_string());
            let _ = this.update(cx, |this, cx| {
                this.hooks.installing = false;
                match result {
                    Ok(hooks) => this.hooks.read = Some(Ok(hooks)),
                    Err(e) => this.error = Some(t!("Hook 没装上，Agent 照常启动：{e}", e)),
                }
                this.run_agent_here(agent, cx);
                cx.notify();
            });
        })
        .detach();
    }
    /// The status bar's hook item: there only while a CLI running here has no hook or an
    /// old one, the welcome page is not already saying it, and the reminder is on. A click
    /// opens 设置 › 通用, which says what the hook is and installs it.
    pub(super) fn hook_item(&self, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        if self.first_run.on {
            return None;
        }
        let hooks = self.hooks.read.as_ref()?.as_ref().ok()?;
        // A conversation going on in one of our terminals, or a CLI the host recognized by
        // its process before it said anything.
        let facts = self.facts();
        let recognized = &self.store.recognition().terminal_agents;
        let runs_here = |executable: &str| {
            (facts.running.iter())
                .chain(recognized.values())
                .any(|a| a.executable() == executable)
        };
        let hint = hint(hooks, runs_here, crate::ui::hook_hint::reminded(cx))?;
        let name = hint.name;
        let text = if hint.outdated {
            t!("{name} 的 Hook 过期了", name)
        } else {
            t!("{name} 没装 Hook", name)
        };
        Some(
            item("status-hook", false)
                .aria_label(text.clone())
                .gap(px(T::S4))
                .child(div().size(px(T::DOT)).rounded_full().bg(Tone::Warn.dot()))
                .child(div().text_color(Tone::Warn.ink()).child(text))
                .tooltip(super::diagnostics::tip(vec![
                    t!("没有 Hook，它的状态和审批只能靠读终端画面来判断").into(),
                    t!("点开 设置 › 通用 安装；这条提醒也在那里关").into(),
                ]))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_settings_page(settings::Page::General, window, cx);
                }))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(agent: &str, state: State) -> Hook {
        Hook {
            agent: agent.into(),
            name: "Claude Code".into(),
            settings: "/h/.claude/settings.json".into(),
            state,
            differences: vec![],
        }
    }
    fn agent(executable: &str) -> Agent {
        Agent::from_executable(executable).unwrap()
    }

    #[test]
    fn the_bar_reminds_only_for_a_cli_running_here_without_its_hook() {
        let missing = [hook("claude", State::Missing)];
        let claude = |e: &str| agent("claude").executable() == e;
        assert_eq!(
            hint(&missing, claude, true),
            Some(Hint {
                name: "Claude Code".into(),
                outdated: false
            })
        );
        // After an update the hook is old: said too, in other words.
        let old = [hook("claude", State::Outdated)];
        assert!(hint(&old, claude, true).is_some_and(|h| h.outdated));
        // Only Codex runs here: Claude Code's hook is nobody's business yet.
        assert_eq!(
            hint(&missing, |e| agent("codex").executable() == e, true),
            None
        );
        assert_eq!(hint(&missing, |_| false, true), None);
        // Installed, unreadable (设置 says why), or the reminder switched off.
        assert_eq!(hint(&[hook("claude", State::Current)], claude, true), None);
        assert_eq!(
            hint(&[hook("claude", State::Unreadable)], claude, true),
            None
        );
        assert_eq!(hint(&missing, claude, false), None);
    }

    #[test]
    fn a_start_writes_the_hook_only_with_the_box_ticked_and_a_file_it_could_read() {
        let missing = [hook("claude", State::Missing)];
        assert!(installs_with_start(&missing, "claude", true));
        assert!(installs_with_start(
            &[hook("claude", State::Outdated)],
            "claude",
            true
        ));
        // The box cleared: the settings file is left alone.
        assert!(!installs_with_start(&missing, "claude", false));
        // Another CLI is started: its start does not write Claude Code's settings.
        assert!(!installs_with_start(&missing, "codex", true));
        // Nothing to do, and a file that could not be read is never written over.
        assert!(!installs_with_start(
            &[hook("claude", State::Current)],
            "claude",
            true
        ));
        assert!(!installs_with_start(
            &[hook("claude", State::Unreadable)],
            "claude",
            true
        ));
    }
}
