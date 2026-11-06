//! Each pane's 看改动 panel: opening, closing, and closing itself when its turn moves on.
use super::changes_panel::{COVERS, ChangesPanel, ChangesSlot, Load};
use super::*;
#[cfg(feature = "visual-test")]
use mooring_core::turn_diff::{TurnChanges, TurnFile};

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
                let mut panel = ChangesPanel {
                    workbench,
                    terminal,
                    session,
                    pane,
                    focus: cx.focus_handle(),
                    load: Load::Reading { say: false },
                    selected: 0,
                    rows: Vec::new(),
                    lists: Vec::new(),
                    #[cfg(feature = "visual-test")]
                    drew: 0,
                };
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
                if slot.view.read(cx).focus.is_focused(window) {
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
    pub focused: bool,
    pub selected: usize,
    pub scroll: (usize, f32),
    pub files: Vec<TurnFile>,
    pub drew: u32,
}

#[cfg(feature = "visual-test")]
impl Workbench {
    pub fn visual_changes(&self, terminal: &str, window: &Window, cx: &gpui::App) -> ChangesProbe {
        let slot = self.layout.changes.get(terminal);
        let panel = slot.map(|s| s.view.read(cx));
        let scroll = panel
            .and_then(|p| p.lists.get(p.selected))
            .map_or((0, 0.), |l| {
                let top = l.logical_scroll_top();
                (top.item_ix, f32::from(top.offset_in_item))
            });
        ChangesProbe {
            open: slot.is_some_and(|s| s.open),
            below: slot.is_some_and(|s| s.below),
            focused: panel.is_some_and(|p| p.focus.is_focused(window)),
            selected: panel.map_or(0, |p| p.selected),
            scroll,
            files: panel.map_or(Vec::new(), |p| match &p.load {
                Load::Ready(c) => c.files.clone(),
                _ => Vec::new(),
            }),
            drew: panel.map_or(0, |p| p.drew),
        }
    }

    /// Opens the panel over `terminal` already holding `changes`, whatever that pane runs.
    pub fn visual_changes_show(
        &mut self,
        terminal: &str,
        changes: TurnChanges,
        cx: &mut Context<Self>,
    ) {
        let pane = self.layout.panes.get(terminal).expect("pane").clone();
        let workbench = cx.entity().downgrade();
        let terminal = terminal.to_owned();
        let view = cx.new(|cx| {
            let mut panel = ChangesPanel {
                workbench,
                terminal: terminal.clone(),
                session: String::new(),
                pane,
                focus: cx.focus_handle(),
                load: Load::Failed,
                selected: 0,
                rows: Vec::new(),
                lists: Vec::new(),
                drew: 0,
            };
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
}
