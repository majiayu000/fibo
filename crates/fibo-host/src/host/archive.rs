//! Conversations and terminals the host keeps out of memory (`fibo_core::retention`):
//! left out at start and once an hour, read back from the store when one moves again.
use super::*;
use fibo_core::retention;

/// How often what aged out is left out; a day's worth would do, an hour keeps it small.
const EVERY: Duration = Duration::from_secs(3600);

impl Host {
    /// Leaves out what is archivable now. The store keeps every row: nothing is written but
    /// the revision, so windows drop them too.
    pub(super) fn archive_idle(&mut self) -> io::Result<()> {
        let now = fibo_core::content::now();
        let (sessions, terminals) = retention::archivable(&self.workspace, now, &self.closed);
        if sessions.is_empty() && terminals.is_empty() {
            return Ok(());
        }
        let mut next = self.workspace.clone();
        retention::leave_out(&mut next, &sessions, &terminals);
        self.commit(next)
    }
    /// Rows `next` made for conversations the store already has (archived, so not in
    /// memory) take back the stored row: same id, outcome, name and todo, and no second
    /// row for the store's UNIQUE(runtime, cli_session_id) to refuse.
    pub(super) fn readopt_archived(&self, next: &mut Workspace) -> io::Result<()> {
        // Every way a conversation appears pushes it at the end; one that left only shortens.
        let before = self.workspace.agent_sessions.len();
        if next.agent_sessions.len() <= before {
            return Ok(());
        }
        let known: std::collections::HashSet<&str> = (self.workspace.agent_sessions.iter())
            .map(|s| s.id.as_str())
            .collect();
        let made: Vec<_> = next
            .agent_sessions
            .iter()
            .filter(|s| !known.contains(s.id.as_str()))
            .map(|s| (s.id.clone(), s.runtime, s.cli_session_id.clone()))
            .collect();
        for (id, runtime, cli) in made {
            let Some(stored) = self.store.stored_session(runtime, &cli)? else {
                continue;
            };
            if stored.id == id {
                continue;
            }
            let terminal = match &stored.terminal {
                Some(t) if !next.terminals.iter().any(|o| &o.id == t) => {
                    self.store.stored_terminal(t)?
                }
                _ => None,
            };
            retention::readopt(next, &id, stored, terminal).map_err(io::Error::other)?;
        }
        Ok(())
    }
}

pub(super) fn start_archiving(shared: &Shared) {
    let weak = Arc::downgrade(shared);
    thread::spawn(move || loop {
        thread::sleep(EVERY);
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let result = lock(&shared).and_then(|mut host| host.archive_idle());
        if let Err(e) = result {
            eprintln!("archiving idle conversations: {e}");
        }
    });
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
