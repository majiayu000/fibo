//! Settings over the window. While a plugin's stage plays, settings is a cached view: the
//! stage asks for every frame and nothing else on the page changes.
use super::*;

impl Workbench {
    pub(in crate::workbench) fn settings_overlay(
        &self,
        view: Entity<SettingsView>,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let player = view.read(cx).playing();
        let page = gpui::AnyView::from(view);
        let page = match &player {
            Some(_) => page
                .cached(
                    gpui::StyleRefinement::default()
                        .w(px(T::SETTINGS_WIDTH))
                        .max_w_full()
                        .h(px(PANEL_HEIGHT)),
                )
                .into_any_element(),
            None => page.into_any_element(),
        };
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .justify_center()
                .items_center()
                .p(px(T::S20))
                .bg(gpui::rgba(T::scrim()))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.settings = None;
                        this.nav.pending_focus = true;
                        cx.notify();
                    }),
                )
                .child(page)
                .children(player.map(|p| div().absolute().inset_0().child(p))),
        )
        .with_priority(2)
        .into_any_element()
    }
}
