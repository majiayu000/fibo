//! Matching the shown panes to the current tab.
use super::*;

impl Workbench {
    pub(super) fn sync_panes(&mut self, cx: &mut Context<Self>) {
        self.drop_moved_dock();
        let ids = if self.nav.overview {
            Vec::new()
        } else {
            self.workspace
                .tabs
                .iter()
                .find(|t| Some(&t.id) == self.nav.tab.as_ref())
                .map(|t| self.workspace.tab_terminals(&t.id))
                .unwrap_or_default()
        };
        if self.nav.zoomed.as_ref().is_some_and(|id| !ids.contains(id)) {
            self.nav.zoomed = None;
        }
        let ids = if let Some(id) = &self.nav.zoomed {
            vec![id.clone()]
        } else {
            ids
        };
        // A shell the host lost on restart comes back in place as soon as its tab shows.
        if !self.store.busy()
            && self.resuming.resume.is_none()
            && self.tasks.chosen_agent.is_none()
            && let Some(id) = reopen_candidate(&self.workspace, &ids, &self.layout.reopened)
        {
            self.layout.reopened.insert(id.clone());
            if self.drop_lost_pane(&id, cx) {
                return;
            }
            self.reopen(id, cx);
        }
        let ids = attachable_ids(&self.workspace, ids);
        self.drop_shown_peek(&ids);
        if self.nav.focused.as_ref().is_none_or(|id| !ids.contains(id)) {
            self.nav.focused = ids.first().cloned();
            self.nav.pending_focus = true;
        }
        self.layout.panes.retain(|id, _| ids.contains(id));
        self.prune_pins();
        let split = ids.len() > 1;
        for id in ids {
            if !self.layout.panes.contains_key(&id) {
                let pane = cx.new(|cx| Pane::new(self.store.socket(), &id, false, cx));
                Self::adopt_pane(&pane, cx);
                self.layout.panes.insert(id, pane);
            }
        }
        for pane in self.layout.panes.values() {
            pane.update(cx, |p, cx| {
                if p.split != split {
                    p.split = split;
                    cx.notify();
                }
            });
        }
        let live: Vec<_> = self
            .workspace
            .agent_sessions
            .iter()
            .filter_map(|s| {
                let shown = s
                    .terminal
                    .as_ref()
                    .is_some_and(|t| self.layout.panes.contains_key(t));
                live_phase(&self.workspace, s).map(|p| (s.id.clone(), p, shown))
            })
            .collect();
        track_unseen(&mut self.attention.phases, &mut self.attention.unseen, live);
        self.mark_pane_agents(cx);
    }
}
