//! Which sidebar rows are lit, read as the rows are drawn: each compares its key with
//! `in_front`, this machine's rows by session (or terminal), another machine's by its key.
use super::*;

impl Workbench {
    /// The keys of the rows lit now, this machine's first.
    pub fn visual_lit_rows(&self, cx: &gpui::App) -> Vec<String> {
        let here = self.in_front(cx);
        let mut lit: Vec<String> = (self.workspace.agent_sessions.iter())
            .filter(|s| self.session_hidden(s).is_none())
            .map(|s| s.id.clone())
            .chain(self.unlinked_in(|_| true).into_iter().map(|u| u.terminal))
            .filter(|key| here.as_ref() == Some(key))
            .collect();
        for index in 0..self.machines.len() {
            let rows = self.session_rows_of(index, cx);
            lit.extend(rows.iter().filter(|r| r.shown).map(|r| r.key().to_owned()));
            let unlinked = self.remote_unlinked(index, None, cx).into_iter();
            lit.extend(
                unlinked
                    .filter_map(|u| u.there.map(|(_, key)| key))
                    .filter(|key| here.as_ref() == Some(key)),
            );
        }
        lit
    }
    /// The key of machine `index`'s row for its terminal `terminal`.
    pub fn visual_remote_row_key(&self, index: usize, terminal: &str, cx: &gpui::App) -> String {
        (self.session_rows_of(index, cx).iter())
            .find(|r| r.target() == terminal)
            .map(|r| r.key().to_owned())
            .unwrap_or_else(|| panic!("machine {index} lists no row for {terminal}"))
    }
    /// The session id of the conversation with this CLI id.
    pub fn visual_session_id(&self, cli_id: &str) -> String {
        self.visual_session_of(cli_id).id
    }
    /// 「← 本机」, as the window takes it.
    pub fn visual_show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_home(window, cx);
    }
}
