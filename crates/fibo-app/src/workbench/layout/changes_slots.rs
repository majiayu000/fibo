//! Each pane's 看改动 panel: opening, closing, and closing itself when its turn moves on.
use super::changes_panel::{COVERS, ChangesPanel, ChangesSlot};
#[cfg(feature = "visual-test")]
use super::changes_panel::{Detail, Shown};
use super::*;
#[cfg(feature = "visual-test")]
use fibo_core::turn_diff::{FileSummary, TurnChanges, TurnFile};

impl Workbench {
    /// What identifies the turn on `session`: a new prompt or a further edit makes it another.
    fn changes_key(&self, session: &AgentSession) -> (u64, u64) {
        (self.workspace.content.get(&session.id)).map_or((0, 0), |c| c.turn_key())
    }

    /// The panel to draw over `terminal`'s body, when it is open on this very turn.
    pub(in crate::workbench) fn changes_over(
        &self,
        terminal: &str,
        session: Option<&AgentSession>,
    ) -> Option<gpui::Div> {
        let slot = self.layout.changes.get(terminal).filter(|s| s.open)?;
        let key = session.map_or((0, 0), |s| self.changes_key(s));
        (slot.key == key).then(|| {
            let style = gpui::StyleRefinement {
                size: gpui::SizeRefinement {
                    width: Some(gpui::relative(1.).into()),
                    height: Some(gpui::relative(1.).into()),
                },
                ..Default::default()
            };
            div()
                .absolute()
                .when(slot.below, |d| d.bottom_0())
                .when(!slot.below, |d| d.top_0())
                .left_0()
                .right_0()
                .h(gpui::relative(COVERS))
                .child(slot.view.clone().cached(style))
        })
    }

    pub(in crate::workbench) fn changes_shown(&self, terminal: &str) -> bool {
        self.layout.changes.get(terminal).is_some_and(|s| s.open)
    }

    /// The 看改动 button.
    pub(in crate::workbench) fn toggle_changes(
        &mut self,
        terminal: &str,
        session: &AgentSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.changes_shown(terminal) {
            return self.close_changes(terminal, window, cx);
        }
        let Some(pane) = self.layout.panes.get(terminal).cloned() else {
            return;
        };
        // A terminal with little on it has its input line near the top, where the panel would
        // cover what you type. Decided on opening: it never moves under your eyes.
        let below = pane.read(cx).cursor_depth().is_some_and(|d| d < COVERS);
        let key = self.changes_key(session);
        let reuse = (self.layout.changes.get(terminal))
            .filter(|s| s.key == key)
            .map(|s| s.view.clone());
        let view = reuse.unwrap_or_else(|| {
            let workbench = cx.entity().downgrade();
            let (terminal, session) = (terminal.to_owned(), session.id.clone());
            let asked = self.store.send(Request::TurnChanges {
                session: session.clone(),
            });
            cx.new(|cx| {
                let mut panel = ChangesPanel::new(workbench, terminal, session, pane, cx);
                panel.read(asked, cx);
                panel
            })
        });
        window.focus(&view.read(cx).focus.clone(), cx);
        let open = ChangesSlot {
            key,
            open: true,
            below,
            view,
        };
        self.layout.changes.insert(terminal.to_owned(), open);
        cx.notify();
    }

    /// Closes it, keeping its place; the keyboard goes back to the terminal.
    pub(crate) fn close_changes(
        &mut self,
        terminal: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(slot) = self.layout.changes.get_mut(terminal) {
            slot.open = false;
        }
        if let Some(pane) = self.layout.panes.get(terminal) {
            window.focus(&pane.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Panels whose turn has moved on close, or whose pane has gone; a keyboard left in one
    /// goes back to its terminal. Run when the workspace changes, not on every frame.
    pub(in crate::workbench) fn settle_changes(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.layout.changes.is_empty() || self.layout.changes_seen == self.workspace.revision {
            return;
        }
        self.layout.changes_seen = self.workspace.revision;
        let panes = &self.layout.panes;
        self.layout
            .changes
            .retain(|terminal, _| panes.contains_key(terminal));
        let ids: Vec<String> = self.layout.changes.keys().cloned().collect();
        for id in ids {
            let current = self.pane_session(&id).map(|s| self.changes_key(s));
            let reviewing = self
                .pane_status(&id)
                .is_some_and(|(g, _)| g == Status::Review);
            let Some(slot) = self.layout.changes.get_mut(&id) else {
                continue;
            };
            // A key of nothing is the performance probe's: real turns have touched a file.
            let current_turn = reviewing && current == Some(slot.key);
            if slot.open && !current_turn && slot.key != (0, 0) {
                slot.open = false;
                if slot.view.read(cx).focus.contains_focused(window, cx) {
                    let pane = self.layout.panes.get(&id).map(|p| p.focus_handle(cx));
                    if let Some(pane) = pane {
                        window.focus(&pane, cx);
                    }
                }
            }
        }
    }
}

/// What the native evidence and the performance probe read and set.
#[cfg(feature = "visual-test")]
pub struct ChangesProbe {
    pub open: bool,
    pub below: bool,
    /// The panel itself has the keyboard, not the filter in it.
    pub focused: bool,
    pub filter_focused: bool,
    pub filter: bool,
    pub selected: usize,
    pub matches: usize,
    pub scroll: (usize, f32),
    pub files: Vec<FileSummary>,
    /// What was read of the selected file.
    pub file: Option<std::sync::Arc<TurnFile>>,
    pub details: usize,
    pub drew: u32,
}

#[cfg(feature = "visual-test")]
impl Workbench {
    pub fn visual_changes(&self, terminal: &str, window: &Window, cx: &gpui::App) -> ChangesProbe {
        let slot = self.layout.changes.get(terminal);
        let panel = slot.map(|s| s.view.read(cx));
        let shown = panel.and_then(|p| match p.details.get(&p.selected) {
            Some(Detail::Ready(shown)) => Some(shown),
            _ => None,
        });
        let filter = panel.and_then(|p| p.filter.as_ref());
        ChangesProbe {
            open: slot.is_some_and(|s| s.open),
            below: slot.is_some_and(|s| s.below),
            focused: panel.is_some_and(|p| p.focus.is_focused(window)),
            filter_focused: filter.is_some_and(|f| f.focus_handle(cx).is_focused(window)),
            filter: filter.is_some(),
            selected: panel.map_or(0, |p| p.selected),
            matches: panel.map_or(0, |p| p.matches.len()),
            scroll: shown.map_or((0, 0.), |s| {
                let top = s.list.logical_scroll_top();
                (top.item_ix, f32::from(top.offset_in_item))
            }),
            files: panel
                .and_then(|p| p.changes())
                .map_or(Vec::new(), |c| c.files.clone()),
            file: shown.map(|s| s.file.clone()),
            details: panel.map_or(0, |p| p.details.len()),
            drew: panel.map_or(0, |p| p.drew),
        }
    }

    /// Opens the panel over `terminal` holding `changes` with its first file read, whatever
    /// that pane runs.
    pub fn visual_changes_show(
        &mut self,
        terminal: &str,
        changes: TurnChanges,
        first: TurnFile,
        cx: &mut Context<Self>,
    ) {
        let pane = self.layout.panes.get(terminal).expect("pane").clone();
        let workbench = cx.entity().downgrade();
        let terminal = terminal.to_owned();
        let view = cx.new(|cx| {
            let mut panel = ChangesPanel::new(workbench, terminal.clone(), String::new(), pane, cx);
            // Already known, so choosing it asks the host for nothing.
            let rows = super::changes_rows::rows_of(&first);
            panel.details.insert(
                0,
                Detail::Ready(Shown {
                    list: gpui::ListState::new(rows.len(), gpui::ListAlignment::Top, px(T::ROW)),
                    file: std::sync::Arc::new(first),
                    rows: std::sync::Arc::new(rows),
                }),
            );
            panel.loaded(Ok(Reply::TurnChanges(changes)), cx);
            panel
        });
        let slot = ChangesSlot {
            key: (0, 0),
            open: true,
            below: false,
            view,
        };
        self.layout.changes.insert(terminal, slot);
        cx.notify();
    }

    /// Asks the panel over `terminal` to draw again, as scrolling it does.
    pub fn visual_changes_poke(&self, terminal: &str, cx: &mut Context<Self>) {
        if let Some(slot) = self.layout.changes.get(terminal) {
            slot.view.update(cx, |_, cx| cx.notify());
        }
    }

    /// How many files and lines this terminal's turn bar states.
    pub fn visual_turn_stated(&self, terminal: &str) -> Option<(usize, (u32, u32))> {
        let s = self.pane_session(terminal)?;
        let c = self.workspace.content.get(&s.id)?;
        Some((self.turn_files(s), c.turn_lines()))
    }
}
