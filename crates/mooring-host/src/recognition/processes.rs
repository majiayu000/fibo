use super::{Process, PS_SPAWNS};
use mooring_core::model::{Agent, ProcessState, Workspace};
use mooring_platform::inspect;
use std::{
    collections::{HashMap, HashSet},
    io,
};

use std::sync::atomic::Ordering;
/// Claude Code writes no transcript when it believes it is a nested child session, so
/// nothing can ever bind it. Only the variable's presence matters; its value is not kept.
pub fn claude_transcript_off(pid: u32) -> bool {
    inspect::env_var(pid, "CLAUDE_CODE_CHILD_SESSION").is_some()
}
/// The Agent a `MOORING_AGENT` value names, by key or executable, in any case.
pub(super) fn agent_named(value: &str) -> Option<Agent> {
    let value = value.trim();
    Agent::all()
        .into_iter()
        .find(|a| a.key().eq_ignore_ascii_case(value) || a.executable().eq_ignore_ascii_case(value))
}
/// Which hinted processes stand for an Agent: the outermost of each chain (children
/// inherit the variable), and only where no process below is recognized by name, so a
/// wrapper that leaves the real CLI visible is not counted twice.
pub(super) fn hinted_roots(
    raw: &[(u32, u32, Option<Agent>, Option<Agent>)],
) -> HashMap<u32, Agent> {
    let parent: HashMap<u32, u32> = raw.iter().map(|r| (r.0, r.1)).collect();
    let hinted: HashMap<u32, Agent> = raw.iter().filter_map(|r| Some((r.0, r.3?))).collect();
    let named: Vec<u32> = raw.iter().filter(|r| r.2.is_some()).map(|r| r.0).collect();
    let below = |root: u32, pid: u32| {
        let mut seen = HashSet::new();
        let mut at = pid;
        while seen.insert(at) {
            match parent.get(&at) {
                Some(p) if *p == root => return true,
                Some(p) => at = *p,
                None => return false,
            }
        }
        false
    };
    hinted
        .iter()
        .filter(|(pid, _)| !parent.get(*pid).is_some_and(|p| hinted.contains_key(p)))
        .filter(|(pid, _)| !named.iter().any(|n| below(**pid, *n)))
        .map(|(pid, agent)| (*pid, *agent))
        .collect()
}
/// Enumerate only the trees rooted at live PTYs. No global ps or process-name scan.
pub fn processes(workspace: &Workspace) -> io::Result<Vec<Process>> {
    let roots: Vec<_> = workspace
        .terminals
        .iter()
        .filter(|t| t.state == ProcessState::Running)
        .filter_map(|t| t.pid)
        .collect();
    let below = inspect::descendants(&roots)?;
    let parents: HashMap<_, _> = below.iter().copied().collect();
    let selected: HashSet<_> = roots
        .iter()
        .copied()
        .chain(below.iter().map(|(pid, _)| *pid))
        .collect();
    if selected.is_empty() {
        return Ok(vec![]);
    }
    // Read in-process rather than spawning `/bin/ps` per hook: the walk above already knows
    // each child's parent, and ps reads the same KERN_PROCARGS2 argv for its command.
    let mut selected: Vec<_> = selected.into_iter().collect();
    selected.sort_unstable();
    PS_SPAWNS.fetch_add(1, Ordering::Relaxed);
    // pid, parent, the Agent its name shows, the Agent a MOORING_AGENT hint names.
    let mut raw = vec![];
    for pid in selected {
        let parent = parents.get(&pid).copied().unwrap_or(0);
        let runtime = crate::agents::from_argv(&inspect::argv(pid).unwrap_or_default());
        let hint = if runtime.is_none() {
            inspect::env_var(pid, mooring_core::protocol::AGENT_ENV).and_then(|v| agent_named(&v))
        } else {
            None
        };
        raw.push((pid, parent, runtime, hint));
    }
    let hinted = hinted_roots(&raw);
    let mut processes = vec![];
    for (pid, parent, runtime, _) in raw {
        let runtime = runtime.or_else(|| hinted.get(&pid).copied());
        let args = if runtime.is_some() {
            inspect::argv(pid).unwrap_or_default()
        } else {
            vec![]
        };
        let (cli_session_id, no_daemon) = runtime
            .map(|a| crate::agents::adapter(a).process_session(pid, &args))
            .unwrap_or_default();
        let launch = runtime
            .map(|a| crate::agents::adapter(a).launch_flags(&args))
            .unwrap_or_default();
        processes.push(Process {
            pid,
            parent,
            runtime,
            cli_session_id,
            no_daemon,
            launch,
        });
    }
    Ok(processes)
}
