//! Another machine's row's right-click menu: what a row of this machine offers (打开,
//! 置顶, 重命名, 标记完成, 关闭终端, 放弃), asked of and done by that machine's workbench
//! here, so its own window and every other one shows the result. A terminal no
//! conversation is tied to offers 置顶 alone; a host from before it kept pins, no 置顶.
use super::*;
use crate::workbench::sidebar_list::SessionAction;

impl Workbench {
    /// Right-click on another machine's row: its menu at the pointer.
    pub(in crate::workbench) fn open_remote_menu(
        &self,
        r: &MachineRow,
        cx: &Context<Self>,
    ) -> impl Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
        let (index, terminal) = (r.index, r.target().to_owned());
        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
            if this.remote_menu_entries(index, &terminal, cx).is_empty() {
                return;
            }
            this.layout.remote_menu = Some((index, terminal.clone(), event.position));
            cx.stop_propagation();
            cx.notify();
        })
    }
    /// The session `target` names on machine `index`: its terminal's, or itself once it
    /// ended.
    fn target_session(&self, index: usize, target: &str, cx: &gpui::App) -> Option<String> {
        if let Some(r) = self.remote_card(index, target) {
            return r.card.session;
        }
        let view = self.machine_view(index)?;
        let w = view.read(cx);
        (w.workspace.agent_sessions.iter())
            .find(|s| s.id == target)
            .map(|s| s.id.clone())
    }
    /// Machine `index`'s card for `terminal`, as its row has it.
    fn remote_card(&self, index: usize, terminal: &str) -> Option<RemoteRow> {
        (self.remote_rows_of(index).into_iter()).find(|r| r.card.id == terminal)
    }
    /// What it lists: that machine's own session menu for the conversation in `terminal`,
    /// less 关闭终端 where its confirmation could not show (peek on, or this window being
    /// another machine's); else 置顶 or 取消置顶, when that machine's host keeps pins.
    pub(in crate::workbench) fn remote_menu_entries(
        &self,
        index: usize,
        terminal: &str,
        cx: &gpui::App,
    ) -> Vec<(&'static str, SessionAction)> {
        let Some(view) = self.machine_view(index) else {
            return Vec::new();
        };
        let w = view.read(cx);
        let session = self.target_session(index, terminal, cx);
        let mut entries = session.map_or_else(Vec::new, |id| w.session_menu_entries(&id));
        // An ended session there has no terminal to pin.
        let live = w.workspace.terminals.iter().any(|t| t.id == terminal);
        if entries.is_empty() && live && w.can_pin() {
            let pinned = w.pins().iter().any(|p| p == terminal);
            let name = if pinned {
                t!("取消置顶")
            } else {
                t!("置顶")
            };
            entries.push((name, SessionAction::Pin(terminal.to_owned())));
        }
        if crate::ui::peek::enabled(cx) || self.host.attached() {
            entries.retain(|(_, a)| !matches!(a, SessionAction::Close(_)));
        }
        entries
    }
    pub(in crate::workbench) fn remote_menu_view(
        &self,
        index: usize,
        terminal: &str,
        position: gpui::Point<gpui::Pixels>,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let entries: Vec<_> = (self.remote_menu_entries(index, terminal, cx).into_iter())
            .map(|(name, action)| {
                let terminal = terminal.to_owned();
                button(
                    gpui::SharedString::from(format!("remote-menu-{name}")),
                    name,
                )
                .w_full()
                .justify_start()
                .border_0()
                .when(name == t!("放弃"), |d| d.text_color(Tone::Danger.ink()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.layout.remote_menu = None;
                    this.remote_action(index, &terminal, &action, window, cx);
                    cx.notify();
                }))
                .into_any_element()
            })
            .collect();
        (!entries.is_empty())
            .then(|| self.popup(position, entries, cx, |this| this.layout.remote_menu = None))
    }
    /// 打开 is the row's click; 关闭终端 shows that machine first, where its confirmation
    /// draws; the rest that machine's workbench does as on its own row.
    pub(in crate::workbench) fn remote_action(
        &mut self,
        index: usize,
        terminal: &str,
        action: &SessionAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let card = self.remote_card(index, terminal);
        let Some(session) = self.target_session(index, terminal, cx) else {
            return;
        };
        match (action, card) {
            (SessionAction::Open, Some(r)) => {
                self.open_remote_peek(index, r.card, &r.host_name, window, cx);
                return;
            }
            // Ended there: that machine resumes it in its own main area, shown here.
            (SessionAction::Open, None) => {
                let space = self.machine_view(index).and_then(|v| {
                    let w = v.read(cx);
                    let s = w
                        .workspace
                        .agent_sessions
                        .iter()
                        .find(|s| s.id == session)?;
                    w.workspace.session_space(s).map(str::to_owned)
                });
                let go = super::super::hosts::Go::Space(space.unwrap_or_default());
                if !self.open_on_host(index, go, window, cx) {
                    return;
                }
            }
            (SessionAction::Close(_), None) => return,
            _ => {}
        }
        if matches!(action, SessionAction::Close(_)) {
            let go = super::super::hosts::Go::Terminal(terminal.to_owned());
            if !self.open_on_host(index, go, window, cx) {
                return;
            }
        }
        let Some(view) = self.machine_view(index) else {
            return;
        };
        let drawn = self.hosted_view().is_some_and(|v| v == view);
        view.update(cx, |w, cx| match action {
            // 标记完成's animation plays on that machine's pane only while it is drawn.
            SessionAction::Settle(outcome) => {
                let found = w.workspace.agent_sessions.iter().find(|s| s.id == session);
                if let Some(s) = found.cloned() {
                    w.settle_session(&s, outcome.clone(), drawn, cx);
                }
            }
            _ => w.session_action(&session, action, window, cx),
        });
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// Pins or unpins `terminal` on machine `index`, by its host.
    pub(in crate::workbench) fn pin_remote(
        &mut self,
        index: usize,
        terminal: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.machine_view(index) {
            view.update(cx, |w, cx| w.toggle_pin(terminal, cx));
        }
    }
    /// What this workbench's host keeps in 置顶 now, asked of it directly.
    pub fn visual_host_pins(&self) -> Option<Vec<String>> {
        match crate::rpc(self.store.socket(), &Request::Workspace) {
            Ok(Reply::Workspace(w)) => w.pinned_terminals,
            _ => None,
        }
    }
    /// Another window on this workbench's host pins or unpins `terminal` there.
    pub fn visual_host_pin(&self, terminal: &str, pinned: bool) {
        let id = terminal.to_owned();
        let edit = Edit::PinTerminal { id, pinned };
        let _ = crate::rpc(
            self.store.socket(),
            &Request::Edit {
                request: None,
                edit,
            },
        );
    }
    /// The first terminal of tab `id`.
    pub fn visual_tab_terminal(&self, id: &str) -> Option<String> {
        self.workspace.tab_terminals(id).into_iter().next()
    }
    /// Machine `index`'s pins as this window's sidebar has them.
    pub fn visual_machine_pins(&self, index: usize) -> Option<Vec<String>> {
        self.machines.get(index)?.view.lock().unwrap().pins.clone()
    }
    /// What machine `index`'s row for `terminal` lists on a right-click.
    pub fn visual_remote_menu(
        &self,
        index: usize,
        terminal: &str,
        cx: &gpui::App,
    ) -> Vec<&'static str> {
        (self.remote_menu_entries(index, terminal, cx).into_iter())
            .map(|(name, _)| name)
            .collect()
    }
    /// That menu's entry `name` clicked; false when it does not list it.
    pub fn visual_remote_menu_do(
        &mut self,
        index: usize,
        terminal: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let entry =
            (self.remote_menu_entries(index, terminal, cx).into_iter()).find(|(n, _)| *n == name);
        let found = entry.is_some();
        if let Some((_, action)) = entry {
            self.remote_action(index, terminal, &action, window, cx);
        }
        cx.notify();
        found
    }
    /// The outcome machine `index`'s card for `terminal` says.
    pub fn visual_remote_outcome(&self, index: usize, terminal: &str) -> Option<String> {
        self.remote_card(index, terminal)?.card.outcome
    }
    /// Right-click on machine `index`'s row for `terminal`, then its menu's one entry: what
    /// that entry said.
    pub fn visual_remote_menu_pin(
        &mut self,
        index: usize,
        terminal: &str,
        cx: &mut Context<Self>,
    ) -> Option<&'static str> {
        let entry = (self.remote_menu_entries(index, terminal, cx).into_iter())
            .find(|(_, a)| matches!(a, SessionAction::Pin(_)))?
            .0;
        self.pin_remote(index, terminal, cx);
        Some(entry)
    }
}
