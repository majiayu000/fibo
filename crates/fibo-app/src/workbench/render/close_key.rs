//! ⌘W, pressed or picked from 文件 → 关闭窗格: what lies over the panes closes first, as a
//! Mac app's ⌘W closes its sheet before the window; the focused terminal only when nothing
//! is over it.
use super::*;

/// What ⌘W closes now.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::workbench) enum CloseTarget {
    Settings,
    Diagnostics,
    Search,
    /// The close question is open: ⌘W does not close a second thing behind it.
    Nothing,
    Menu,
    Peek,
    Form,
    Overview,
    Pane,
}

/// What is open over the panes.
#[derive(Default)]
pub(in crate::workbench) struct Over {
    pub settings: bool,
    pub diagnostics: bool,
    pub search: bool,
    pub confirm: bool,
    pub menu: bool,
    pub peek: bool,
    pub form: bool,
    pub overview: bool,
}

/// The topmost open thing, in the reverse of the order the window draws them
/// (`window_layers`, then the peek over the main area, then the page in it).
pub(in crate::workbench) fn close_target(over: &Over) -> CloseTarget {
    [
        (over.settings, CloseTarget::Settings),
        (over.diagnostics, CloseTarget::Diagnostics),
        (over.search, CloseTarget::Search),
        (over.confirm, CloseTarget::Nothing),
        (over.menu, CloseTarget::Menu),
        (over.peek, CloseTarget::Peek),
        (over.form, CloseTarget::Form),
        (over.overview, CloseTarget::Overview),
    ]
    .into_iter()
    .find_map(|(open, target)| open.then_some(target))
    .unwrap_or(CloseTarget::Pane)
}

impl Workbench {
    fn over(&self) -> Over {
        Over {
            settings: self.settings.is_some(),
            diagnostics: self.diagnostics.is_some(),
            search: self.search.is_open(),
            confirm: self.layout.confirm_close.is_some(),
            menu: self.menu_open(),
            peek: self.peek.is_some(),
            form: self.tasks.form.is_some(),
            overview: self.nav.overview || !self.overview.cleanup.is_empty(),
        }
    }
    /// The keymap's 关闭 (Action::Close), from the key or the menu bar.
    pub(in crate::workbench) fn close_key(&mut self, cx: &mut Context<Self>) {
        match close_target(&self.over()) {
            CloseTarget::Settings => {
                self.settings = None;
                self.nav.pending_focus = true;
            }
            CloseTarget::Diagnostics => self.hide_diagnostics(cx),
            CloseTarget::Search => self.close_search(cx),
            CloseTarget::Nothing => {}
            CloseTarget::Menu => self.dismiss_top_layer(),
            CloseTarget::Peek => self.close_peek(cx),
            CloseTarget::Form => {
                self.close_form();
                self.nav.pending_focus = true;
            }
            CloseTarget::Overview => self.leave_overview(cx),
            CloseTarget::Pane => self.close_focused(cx),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{CloseTarget, Over, close_target};

    #[test]
    fn cmd_w_closes_what_is_over_the_panes_before_the_terminal() {
        assert_eq!(close_target(&Over::default()), CloseTarget::Pane);
        let only = |set: fn(&mut Over)| {
            let mut over = Over::default();
            set(&mut over);
            close_target(&over)
        };
        assert_eq!(only(|o| o.search = true), CloseTarget::Search);
        assert_eq!(only(|o| o.form = true), CloseTarget::Form);
        assert_eq!(only(|o| o.overview = true), CloseTarget::Overview);
        assert_eq!(only(|o| o.settings = true), CloseTarget::Settings);
        assert_eq!(only(|o| o.diagnostics = true), CloseTarget::Diagnostics);
        assert_eq!(only(|o| o.menu = true), CloseTarget::Menu);
        assert_eq!(only(|o| o.peek = true), CloseTarget::Peek);
        assert_eq!(only(|o| o.confirm = true), CloseTarget::Nothing);
        // Topmost first, whatever else is open beneath.
        let all = Over {
            settings: true,
            diagnostics: true,
            search: true,
            confirm: true,
            menu: true,
            peek: true,
            form: true,
            overview: true,
        };
        assert_eq!(close_target(&all), CloseTarget::Settings);
        let palette_over_board = Over {
            search: true,
            overview: true,
            ..Over::default()
        };
        assert_eq!(close_target(&palette_over_board), CloseTarget::Search);
        let form_over_board = Over {
            form: true,
            overview: true,
            ..Over::default()
        };
        assert_eq!(close_target(&form_over_board), CloseTarget::Form);
    }
}
