//! The InputOwner fixture's pane made again the way a tab switch or a split opened on
//! another machine makes it (`sync_panes`).
use super::*;

impl Workbench {
    /// Drops the shown tab's first pane and lets `sync_panes` make it again.
    pub fn visual_remake_shown_pane(&mut self, cx: &mut Context<Self>) {
        let tab = self.nav.tab.clone().expect("a shown tab");
        let id = self.workspace.tab_terminals(&tab)[0].clone();
        self.layout.panes.remove(&id);
        self.sync_panes(cx);
    }
}
