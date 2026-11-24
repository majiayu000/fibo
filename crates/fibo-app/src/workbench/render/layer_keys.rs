//! Keys while a menu or the close question is open over the panes.
//!
//! None of these layers takes the focus: the terminal behind keeps it, so without this
//! Esc reached the Agent and interrupted the very turn the question was about.
use super::*;

/// What a key does while a layer is open.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::workbench) enum LayerKey {
    /// No layer of ours is open, or one that reads its own keys is on top.
    Pass,
    /// The topmost layer closes; the key goes no further.
    Dismiss,
    /// Swallowed: the close question is open, and nothing typed reaches the terminal.
    Hold,
}

/// Esc closes the topmost layer. While the close question is open every other key stops
/// there too: Return does not pick 停止并关闭 for you, and ⌘W does not close a second
/// thing behind the first.
pub(in crate::workbench) fn layer_key(
    confirm: bool,
    menu: bool,
    keystroke: &gpui::Keystroke,
) -> LayerKey {
    let escape = keystroke.key == "escape" && !keystroke.modifiers.modified();
    match (confirm, menu, escape) {
        (true, _, true) | (false, true, true) => LayerKey::Dismiss,
        (true, _, false) => LayerKey::Hold,
        _ => LayerKey::Pass,
    }
}

impl Workbench {
    /// Settings, diagnostics and ⌘P hold the focus and close themselves on Esc.
    fn self_keyed_layer(&self) -> bool {
        self.settings.is_some() || self.diagnostics.is_some() || self.search.is_open()
    }
    pub(super) fn menu_open(&self) -> bool {
        self.layout.menu.is_some()
            || self.sidebar.view_menu.is_some()
            || self.phones.open
            || self.statusbar.is_some()
            || self.layout.icon_menu.is_some()
            || self.layout.tab_menu.is_some()
            || self.layout.session_menu.is_some()
            || self.layout.remote_menu.is_some()
            || self.layout.space_menu.is_some()
            || self.sidebar.machine_menu.is_some()
    }
    /// Closes the topmost open layer, in the reverse of the order `window_layers` draws
    /// them, so Esc takes back the last thing that appeared.
    pub(super) fn dismiss_top_layer(&mut self) {
        if self.layout.confirm_close.take().is_some()
            || self.sidebar.machine_menu.take().is_some()
            || self.layout.space_menu.take().is_some()
            || self.layout.session_menu.take().is_some()
            || self.layout.remote_menu.take().is_some()
            || self.layout.tab_menu.take().is_some()
            || self.layout.icon_menu.take().is_some()
            || self.statusbar.take().is_some()
        {
            return;
        }
        if self.phones.open {
            self.phones = Default::default();
        } else if self.sidebar.view_menu.take().is_none() {
            self.layout.menu = None;
        }
    }
    /// Runs on the way down from the window, before the focused terminal sees the key.
    pub(in crate::workbench) fn layer_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.self_keyed_layer() {
            return;
        }
        let fate = layer_key(
            self.layout.confirm_close.is_some(),
            self.menu_open(),
            &event.keystroke,
        );
        if fate == LayerKey::Dismiss {
            self.dismiss_top_layer();
            cx.notify();
        }
        if fate != LayerKey::Pass {
            cx.stop_propagation();
            window.prevent_default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LayerKey, layer_key};
    fn key(text: &str) -> gpui::Keystroke {
        gpui::Keystroke::parse(text).unwrap()
    }
    #[test]
    fn esc_closes_the_close_question_and_nothing_typed_reaches_the_terminal() {
        assert_eq!(layer_key(true, false, &key("escape")), LayerKey::Dismiss);
        for typed in ["enter", "a", "space", "ctrl-c", "cmd-w", "shift-escape"] {
            assert_eq!(
                layer_key(true, false, &key(typed)),
                LayerKey::Hold,
                "{typed}"
            );
        }
    }
    #[test]
    fn esc_closes_a_menu_and_other_keys_still_reach_the_terminal() {
        assert_eq!(layer_key(false, true, &key("escape")), LayerKey::Dismiss);
        assert_eq!(layer_key(false, true, &key("a")), LayerKey::Pass);
        assert_eq!(layer_key(false, true, &key("cmd-w")), LayerKey::Pass);
    }
    #[test]
    fn with_nothing_open_esc_belongs_to_the_terminal() {
        assert_eq!(layer_key(false, false, &key("escape")), LayerKey::Pass);
    }
}
