use super::*;

impl Workbench {
    /// Right-click on a session row: its menu at the pointer.
    pub(in crate::workbench) fn open_session_menu(
        &self,
        session: &AgentSession,
        cx: &Context<Self>,
    ) -> impl Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
        let id = session.id.clone();
        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
            this.layout.session_menu = Some((id.clone(), event.position));
            cx.stop_propagation();
            cx.notify();
        })
    }
    /// What the session menu lists, in order: 打开, then 置顶 / 取消置顶 for a session
    /// whose terminal runs (the pin being that terminal), 重命名, 标记完成 (撤销结局 once
    /// it has an ending), 关闭终端, and last 放弃 while it is not given up.
    pub(in crate::workbench) fn session_menu_entries(
        &self,
        id: &str,
    ) -> Vec<(&'static str, SessionAction)> {
        let Some(session) = self.workspace.agent_sessions.iter().find(|s| s.id == id) else {
            return vec![];
        };
        let terminal = live_terminal(&self.workspace, session).map(|t| t.id.clone());
        let mut out = vec![(t!("打开"), SessionAction::Open)];
        if let Some(t) = &terminal {
            let pinned = self.sidebar.pinned.contains(t);
            out.push((
                if pinned {
                    t!("取消置顶")
                } else {
                    t!("置顶")
                },
                SessionAction::Pin(t.clone()),
            ));
        }
        out.push((t!("重命名"), SessionAction::Rename));
        out.push(match session.outcome {
            Outcome::Open => (t!("标记完成"), SessionAction::Settle(Outcome::Completed)),
            _ => (t!("撤销结局"), SessionAction::Settle(Outcome::Open)),
        });
        if let Some(t) = terminal {
            out.push((t!("关闭终端"), SessionAction::Close(t)));
        }
        if session.outcome != Outcome::Abandoned {
            out.push((t!("放弃"), SessionAction::Settle(Outcome::Abandoned)));
        }
        out
    }
    pub(in crate::workbench) fn session_menu_view(
        &self,
        id: &str,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let entries = self
            .session_menu_entries(id)
            .into_iter()
            .map(|(name, action)| {
                let id = id.to_owned();
                button(
                    gpui::SharedString::from(format!("session-menu-{name}")),
                    name,
                )
                .w_full()
                .justify_start()
                .border_0()
                .when(name == t!("放弃"), |d| d.text_color(Tone::Danger.ink()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.layout.session_menu = None;
                    this.session_action(&id, &action, window, cx);
                    cx.notify();
                }))
                .into_any_element()
            })
            .collect();
        self.popup(position, entries, window, cx, |this| {
            this.layout.session_menu = None
        })
    }
    fn session_action(
        &mut self,
        id: &str,
        action: &SessionAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            SessionAction::Open => {
                if let Some(s) = self.workspace.agent_sessions.iter().find(|s| s.id == id) {
                    let s = s.clone();
                    self.open_session(&s, window, cx);
                }
            }
            SessionAction::Pin(terminal) => self.toggle_pin(terminal, cx),
            SessionAction::Rename => self.rename_session(id, window, cx),
            SessionAction::Close(terminal) => self.close_pane(terminal, cx),
            SessionAction::Settle(outcome) => {
                if let Some(s) = self.workspace.agent_sessions.iter().find(|s| s.id == id) {
                    let s = s.clone();
                    // 标记完成 plays its animation on the session's pane when one shows it.
                    self.settle_session(&s, outcome.clone(), true, cx);
                }
            }
        }
    }
}
