//! What the performance probe drives: the animation cards, as a person pointing at them would.
use super::*;
use crate::workbench::complete::stage_probe;
use fibo_core::plugin_category::Category;
use std::sync::atomic::Ordering;

impl Workbench {
    /// 发现 narrowed to the animations, with `hover` pointed at.
    pub(crate) fn visual_plugins_grid(&mut self, hover: Option<&str>, cx: &mut Context<Self>) {
        let view = self.settings.clone().expect("settings open");
        view.update(cx, |v, cx| {
            v.market.tab = Tab::Discover;
            v.market.category = Some(Category::Effects);
            v.market.open = None;
            v.market.hover = hover.map(|id| (id.to_owned(), std::time::Instant::now()));
            v.sync_player(cx);
        });
    }
    /// What a change on the page does: settings draws again.
    pub(crate) fn visual_plugins_change(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.settings.clone() {
            view.update(cx, |_, cx| cx.notify());
        }
    }
    /// What a playing stage's request for the next frame does: only the player is notified.
    pub(crate) fn visual_plugins_frame(&mut self, cx: &mut Context<Self>) {
        let player = self
            .settings
            .as_ref()
            .and_then(|view| view.read(cx).market.player.clone());
        if let Some(player) = player {
            player.update(cx, |_, cx| cx.notify());
        }
    }
    pub(crate) fn visual_stage_hold(t: Option<f32>) {
        stage_probe::hold(t);
    }
    /// How many frames stages have asked for, reset by reading.
    pub(crate) fn visual_stage_asked() -> usize {
        stage_probe::ASKED.swap(0, Ordering::Relaxed)
    }
}
