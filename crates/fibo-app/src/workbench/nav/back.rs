//! 回到刚才 (#515): ⌘J and a notification note where they left from; the shortcut goes
//! back there, and pressed again returns to where it was pressed.
use super::super::*;
use super::Spot;

impl Workbench {
    /// Where the user is now, to come back to.
    fn spot(&self, cx: &gpui::App) -> Spot {
        let peek = self.peek.as_ref().and_then(|p| p.remote.as_ref());
        Spot {
            host: self.hosts.showing.clone(),
            hosted: (self.hosts.showing.as_ref())
                .and_then(|_| self.hosted_view()?.read(cx).nav.focused.clone()),
            peek: peek.map(|r| (r.machine, r.terminal.clone(), r.host.clone())),
            page: self.nav.page(),
            focused: self.nav.focused.clone(),
            here: self.overview.here.clone(),
        }
    }
    /// What the view is on, as ⌘J counts its place in the queue: the peeked or drawn
    /// machine's terminal, the focused pane's session or asking terminal, the board's card.
    pub(in crate::workbench) fn here_key(&self, cx: &gpui::App) -> Option<String> {
        match self.peek.as_ref().filter(|p| p.remote.is_some()) {
            Some(peek) => Some(peek.terminal.clone()),
            None if self.hosted_view().is_some() => self.hosted_focus(cx),
            None => self
                .nav
                .focused
                .as_ref()
                .and_then(|id| self.current_agent(id))
                .map(|s| s.id.clone())
                .or_else(|| {
                    (self.nav.focused.clone()).filter(|t| self.store.recognition().asks_first(t))
                })
                .or_else(|| self.overview.here.clone()),
        }
    }
    /// A jump the user did not pick a place for (⌘J, a notification) is about to go to `to`.
    pub(in crate::workbench) fn note_jump(&mut self, to: &str, cx: &gpui::App) {
        let here = self.here_key(cx);
        let from = self.spot(cx);
        self.nav.trail.jump(from, here.as_deref(), to);
    }
    /// The jump noted by `note_jump` has moved the view.
    pub(in crate::workbench) fn note_landed(&mut self) {
        let page = self.nav.page();
        self.nav.trail.landed_on(page);
    }
    /// 回到刚才: the spot the last jump left from; nothing when no jump left one.
    pub(in crate::workbench) fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // As another machine's main area, the window's key: the window noted the jump.
        if self.hosts.embedded {
            cx.emit(super::super::hosts::HostEvent::Back);
            return;
        }
        let from = self.spot(cx);
        let Some(to) = self.nav.trail.back(from) else {
            return;
        };
        self.close_peek(cx);
        if let Some((index, terminal, name)) = &to.peek {
            let row = (self.remote_rows_of(*index).into_iter()).find(|r| r.card.id == *terminal);
            if let Some(row) = row {
                self.open_remote_peek(*index, row.card, name, window, cx);
                return;
            }
        }
        if let Some(host) = to
            .host
            .filter(|h| self.hosts.views.iter().any(|(v, _)| v == h))
        {
            let go = to.hosted.map(super::super::hosts::Go::Terminal);
            self.show_host(host, go, window, cx);
            return;
        }
        let (space, tab, overview, detail) = to.page;
        if overview {
            self.leave_host(cx);
            self.nav.space = space;
            // The tab under the board, too: leaving it shows the panes of nav.tab.
            if let Some(tab) = tab.filter(|t| self.workspace.tabs.iter().any(|w| w.id == *t)) {
                self.nav.tab = Some(tab);
            }
            self.show_overview(cx);
            self.overview.here = to.here;
        } else if let Some(terminal) = to
            .focused
            .filter(|t| self.workspace.terminal_tab(t).is_some())
        {
            self.focus_terminal(&terminal, window, cx);
        } else {
            if self.nav.overview {
                self.leave_overview(cx);
            }
            self.pick_space(&space, cx);
            if let Some(tab) = tab.filter(|t| self.workspace.tabs.iter().any(|w| w.id == *t)) {
                self.show_tab(&tab, window, cx);
            }
        }
        self.nav.task_detail = detail;
        cx.notify();
    }
}
#[cfg(feature = "visual-test")]
impl Workbench {
    pub fn visual_go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.go_back(window, cx);
    }
}
