//! Another machine's session row, drawn by `flat_row` from that machine's own workbench
//! here, as this machine's rows are from this one: what is added for the other machine
//! (its mark, 断开, 先不管) and where its clicks go.
use super::*;
use crate::workbench::diagnostics::TipLine;
use crate::workbench::sidebar_list::{End, SessionAction};

impl RemoteRow {
    /// Its terminal on its machine, which keys its peek, menu and folds.
    pub(in crate::workbench) fn key(&self) -> &str {
        &self.key
    }
    /// Its terminal's id on its machine, which that machine's workbench acts on.
    pub(in crate::workbench) fn terminal(&self) -> &str {
        &self.card.id
    }
    pub(in crate::workbench) fn quiet(&self) -> bool {
        self.quiet
    }
    /// Dropped off: faded, says 断开, opens nothing.
    pub(in crate::workbench) fn dropped(&self) -> bool {
        !self.online && !self.quiet
    }
}

impl Workbench {
    /// `r` drawn by `flat_row` from its machine's workbench, by the same rules as this
    /// machine's rows; None when that machine names no session for it.
    pub(super) fn shared_remote_row(
        &self,
        r: &RemoteRow,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let view = self.machine_view(r.index)?;
        let w = view.read(cx);
        let id = r.card.session.as_deref()?;
        let s = w.workspace.agent_sessions.iter().find(|s| s.id == id)?;
        let (g, attention) =
            crate::model::session_state::session_status(&w.workspace, &w.attention.unseen, s);
        let detail = self.listed_detail(w, &(s, g, attention), r.shown);
        Some(self.flat_row(w, Some(r), (s, g, attention), None, detail, End::State, cx))
    }
    /// Its machine's square, when this list holds more than one machine.
    pub(in crate::workbench) fn there_mark(&self, r: &RemoteRow) -> Option<gpui::Div> {
        (!r.quiet && self.machine_layout() == Machines::Mixed && self.machine_scope().is_none())
            .then(|| machine_mark(machine_letter(&r.machine), !r.online))
    }
    /// Its space's tag, gray on a machine put aside.
    pub(in crate::workbench) fn there_tag(&self, r: &RemoteRow, id: &str, name: &str) -> gpui::Div {
        if r.quiet {
            quiet::quiet_tag(name)
        } else {
            self.space_tag(id, name)
        }
    }
    /// The hover card while its machine is not connected.
    pub(in crate::workbench) fn there_tip(&self, r: &RemoteRow) -> Option<Vec<TipLine>> {
        let text = match (&r.why, r.online) {
            (_, true) => return None,
            _ if r.quiet => t!("{machine} 现在没连上", machine = r.machine),
            (Some(why), _) => t!("{machine} 没连上：{why}", machine = r.machine, why),
            (None, false) => t!("正在连 {machine}", machine = r.machine),
        };
        Some(vec![TipLine::Title(text)])
    }
    /// A click on its row or a branch under it: open it here, or rename it on a double
    /// click, both by its machine's workbench.
    pub(in crate::workbench) fn click_there(
        &mut self,
        index: usize,
        terminal: &str,
        rename: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = if rename {
            SessionAction::Rename
        } else {
            SessionAction::Open
        };
        self.remote_action(index, terminal, &action, window, cx);
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// Whether machine `index`'s row for `terminal` is drawn by `flat_row` from that
    /// machine's workbench, not from its card alone.
    /// A plain click on machine `index`'s row for `terminal`, as its row's click does.
    pub fn visual_click_remote_row(
        &mut self,
        index: usize,
        terminal: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row = (self.remote_rows_of(index).into_iter()).find(|r| r.card.id == terminal);
        if let Some(r) = row {
            self.click_there(r.index, r.terminal(), false, window, cx);
        }
    }
    pub fn visual_row_shared(&self, index: usize, terminal: &str, cx: &Context<Self>) -> bool {
        (self.remote_rows_of(index).iter())
            .filter(|r| r.card.id == terminal)
            .any(|r| self.shared_remote_row(r, cx).is_some())
    }
}
