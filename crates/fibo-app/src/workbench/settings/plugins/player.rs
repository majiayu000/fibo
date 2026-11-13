//! Which stage plays over the plugin page, kept in step with what is pointed at and opened.
use super::*;

impl SettingsView {
    /// The player, while something plays: settings is then reused frame to frame.
    pub(in crate::workbench) fn playing(
        &self,
    ) -> Option<Entity<crate::workbench::complete::Player>> {
        (self.page == Page::Plugins && self.market.play_id().is_some())
            .then(|| self.market.player.clone())
            .flatten()
    }
    /// Tells the player what the page now wants played.
    pub(in crate::workbench) fn sync_player(&mut self, cx: &mut Context<Self>) {
        use crate::workbench::complete::Play;
        let items = catalog::items(&self.market.offers());
        let of = |id: &str| items.iter().find(|i| i.id == id).and_then(|i| i.complete);
        let now = match (&self.market.hover, &self.market.open, self.market.opened) {
            (Some((id, from)), ..) => of(id).map(|c| (c, Play::Loop(*from))),
            (None, Some(id), Some(from)) => of(id).map(|c| (c, Play::Once(from))),
            _ => None,
        };
        if let Some(player) = &self.market.player {
            player.update(cx, |p, cx| {
                p.now = now;
                cx.notify();
            });
        }
        cx.notify();
    }
}
