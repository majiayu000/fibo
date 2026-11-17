//! A to-do's title names every conversation started for it or given it. When more than
//! one of them would be called by it, each also says which Agent runs it, and a second
//! one of the same Agent its number, so the sidebar, the board, the phone and `fibo list`
//! never show two rows called the same.
use super::{AgentSession, Workspace};

impl Workspace {
    /// `title`, the title of `task`, as the name of `s`: alone when no other conversation
    /// on the task is named by it (one the user named is not), else `title · Agent`, and
    /// `title · Agent N` for the Nth of one Agent in the order they were first seen (the
    /// order `agent_sessions` keeps, stored rows included).
    pub(super) fn task_session_name(&self, s: &AgentSession, task: &str, title: &str) -> String {
        let mut shared = false;
        let mut before = 0;
        let mut seen = false;
        for o in self.agent_sessions.iter().filter(|o| {
            o.task.as_deref() == Some(task) && (o.id == s.id || o.name.trim().is_empty())
        }) {
            if o.id == s.id {
                seen = true;
            } else {
                shared = true;
                if !seen && o.runtime == s.runtime {
                    before += 1;
                }
            }
        }
        let agent = s.runtime.name();
        match (shared, before) {
            (false, _) => title.to_owned(),
            (true, 0) => t!("{task} · {agent}", task = title, agent),
            (true, n) => t!("{task} · {agent} {n}", task = title, agent, n = n + 1),
        }
    }
}
