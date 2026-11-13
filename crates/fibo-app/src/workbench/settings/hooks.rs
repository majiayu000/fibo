//! 设置 › 通用's Agent Hook card: the one place a CLI's hook is installed, renewed and taken
//! out, and where the status bar's reminder is switched. The welcome page, the status bar
//! and ⌘P lead here.
use super::*;
use crate::agent_hooks::State;

impl SettingsView {
    /// Installs (`true`) or removes `agent`'s hooks first when given (only ever from a
    /// click), then reads every CLI's again. Both run in the background: they spawn
    /// `fibo` and read small files.
    pub(super) fn change_hooks(change: Option<(String, bool)>, cx: &mut Context<Self>) {
        let changed = change.is_some();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match &change {
                        Some((agent, true)) => crate::agent_hooks::install(agent)?,
                        Some((agent, false)) => crate::agent_hooks::uninstall(agent)?,
                        None => {}
                    }
                    crate::agent_hooks::check()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.hooks = Some(result.map_err(|e| e.to_string()));
                this.installing = None;
                if changed {
                    cx.emit(SettingsAction::HooksChanged);
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// One row per CLI whose hooks Fibo installs: their state as `mooring hooks
    /// status` read it when the panel opened, and the one thing to do about it.
    fn hook_rows(&self, first: bool, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        let hooks = match &self.hooks {
            None => {
                return vec![
                    self.row(first, "Agent Hook", t!("正在检查…"), div())
                        .into_any_element(),
                ];
            }
            Some(Err(e)) => {
                let note = t!("没能检查：{e}", e);
                return vec![
                    self.row(first, "Agent Hook", &note, div())
                        .into_any_element(),
                ];
            }
            Some(Ok(hooks)) => hooks,
        };
        hooks
            .iter()
            .enumerate()
            .map(|(i, hook)| {
                // (what the row says, the button, whether the button installs)
                let (state, action) = match hook.state {
                    State::Current => (t!("已安装且是最新"), Some((t!("卸载"), false))),
                    State::Outdated => (t!("已安装但过期"), Some((t!("重新安装"), true))),
                    State::Missing => (t!("未安装"), Some((t!("安装"), true))),
                    State::Unreadable => (t!("读不了这个文件"), None),
                };
                let mut note = state.to_owned();
                if !hook.differences.is_empty() {
                    note = t!(
                        "{note}：{diffs}",
                        note,
                        diffs = hook.differences.join(t!("；"))
                    );
                }
                let busy = self.installing.is_some();
                let control = action.map(|(text, install)| {
                    let agent = hook.agent.clone();
                    let text = if self.installing.as_ref() == Some(&agent) {
                        if install {
                            t!("正在安装…")
                        } else {
                            t!("正在卸载…")
                        }
                    } else {
                        text
                    };
                    secondary(format!("hooks-{agent}"), text)
                        .when(busy, |b| b.opacity(0.5))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.installing.is_some() {
                                return;
                            }
                            // Writes the CLI's settings: only ever from this click.
                            this.installing = Some(agent.clone());
                            cx.notify();
                            Self::change_hooks(Some((agent.clone(), install)), cx);
                        }))
                });
                self.file_row(
                    first && i == 0,
                    format!("{} Hook", hook.name),
                    tilde(&hook.settings),
                    Some((
                        note,
                        matches!(hook.state, State::Outdated | State::Unreadable),
                    )),
                    div().children(control),
                    cx,
                )
            })
            .collect()
    }
    /// The card's rows: each CLI's hook, then the reminder's switch.
    pub(super) fn hook_card(&self, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        let mut rows = Vec::new();
        if self.shows(Page::General, "Agent Hook", cx) {
            rows.extend(self.hook_rows(true, cx));
        }
        if self.shows(Page::General, t!("没装 Hook 时提醒"), cx) {
            let on = crate::ui::hook_hint::reminded(cx);
            let toggle =
                switch("hook-hint-toggle", on).on_click(cx.listener(move |this, _, _, cx| {
                    this.text_error = crate::ui::hook_hint::choose(!on, cx).err();
                    cx.refresh_windows();
                    cx.notify();
                }));
            rows.push(
                self.row(
                    rows.is_empty(),
                    t!("没装 Hook 时提醒"),
                    t!("Hook 让 Agent 自己报状态和审批，不靠读终端画面，只在 Fibo 的终端里起作用。开着的 Agent 没装或过期时，在状态栏说一声"),
                    toggle,
                )
                .into_any_element(),
            );
        }
        rows
    }
}
