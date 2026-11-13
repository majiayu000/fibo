//! Switching tabs and moving input between panes, and the workbench's keymap actions.
use super::*;

impl Workbench {
    /// Shows a tab of the current space and puts input in its focused pane.
    pub(in crate::workbench) fn show_tab(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav.tab = Some(id.to_owned());
        if self.nav.overview || !self.overview.cleanup.is_empty() {
            self.leave_overview(cx);
        }
        self.sync_panes(cx);
        if let Some(p) = self
            .nav
            .focused
            .as_ref()
            .and_then(|id| self.layout.panes.get(id))
        {
            window.focus(&p.focus_handle(cx), cx);
        }
        cx.notify();
    }
    /// The workbench's keymap actions (crate::keymap); true when one ran. The focused
    /// terminal runs its own (查找、清屏、字号) before this sees them.
    pub(in crate::workbench) fn shortcut(
        &mut self,
        action: crate::keymap::Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use crate::keymap::Action;
        let row: Vec<_> = self
            .workspace
            .tabs
            .iter()
            .filter(|t| t.space == self.nav.space)
            .map(|t| t.id.clone())
            .collect();
        let at = self
            .nav
            .tab
            .as_ref()
            .and_then(|t| row.iter().position(|r| r == t));
        let target = match action {
            Action::NewTab => {
                self.launch(false, None, cx);
                return true;
            }
            Action::Close => {
                self.close_focused(cx);
                return true;
            }
            Action::SplitRight | Action::SplitDown => {
                let pane = self.nav.tab.as_ref().and_then(|tab| {
                    let panes = self.workspace.tab_terminals(tab);
                    self.nav
                        .focused
                        .clone()
                        .filter(|id| panes.contains(id))
                        .or_else(|| panes.first().cloned())
                });
                match pane {
                    Some(id) => {
                        let label = if action == Action::SplitDown {
                            tk!("向下分屏")
                        } else {
                            tk!("向右分屏")
                        };
                        self.menu_action(&id, label, window, cx);
                    }
                    None => self.launch(false, None, cx),
                }
                return true;
            }
            Action::Search => {
                if self.search.is_open() {
                    self.close_search(cx);
                } else {
                    self.open_search(window, cx);
                }
                return true;
            }
            Action::Settings => {
                self.show_settings(window, cx);
                return true;
            }
            Action::NextWaiting => {
                self.next_waiting(window, cx);
                return true;
            }
            Action::ToggleSidebar => {
                self.toggle_sidebar(cx);
                return true;
            }
            Action::PrevPane | Action::NextPane => {
                self.focus_neighbour(action == Action::NextPane, window, cx);
                return true;
            }
            Action::PaneLeft | Action::PaneRight | Action::PaneUp | Action::PaneDown => {
                use fibo_core::layout::Direction;
                let to = match action {
                    Action::PaneLeft => Direction::Left,
                    Action::PaneRight => Direction::Right,
                    Action::PaneUp => Direction::Up,
                    _ => Direction::Down,
                };
                self.focus_toward(to, window, cx);
                return true;
            }
            Action::PrevTab => at.map(|i| (i + row.len() - 1) % row.len()),
            Action::NextTab => at.map(|i| (i + 1) % row.len()),
            Action::LastTab => row.len().checked_sub(1),
            Action::Tab(n) => Some(usize::from(n) - 1).filter(|i| *i < row.len()),
            _ => return false,
        };
        if let Some(id) = target.and_then(|i| row.get(i)).cloned() {
            self.show_tab(&id, window, cx);
        }
        true
    }
    /// ⌘[ / ⌘]: input moves to the previous or next pane of the tab in layout order,
    /// wrapping around, as Ghostty's goto_split does; a zoomed pane is restored first.
    fn focus_neighbour(&mut self, next: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.nav.tab.clone() else {
            return;
        };
        let panes: Vec<_> = self
            .workspace
            .tab_terminals(&tab)
            .into_iter()
            .filter(|id| self.layout.panes.contains_key(id))
            .collect();
        if panes.len() < 2 {
            return;
        }
        let at = self
            .nav
            .focused
            .as_ref()
            .and_then(|id| panes.iter().position(|p| p == id))
            .unwrap_or(0);
        let to = if next {
            (at + 1) % panes.len()
        } else {
            (at + panes.len() - 1) % panes.len()
        };
        self.focus_pane(panes[to].clone(), window, cx);
    }
    /// ⌘⌥ and an arrow: input moves to the pane on that side of the focused one, by
    /// where the panes sit; nothing happens at the tab's edge.
    fn focus_toward(
        &mut self,
        to: fibo_core::layout::Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.nav.tab.clone() else {
            return;
        };
        let Some(layout) = self
            .workspace
            .tabs
            .iter()
            .find(|t| t.id == tab)
            .and_then(|t| t.layout.clone())
        else {
            return;
        };
        let from = self
            .nav
            .focused
            .clone()
            .filter(|id| layout.contains(id))
            .or_else(|| layout.ids().first().cloned());
        if let Some(id) = from
            .and_then(|from| layout.neighbour(&from, to))
            .filter(|id| self.layout.panes.contains_key(id))
        {
            self.focus_pane(id, window, cx);
        }
    }
    fn focus_pane(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.zoomed = None;
        self.nav.focused = Some(id.clone());
        self.sync_panes(cx);
        if let Some(pane) = self.layout.panes.get(&id) {
            window.focus(&pane.focus_handle(cx), cx);
        }
        cx.notify();
    }
}
