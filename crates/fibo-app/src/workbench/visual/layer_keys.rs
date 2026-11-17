//! The layer keys evidence: what a pane under the close question or a menu receives.
use super::*;

impl Workbench {
    /// The pane holding input focus.
    pub fn visual_focused_pane(&self, window: &Window, cx: &gpui::App) -> Option<Entity<Pane>> {
        self.layout
            .panes
            .values()
            .find(|p| p.focus_handle(cx).is_focused(window))
            .cloned()
    }
    /// The shown tab's menu, as a right click on it opens it.
    pub fn visual_open_tab_menu(&mut self, cx: &mut Context<Self>) {
        let tab = self.nav.tab.clone().expect("a shown tab");
        self.layout.tab_menu = Some((tab, Default::default()));
        cx.notify();
    }
    /// The close question and the tab menu: whether each is open.
    pub fn visual_layers(&self) -> (bool, bool) {
        (
            self.layout.confirm_close.is_some(),
            self.layout.tab_menu.is_some(),
        )
    }
}
