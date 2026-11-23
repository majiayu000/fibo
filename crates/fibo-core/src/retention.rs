//! Which conversations and terminals the host keeps in memory (docs/workspace-retention,
//! option B, W = 30 days on 11-24). The rest stays in the store as an archive: nothing is
//! deleted, it is only no longer loaded, cloned on each commit and sent. A conversation
//! whose log moves again comes back from the store with its outcome, name and todo.
use crate::model::{Outcome, ProcessState, Workspace};
use std::collections::HashSet;

/// How long an ended conversation with nothing left for you stays in memory.
pub const KEEP_FOR: u64 = 30 * 86_400;

/// What can go: (conversations, terminals). `held` are terminals the host still waits on
/// (a closed pane whose process has not ended).
pub fn archivable(
    w: &Workspace,
    now: u64,
    held: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    let in_tabs: HashSet<String> = w
        .tabs
        .iter()
        .filter_map(|t| t.layout.as_ref())
        .flat_map(|l| l.ids())
        .collect();
    let pinned: HashSet<&String> = w.pinned_terminals.iter().flatten().collect();
    let open_terminal = |id: &String| {
        in_tabs.contains(id)
            || pinned.contains(id)
            || held.contains(id)
            || w.terminals.iter().any(|t| {
                &t.id == id
                    && matches!(
                        t.state,
                        ProcessState::Starting | ProcessState::Running | ProcessState::Draining
                    )
            })
    };
    let open_task = |id: &String| {
        w.tasks.iter().any(|t| &t.id == id)
            && !matches!(
                w.task_outcomes.get(id),
                Some(Outcome::Completed | Outcome::Abandoned)
            )
    };
    let mut sessions = HashSet::new();
    let mut kept_terminals = HashSet::new();
    for s in &w.agent_sessions {
        let keep = s.terminal.as_ref().is_some_and(open_terminal)
            || matches!(s.outcome, Outcome::Deferred { .. })
            || s.task.as_ref().is_some_and(open_task)
            || now.saturating_sub(w.session_updated(s)) < KEEP_FOR
            || w.needs_you(s, now);
        if keep {
            kept_terminals.extend(s.terminal.clone());
        } else {
            sessions.insert(s.id.clone());
        }
    }
    // A terminal goes with its conversations, never before them: a kept one that failed
    // reads its 出错了 from the terminal's row. One a restart interrupted stays while it
    // is listed to reopen (`lost_agents`).
    let terminals = w
        .terminals
        .iter()
        .filter(|t| {
            !open_terminal(&t.id)
                && !kept_terminals.contains(&t.id)
                && !w.lost_agents.contains_key(&t.id)
        })
        .map(|t| t.id.clone())
        .collect();
    (sessions, terminals)
}

/// Leaves out what `archivable` found; whether anything went.
pub fn archive(w: &mut Workspace, now: u64, held: &HashSet<String>) -> bool {
    let (sessions, terminals) = archivable(w, now, held);
    leave_out(w, &sessions, &terminals)
}
/// Leaves these conversations, their content and these terminals out of `w`.
pub fn leave_out(
    w: &mut Workspace,
    sessions: &HashSet<String>,
    terminals: &HashSet<String>,
) -> bool {
    if sessions.is_empty() && terminals.is_empty() {
        return false;
    }
    w.agent_sessions.retain(|s| !sessions.contains(&s.id));
    w.content.retain(|id, _| !sessions.contains(id));
    w.terminals.retain(|t| !terminals.contains(&t.id));
    true
}

/// A conversation that came back while archived: the store's row takes the place of the
/// one just made for it, and the new sighting merges into it as any sighting of a known
/// conversation does, so its outcome, name and todo stay. Its terminal comes back with it
/// when it ran in one. Without this the new row would break the store's
/// UNIQUE(runtime, cli_session_id).
pub fn readopt(
    w: &mut Workspace,
    made: &str,
    archived: crate::model::AgentSession,
    terminal: Option<crate::model::Terminal>,
) -> Result<(), String> {
    let Some(i) = w.agent_sessions.iter().position(|s| s.id == made) else {
        return Ok(());
    };
    let mut seen = w.agent_sessions.remove(i);
    let content = w.content.remove(made);
    let launch = w.agent_launch.remove(made);
    if let Some(t) = terminal.filter(|t| !w.terminals.iter().any(|o| o.id == t.id)) {
        w.terminals.push(t);
    }
    let id = archived.id.clone();
    // Archived means its terminal ended: it is no longer current anywhere.
    w.agent_sessions.push(crate::model::AgentSession {
        current: false,
        ..archived
    });
    seen.id = id.clone();
    w.observe_agent(seen)?;
    if let Some(c) = content {
        w.content.insert(id.clone(), c);
    }
    if let Some(flags) = launch {
        w.agent_launch.insert(id, flags);
    }
    Ok(())
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;
