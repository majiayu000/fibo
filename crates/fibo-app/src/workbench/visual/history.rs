use super::*;
use std::cell::Cell;

thread_local! {
    static FULL: Cell<bool> = const { Cell::new(false) };
    static SKIPPED: Cell<usize> = const { Cell::new(0) };
}

pub(in crate::workbench) fn full_history() -> bool {
    FULL.get()
}

pub(in crate::workbench) fn skipped_history() {
    SKIPPED.set(SKIPPED.get() + 1);
}

impl Workbench {
    pub(crate) fn visual_full_history(full: bool) {
        FULL.set(full);
        SKIPPED.set(0);
    }

    pub(crate) fn visual_offscreen_history() -> usize {
        SKIPPED.get()
    }

    pub(crate) fn visual_history_waiting(w: &mut Workspace, active: &str) {
        let card = std::sync::Arc::make_mut(w.content.get_mut(active).unwrap());
        card.turn = fibo_core::content::Turn::Waiting;
        card.waiting = "The probe needs your approval".into();
    }

    pub(crate) fn visual_history_fixture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Workspace, String, String) {
        use crate::machines::{Remote, Spec};
        self.machines.push(Remote::fixed(
            Spec {
                quiet: true,
                ..Spec::new("fixture-other-machine".into())
            },
            vec![],
            vec![],
        ));
        self.visual_choose_preset("machine-fold", cx);
        let tab = self.workspace.terminal_tab("p0").unwrap().clone();
        self.nav.space = tab.space;
        self.show_tab(&tab.id, window, cx);
        let mut w = self.workspace.clone();
        let active = w.current_agent("p0").unwrap().id.clone();
        let terminal = w.terminals.iter().find(|t| t.id == "p0").unwrap().clone();
        let at = now();
        let mut latest = None;
        for (n, session) in w
            .agent_sessions
            .iter_mut()
            .filter(|s| s.terminal.is_none())
            .take(50)
            .enumerate()
        {
            let id = format!("history-fixture-{n}");
            session.terminal = Some(id.clone());
            session.current = false;
            session.outcome = Outcome::Completed;
            session.updated = at.saturating_sub(60 + n as u64 * 60);
            session.name = format!("History row {}", n + 1);
            let card = std::sync::Arc::make_mut(w.content.get_mut(&session.id).unwrap());
            card.interactive = true;
            card.hidden_reason.clear();
            card.updated = session.updated;
            card.turn = fibo_core::content::Turn::Completed;
            card.last_reply = "First summary from the fixture".into();
            w.terminals.push(Terminal {
                id,
                state: ProcessState::Exited,
                pid: None,
                exit_code: Some(0),
                ..terminal.clone()
            });
            latest.get_or_insert_with(|| session.id.clone());
        }
        self.apply_workspace(w.clone(), cx);
        cx.notify();
        (w, active, latest.unwrap())
    }

    pub(crate) fn visual_history_apply(&mut self, mut w: Workspace, cx: &mut Context<Self>) {
        w.revision = w.revision.max(self.workspace.revision);
        self.apply_workspace(w, cx);
        cx.notify();
    }

    pub(crate) fn visual_history_expand(&mut self, id: &str, cx: &mut Context<Self>) {
        self.toggle_done(id, cx);
    }
}
