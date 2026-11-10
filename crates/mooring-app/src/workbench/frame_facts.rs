//! What every frame asks of all the sessions, worked out once per snapshot and second.
//!
//! Terminal output draws the title row, the tabs and the status bar each frame, and each
//! of them used to walk every session: on a long-used install that was about a third of
//! the frame. All of it depends only on the workspace, the current second and the
//! terminals being resumed, so nothing here shows later than a fresh count would. The
//! decisions stay in core; this only remembers their answers.
use super::*;
use rustc_hash::FxHashMap;
use std::{cell::RefCell, rc::Rc};

/// Everything a set of facts was worked out from. Debug builds work them out again on
/// each use and fail on a difference, so an input missing here cannot go unnoticed.
#[derive(Debug, PartialEq, Eq)]
struct Inputs {
    second: u64,
    revision: u64,
    /// Scenes edit the workspace in place; a snapshot from the host changes the revision
    /// or is dropped by `FactsCache::forget`.
    sizes: [usize; 4],
    resumed: HashMap<String, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FrameFacts {
    inputs: Inputs,
    /// A terminal's current sessions, in workspace order. Indexes into `agent_sessions`.
    current: FxHashMap<String, Vec<usize>>,
    /// The session each terminal ran latest, for a pane whose conversation has ended.
    latest: FxHashMap<String, usize>,
    needs_you: Vec<usize>,
    /// How many sessions the window lists, open in a tab and not hidden, in every space:
    /// 「本机 · N 个会话」.
    pub(super) listed: usize,
    /// Sessions in Mooring's terminals that work and that wait for your check.
    pub(super) working: usize,
    pub(super) review: usize,
    /// The CLIs with a conversation going on in one of our terminals, each once.
    pub(super) running: Vec<Agent>,
}

/// `Workbench::facts` keeps its answer here until an input changes.
#[derive(Default)]
pub(super) struct FactsCache(RefCell<Option<Rc<FrameFacts>>>);

impl FactsCache {
    /// A snapshot can change what a session shows without a new revision: the host lays
    /// what it reads off terminal titles over each one it sends.
    pub(super) fn forget(&self) {
        self.0.take();
    }
}

impl FrameFacts {
    fn work_out(w: &Workspace, inputs: Inputs) -> Self {
        let now = inputs.second;
        let mut listed = 0;
        let (mut working, mut review) = (0, 0);
        let mut current: FxHashMap<String, Vec<usize>> = FxHashMap::default();
        let mut latest: FxHashMap<String, usize> = FxHashMap::default();
        let mut newest: FxHashMap<&str, u64> = FxHashMap::default();
        let mut needs_you = Vec::new();
        let mut running = Vec::new();
        for (index, s) in w.agent_sessions.iter().enumerate() {
            if let Some(terminal) = s.terminal.as_deref() {
                if s.current {
                    current.entry(terminal.to_owned()).or_default().push(index);
                    if !running.contains(&s.runtime) {
                        running.push(s.runtime);
                    }
                }
                let updated = w.session_updated(s);
                // The last of equals, as `max_by_key` picks.
                if newest.get(terminal).is_none_or(|best| updated >= *best) {
                    newest.insert(terminal, updated);
                    latest.insert(terminal.to_owned(), index);
                }
            }
            if w.needs_you(s, now) {
                needs_you.push(index);
            }
            let hidden = sessions::session_hidden_in(w, s).is_some();
            if !hidden && sessions::session_open_in(w, &inputs.resumed, s) {
                listed += 1;
            }
            if !hidden && s.managed() && s.outcome.needs_review(now) {
                match mooring_core::status::of(w, s, now) {
                    status if status.working() => working += 1,
                    Status::Review => review += 1,
                    _ => {}
                }
            }
        }
        Self {
            inputs,
            current,
            latest,
            needs_you,
            listed,
            working,
            review,
            running,
        }
    }

    fn current_agent<'a>(&self, w: &'a Workspace, terminal: &str) -> Option<&'a AgentSession> {
        w.agent_sessions.get(*self.current.get(terminal)?.first()?)
    }

    fn latest<'a>(&self, w: &'a Workspace, terminal: &str) -> Option<&'a AgentSession> {
        w.agent_sessions.get(*self.latest.get(terminal)?)
    }

    fn pane_agent(
        &self,
        w: &Workspace,
        recognized: &HashMap<String, Agent>,
        terminal: &str,
    ) -> Option<Agent> {
        self.current_agent(w, terminal)
            .map(|s| s.runtime)
            .or_else(|| recognized.get(terminal).copied())
            .or_else(|| self.latest(w, terminal).map(|s| s.runtime))
    }

    fn current_on<'a>(&self, w: &'a Workspace, terminals: &[String]) -> Vec<&'a AgentSession> {
        let mut at: Vec<usize> = terminals
            .iter()
            .filter_map(|terminal| self.current.get(terminal))
            .flatten()
            .copied()
            .collect();
        at.sort_unstable();
        at.dedup();
        at.into_iter()
            .filter_map(|index| w.agent_sessions.get(index))
            .collect()
    }

    fn needs_you<'a>(&self, w: &'a Workspace) -> Vec<&'a AgentSession> {
        self.needs_you
            .iter()
            .filter_map(|index| w.agent_sessions.get(*index))
            .collect()
    }
}

fn inputs(w: &Workspace, resumed: &HashMap<String, String>, second: u64) -> Inputs {
    Inputs {
        second,
        revision: w.revision,
        sizes: [
            w.agent_sessions.len(),
            w.terminals.len(),
            w.tabs.len(),
            w.content.len(),
        ],
        resumed: resumed.clone(),
    }
}

impl Workbench {
    pub(super) fn facts(&self) -> Rc<FrameFacts> {
        let w = &self.workspace;
        let inputs = inputs(w, &self.resuming.resumed, now());
        if let Some(facts) = self.facts.0.borrow().as_ref()
            && facts.inputs == inputs
        {
            debug_assert_eq!(
                **facts,
                FrameFacts::work_out(w, inputs),
                "stale frame facts"
            );
            return facts.clone();
        }
        let facts = Rc::new(FrameFacts::work_out(w, inputs));
        *self.facts.0.borrow_mut() = Some(facts.clone());
        facts
    }

    /// `Workspace::current_agent`, without looking through every session.
    pub(super) fn current_agent(&self, terminal: &str) -> Option<&AgentSession> {
        self.facts().current_agent(&self.workspace, terminal)
    }

    /// The conversation a pane shows: its current one, else the latest it ran.
    pub(super) fn pane_session(&self, terminal: &str) -> Option<&AgentSession> {
        let facts = self.facts();
        facts
            .current_agent(&self.workspace, terminal)
            .or_else(|| facts.latest(&self.workspace, terminal))
    }

    /// The CLI's mark and name do not require an associated conversation. A newly
    /// recognized process takes precedence over the pane's historical conversation.
    pub(super) fn pane_agent(&self, terminal: &str) -> Option<Agent> {
        self.facts().pane_agent(
            &self.workspace,
            &self.store.recognition().terminal_agents,
            terminal,
        )
    }

    /// The current sessions of these terminals, in workspace order.
    pub(super) fn current_on(&self, terminals: &[String]) -> Vec<&AgentSession> {
        self.facts().current_on(&self.workspace, terminals)
    }

    /// Every session that needs you: the one count the title row, the chip and the status
    /// bar all show. Only Mooring's own terminals ask; scanned-only sessions stay in the
    /// overview.
    pub(super) fn needs_you(&self) -> Vec<&AgentSession> {
        self.facts().needs_you(&self.workspace)
    }

    /// Which Agent each pane holds, for its tint. One the host sees but cannot read (a
    /// shared-daemon Codex, a Claude that saves no transcript) still counts: the host says
    /// which Agent is behind each terminal note.
    pub(super) fn mark_pane_agents(&mut self, cx: &mut Context<Self>) {
        for (id, pane) in &self.layout.panes {
            let agent = self
                .current_agent(id)
                .map(|s| s.runtime)
                .or_else(|| self.store.recognition().terminal_agents.get(id).copied());
            // What its conversation was asked, for pinning the turn's ask.
            let asks: Vec<String> = self
                .pane_session(id)
                .and_then(|s| self.workspace.content.get(&s.id))
                .map(|c| c.asks.iter().map(|q| q.text.clone()).collect())
                .unwrap_or_default();
            pane.update(cx, |p, cx| {
                p.agent = agent;
                p.set_asks(asks, cx);
            });
        }
    }

    /// Two sessions whose terminal is gone, as after closing their panes.
    #[cfg(feature = "visual-test")]
    pub(super) fn visual_close_two_sessions(&mut self) {
        for s in self.workspace.agent_sessions.iter_mut().take(2) {
            s.terminal = Some("closed-visual".into());
        }
    }
}

#[cfg(test)]
#[path = "frame_facts_tests.rs"]
mod tests;
