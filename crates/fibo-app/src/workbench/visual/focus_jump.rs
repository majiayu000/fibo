//! Where a jump lands: a sidebar row's click and ⌘J, done the way a click does them, and
//! what the window then says is focused.
use super::*;

/// What the window shows as focused after a jump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusState {
    pub space: String,
    pub tab: Option<String>,
    /// `nav.focused`: the pane the window keeps typing into.
    pub focused: Option<String>,
    /// The shown pane whose focus handle holds the keyboard.
    pub keyboard: Option<String>,
    /// The terminal of the sidebar's selected row.
    pub row: Option<String>,
    pub zoomed: Option<String>,
    pub shown: Vec<String>,
}

impl Workbench {
    pub(super) fn visual_session_of(&self, cli_id: &str) -> AgentSession {
        self.workspace
            .agent_sessions
            .iter()
            .find(|s| s.cli_session_id == cli_id)
            .cloned()
            .unwrap_or_else(|| panic!("no session {cli_id}"))
    }
    /// A click on the sidebar row of the conversation with this CLI id.
    pub fn visual_click_row(&mut self, cli_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let session = self.visual_session_of(cli_id);
        self.click_row(&session, false, window, cx);
    }
    /// The terminal the conversation with this CLI id runs in.
    pub fn visual_terminal_of(&self, cli_id: &str) -> Option<String> {
        live_terminal(&self.workspace, &self.visual_session_of(cli_id)).map(|t| t.id.clone())
    }
    pub fn visual_split_pane(&self, terminal: &str) -> Entity<Pane> {
        self.layout.panes[terminal].clone()
    }
    /// Puts the keyboard in `terminal`'s pane, as a click inside it does.
    pub fn visual_click_pane(
        &mut self,
        terminal: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav.focused = Some(terminal.to_owned());
        if let Some(pane) = self.layout.panes.get(terminal) {
            window.focus(&pane.focus_handle(cx), cx);
        }
        cx.notify();
    }
    /// Shows `tab`, as a click on it does.
    pub fn visual_show_tab(&mut self, tab: &str, cx: &mut Context<Self>) {
        self.nav.tab = Some(tab.to_owned());
        self.nav.overview = false;
        self.nav.pending_focus = true;
        self.sync_panes(cx);
        cx.notify();
    }
    /// Zooms `terminal`'s pane, as its menu's 放大 does.
    pub fn visual_zoom(&mut self, terminal: &str, cx: &mut Context<Self>) {
        self.nav.zoomed = Some(terminal.to_owned());
        self.nav.focused = Some(terminal.to_owned());
        self.nav.pending_focus = true;
        self.sync_panes(cx);
        cx.notify();
    }
    /// Focuses a terminal the way an unlinked row or another machine's card does.
    pub fn visual_focus_terminal(
        &mut self,
        terminal: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_terminal(terminal, window, cx);
    }
    pub fn visual_focus_state(&self, window: &Window, cx: &gpui::App) -> FocusState {
        let keyboard = self
            .layout
            .panes
            .iter()
            .find(|(_, p)| p.focus_handle(cx).is_focused(window))
            .map(|(id, _)| id.clone());
        let row = match &self.peek {
            Some(peek) => Some(peek.terminal.clone()),
            None => self
                .nav
                .focused
                .as_ref()
                .and_then(|id| self.current_agent(id))
                .and_then(|s| live_terminal(&self.workspace, s))
                .map(|t| t.id.clone()),
        };
        let mut shown: Vec<_> = self.layout.panes.keys().cloned().collect();
        shown.sort();
        FocusState {
            space: self.nav.space.clone(),
            tab: self.nav.tab.clone(),
            focused: self.nav.focused.clone(),
            keyboard,
            row,
            zoomed: self.nav.zoomed.clone(),
            shown,
        }
    }
}
