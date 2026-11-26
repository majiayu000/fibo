//! The InputOwner fixture's window going out of use and back, and its pane made again the
//! way a split opened on another machine reaches it (`sync_panes`).
use super::*;

impl Workbench {
    pub fn visual_set_window_away(&mut self, away: bool) {
        self.hosts.window_away = away;
    }
    /// Drops the shown tab's first pane and lets `sync_panes` make it again.
    pub fn visual_remake_shown_pane(&mut self, cx: &mut Context<Self>) {
        let tab = self.nav.tab.clone().expect("a shown tab");
        let id = self.workspace.tab_terminals(&tab)[0].clone();
        self.layout.panes.remove(&id);
        self.sync_panes(cx);
    }
}
