//! A conversation is called by its own words (its CLI's name, its first ask) before the
//! title of the to-do it works for, so the conversations of one to-do are told apart by
//! what each does. When two of one to-do would still be called the same, each also says
//! which Agent runs it, and a second one of the same Agent its number, so the sidebar, the
//! board, the phone and `fibo list` never show two rows of one to-do called the same.
use super::{AgentSession, NameSource, Workspace};

impl Workspace {
    /// The name `s` would have without one the user gave: its CLI's, else its first ask,
    /// else its task's title.
    pub(super) fn automatic_naming(&self, s: &AgentSession) -> Option<(String, NameSource)> {
        if let Some(c) = self.content.get(&s.id) {
            let named = crate::content::topic(&c.name);
            if !named.is_empty() {
                return Some((crate::content::short(&named, 70), NameSource::Agent));
            }
            // The first ask keeps its lines, so pasted chrome above the words can go.
            let first = match c.asks.first() {
                Some(q) if c.first_ask_known => q.text.as_str(),
                _ => c.title.as_str(),
            };
            let first = crate::content::short(&crate::content::topic(first), 70);
            if !first.is_empty() {
                return Some((first, NameSource::FirstAsk));
            }
        }
        s.task
            .as_ref()
            .and_then(|id| self.tasks.iter().find(|t| &t.id == id))
            .map(|t| t.title.trim())
            .filter(|t| !t.is_empty())
            .map(|t| (t.to_owned(), NameSource::Task))
    }
    /// `name` as the name of `s`: alone when no other conversation on its task would be
    /// called it (one the user named is not), else `name · Agent`, and `name · Agent N`
    /// for the Nth of one Agent in the order they were first seen (the order
    /// `agent_sessions` keeps, stored rows included).
    pub(super) fn distinct_name(&self, s: &AgentSession, name: String) -> String {
        let Some(task) = s.task.as_deref() else {
            return name;
        };
        let mut shared = false;
        let mut before = 0;
        let mut seen = false;
        for o in self.agent_sessions.iter().filter(|o| {
            o.task.as_deref() == Some(task) && (o.id == s.id || o.name.trim().is_empty())
        }) {
            if o.id == s.id {
                seen = true;
            } else if self.automatic_naming(o).is_some_and(|(n, _)| n == name) {
                shared = true;
                if !seen && o.runtime == s.runtime {
                    before += 1;
                }
            }
        }
        let agent = s.runtime.name();
        match (shared, before) {
            (false, _) => name,
            (true, 0) => t!("{task} · {agent}", task = name, agent),
            (true, n) => t!("{task} · {agent} {n}", task = name, agent, n = n + 1),
        }
    }
}
