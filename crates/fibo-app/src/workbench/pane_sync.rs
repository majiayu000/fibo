//! Matching the shown panes to the current tab.
use super::*;

/// The zoom a jump to `target` keeps: its own. Another pane zoomed shows alone, so
/// `sync_panes` would hand the keyboard back to it instead of the pane jumped to.
pub(super) fn zoom_kept(zoomed: Option<String>, target: &str) -> Option<String> {
    zoomed.filter(|z| z == target)
}

impl Workbench {
    pub(super) fn sync_panes(&mut self, cx: &mut Context<Self>) {
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
        // Another machine's workbench kept here undrawn shows no tab yet.
        let undrawn = self.undrawn();
        // A shell the host lost on restart comes back in place as soon as its tab shows.
        if !undrawn
            && !self.store.busy()
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
        self.prune_pins(cx);
        let split = ids.len() > 1;
        // Showing a terminal is not wanting its input: a pane made here writes only when
        // nobody else does. Tab switches, restores and a split synced from another machine's
        // window all make panes; typing in one is what takes the input (`take_input`).
        for id in ids {
            if !undrawn && !self.layout.panes.contains_key(&id) {
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
                let key = self
                    .workspace
                    .content
                    .get(&s.id)
                    .map(|c| c.turn_key.clone());
                live_phase(&self.workspace, s)
                    .map(|p| (s.id.clone(), p, shown, key.unwrap_or_default()))
            })
            .collect();
        let a = &mut self.attention;
        track_unseen(&mut a.phases, &mut a.seen, &mut a.unseen, live);
        self.mark_pane_agents(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::zoom_kept;

    #[test]
    fn a_jump_to_another_pane_ends_the_zoom_and_one_to_the_zoomed_pane_keeps_it() {
        assert_eq!(zoom_kept(Some("p0".into()), "p2"), None);
        assert_eq!(zoom_kept(Some("p2".into()), "p2"), Some("p2".into()));
        assert_eq!(zoom_kept(None, "p2"), None);
    }
}
