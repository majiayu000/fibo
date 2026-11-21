//! The menu bar's items that act on this window (crate::app_menu).
use super::*;
use crate::app_menu::{About, Minimize, RunKey, ShowKeys, Zoom};
use gpui::App;

impl Workbench {
    /// Where input is in this workbench (a hosted machine's included), it takes the item
    /// first; `install` covers the window with nothing focused.
    pub(in crate::workbench) fn menu_bar_handlers(
        &self,
        root: gpui::Div,
        cx: &Context<Self>,
    ) -> gpui::Div {
        root.on_action(cx.listener(|this, a: &RunKey, window, cx| this.menu_key(a.0, window, cx)))
            .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
            .on_action(cx.listener(|_, _: &Zoom, window, _| window.zoom_window()))
            .on_action(cx.listener(|this, _: &About, window, cx| {
                this.show_settings_page(settings::Page::About, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowKeys, window, cx| {
                this.show_settings_page(settings::Page::Keys, window, cx)
            }))
    }
    /// A keymap item that reached the workbench: input was not in a terminal, or the action
    /// is the workbench's. 字号 and 滚动 still go to the pane that last had input; copy and
    /// paste never do, so a click can't type into an Agent you aren't looking at.
    fn menu_key(
        &mut self,
        action: crate::keymap::Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !action.in_terminal() {
            self.shortcut(action, window, cx);
            return;
        }
        let pane = self
            .nav
            .focused
            .as_ref()
            .and_then(|id| self.layout.panes.get(id));
        if let Some(pane) = pane.cloned() {
            pane.update(cx, |pane, cx| pane.run_key(action, window, cx));
        }
    }
}

/// With nothing focused GPUI asks only the window's root, which has no listeners, and the
/// items would stay gray: these take them there. They run a beat later, since the window
/// is still busy dispatching when they are called.
pub(crate) fn install(cx: &mut App) {
    cx.on_action(|a: &RunKey, cx| {
        let action = a.0;
        later(cx, move |w, window, cx| w.menu_key(action, window, cx));
    });
    cx.on_action(|_: &Minimize, cx| later(cx, |_, window, _| window.minimize_window()));
    cx.on_action(|_: &Zoom, cx| later(cx, |_, window, _| window.zoom_window()));
    cx.on_action(|_: &About, cx| {
        later(cx, |w, window, cx| {
            w.show_settings_page(settings::Page::About, window, cx)
        })
    });
    cx.on_action(|_: &ShowKeys, cx| {
        later(cx, |w, window, cx| {
            w.show_settings_page(settings::Page::Keys, window, cx)
        })
    });
}
fn later(
    cx: &mut App,
    run: impl FnOnce(&mut Workbench, &mut Window, &mut Context<Workbench>) + 'static,
) {
    let target = (cx.active_window().and_then(|w| w.downcast::<Workbench>()))
        .or_else(|| cx.windows().iter().find_map(|w| w.downcast::<Workbench>()));
    let Some(target) = target else { return };
    cx.defer(move |cx| {
        if let Err(error) = target.update(cx, run) {
            eprintln!("fibo: menu item: {error}");
        }
    });
}
