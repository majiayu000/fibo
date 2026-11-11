//! Closing a pane, a tab, an Agent or the host, with a confirmation when it would cost work.
use super::*;

impl Workbench {
    /// Closing a pane stops its terminal; its tab closes with the last pane.
    pub(super) fn close_pane(&mut self, id: &str, cx: &mut Context<Self>) {
        self.request_close(Closing::Pane(id.to_owned()), cx);
    }
    /// Closing a tab removes it and stops every terminal in it.
    pub(super) fn close_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        self.request_close(Closing::Tab(id.to_owned()), cx);
    }
    /// Cmd+W: the focused pane of the current tab, else the current tab.
    pub(super) fn close_focused(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.nav.tab.clone() else {
            return;
        };
        match self
            .nav
            .focused
            .clone()
            .filter(|id| self.workspace.tab_terminals(&tab).contains(id))
        {
            Some(pane) => self.close_pane(&pane, cx),
            None => self.close_tab(&tab, cx),
        }
    }
    /// Terminals of `closing` with an Agent still running in them.
    /// Each live Agent the close would stop: its title and what it is doing now.
    pub(super) fn running_agents(&self, closing: &Closing) -> Vec<(String, Option<Phase>)> {
        let ids = match closing {
            Closing::Tab(id) => self.workspace.tab_terminals(id),
            Closing::Pane(id) | Closing::Agent(id) | Closing::Unmatched(id) => vec![id.clone()],
            Closing::Host => self.running_terminals(),
        };
        ids.into_iter()
            .filter(|id| {
                self.workspace
                    .terminals
                    .iter()
                    .any(|t| &t.id == id && t.state == ProcessState::Running)
            })
            .filter_map(|id| self.current_agent(&id))
            .map(|s| {
                (
                    self.workspace.session_title(s),
                    live_phase(&self.workspace, s),
                )
            })
            .collect()
    }
    /// Terminals whose program is still running, Agent or not.
    pub(super) fn running_terminals(&self) -> Vec<String> {
        self.workspace
            .terminals
            .iter()
            .filter(|t| t.state == ProcessState::Running)
            .map(|t| t.id.clone())
            .collect()
    }
    pub(super) fn request_close(&mut self, closing: Closing, cx: &mut Context<Self>) {
        self.layout.menu = None;
        self.layout.tab_menu = None;
        self.layout.session_menu = None;
        self.layout.space_menu = None;
        let quiet = match closing {
            // A host that hands its terminals over ends none of them: nothing to confirm.
            Closing::Host => {
                self.running_terminals().is_empty()
                    || crate::lifecycle::stale_build().is_some_and(|b| b.handoff)
            }
            // An Agent whose turn has ended loses nothing it can't resume: only work in
            // progress or a question waiting for an answer is worth a confirmation, and
            // so is a tab of several splits, which a stray click would undo at once.
            _ => {
                splits_closed(&self.workspace, &closing) == 0
                    && !interrupts(&self.running_agents(&closing))
            }
        };
        if quiet {
            self.close_now(closing, cx);
        } else {
            self.layout.confirm_close = Some(closing);
            cx.notify();
        }
    }
    /// Stops the older host off the UI thread and quits; the app reopens with its own host.
    fn restart_host(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.store.socket().parent().map(Path::to_path_buf) else {
            return;
        };
        // Recorded while the old host still answers; the release hook runs after it is gone.
        record_left(self.store.socket(), self.away.unwrap_or_else(now));
        self.restarting = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { crate::lifecycle::restart_host(&root) })
                .await;
            match result {
                Ok(()) => {
                    cx.update(|cx| cx.quit());
                }
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        this.restarting = false;
                        this.error = Some(t!("重启后台失败：{e}", e));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
    pub(super) fn close_now(&mut self, closing: Closing, cx: &mut Context<Self>) {
        self.layout.confirm_close = None;
        // Only the Agent ends; its pane stays where it is, zoomed or not.
        if let Closing::Agent(id) = closing {
            return self.request(Request::StopAgent { id }, cx);
        }
        self.nav.zoomed = None;
        let (tab, edit) = match closing {
            Closing::Host => return self.restart_host(cx),
            Closing::Agent(_) => return,
            // What the user agreed to end: refused if an Agent split it meanwhile.
            Closing::Tab(id) => {
                let expect = Some(self.workspace.tab_terminals(&id));
                (Some(id.clone()), Edit::CloseTab { id, expect })
            }
            Closing::Pane(terminal) | Closing::Unmatched(terminal) => (
                self.workspace
                    .terminal_tab(&terminal)
                    .filter(|t| t.layout.as_ref().is_some_and(|l| l.ids().len() == 1))
                    .map(|t| t.id.clone()),
                Edit::ClosePane { terminal },
            ),
        };
        // The neighbour to the right takes over, like a browser; else the one to the left.
        if let Some(tab) = tab.filter(|t| self.nav.tab.as_ref() == Some(t)) {
            let row: Vec<_> = self
                .workspace
                .tabs
                .iter()
                .filter(|t| t.space == self.nav.space)
                .map(|t| t.id.clone())
                .collect();
            let at = row.iter().position(|t| *t == tab).unwrap_or(0);
            self.nav.tab = row
                .get(at + 1)
                .or_else(|| at.checked_sub(1).and_then(|i| row.get(i)))
                .cloned();
            self.nav.focused = None;
            self.nav.pending_focus = true;
            self.sync_panes(cx);
        }
        self.edit(edit, cx);
    }
}
