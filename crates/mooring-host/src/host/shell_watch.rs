//! Keeps looking at the background shells of turns that stopped with some left, so the
//! snapshot can tell a job the agent waits for from a server it left running. Looks only
//! while such a turn exists; otherwise the thread waits for the next commit.
use super::*;
use crate::background_shells::{self as shells, Judged};

/// (task id, command) for each shell one Stop listed.
type Tasks = Vec<(String, String)>;
#[derive(Default)]
pub(super) struct ShellWatch {
    /// Session -> (task id, command) from its last Stop. Memory only: a command line may
    /// hold a secret, so it is never stored or sent.
    commands: HashMap<String, Tasks>,
    /// (session, task id) -> the judgment so far.
    judged: HashMap<(String, String), Judged>,
    wake: Option<SyncSender<()>>,
}
impl ShellWatch {
    /// A hook listed `session`'s shells: these replace what it listed before.
    pub(super) fn listed(&mut self, session: &str, facts: &HookFacts) {
        if facts.background.is_none() {
            return;
        }
        let tasks: Vec<_> = facts
            .shells
            .iter()
            .map(|t| (t.id.clone(), t.command.clone()))
            .collect();
        self.judged
            .retain(|(s, id), _| s != session || tasks.iter().any(|(t, _)| t == id));
        if tasks.is_empty() {
            self.commands.remove(session);
        } else {
            self.commands.insert(session.into(), tasks);
        }
        self.poke();
    }
    pub(super) fn poke(&self) {
        if let Some(tx) = &self.wake {
            let _ = tx.try_send(());
        }
    }
    /// Whether this task of `session` is a job, by what the looks so far found.
    pub(super) fn job(&self, session: &str, task: &str) -> bool {
        self.judged
            .get(&(session.to_owned(), task.to_owned()))
            .is_some_and(|j| j.job)
    }
}
impl Host {
    /// Sessions to look at: a live agent whose stopped turn left shells it listed.
    fn shell_targets(&self) -> Vec<(String, u32, Tasks)> {
        self.agent_pids
            .iter()
            .filter_map(|(pid, id)| {
                let c = self.workspace.content.get(id)?;
                // A turn that starts again clears its shells (see `Parser::phase`).
                if c.shells.is_empty() {
                    return None;
                }
                let tasks: Vec<_> = self
                    .shell_watch
                    .commands
                    .get(id)?
                    .iter()
                    .filter(|(t, _)| c.shells.iter().any(|s| &s.id == t))
                    .cloned()
                    .collect();
                (!tasks.is_empty()).then(|| (id.clone(), *pid, tasks))
            })
            .collect()
    }
    /// Marks each listed shell of a live session a job or not, and counts the jobs.
    pub(super) fn overlay_shells(&self, w: &mut Workspace) {
        for s in w.agent_sessions.iter().filter(|s| s.current) {
            let Some(c) = w.content.get_mut(&s.id) else {
                continue;
            };
            if c.shells.is_empty() {
                continue;
            }
            for t in &mut c.shells {
                t.job = self.shell_watch.job(&s.id, &t.id);
            }
            if let Some(b) = &mut c.background {
                b.jobs = (c.shells.iter().filter(|t| t.job).count() as u32).min(b.shells);
            }
        }
    }
}
pub(super) fn start(shared: &Shared) -> io::Result<()> {
    let (tx, rx) = sync_channel(1);
    lock(shared)?.shell_watch.wake = Some(tx);
    let weak = Arc::downgrade(shared);
    thread::spawn(move || loop {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let targets = match lock(&shared) {
            Ok(host) => host.shell_targets(),
            Err(_) => return,
        };
        drop(shared);
        if targets.is_empty() {
            if rx.recv().is_err() {
                return;
            }
            continue;
        }
        let looks: Vec<(u32, Tasks)> = targets
            .iter()
            .map(|(_, pid, tasks)| (*pid, tasks.clone()))
            .collect();
        match shells::look(&looks) {
            Ok(seen) => {
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                let Ok(mut host) = lock(&shared) else {
                    return;
                };
                let mut changed = false;
                for (session, _, tasks) in &targets {
                    for (task, _) in tasks {
                        let key = (session.clone(), task.clone());
                        let before = host.shell_watch.judged.get(&key).copied();
                        let now = shells::judge(before, seen.get(task).copied());
                        changed |= before.map(|b| b.job) != Some(now.job);
                        host.shell_watch.judged.insert(key, now);
                    }
                }
                // Nothing to store; a new revision makes clients fetch the overlaid snapshot.
                if changed {
                    let next = host.workspace.clone();
                    if let Err(e) = host.commit(next) {
                        eprintln!("background shells: {e}");
                    }
                }
            }
            Err(e) => eprintln!("background shells: {e}"),
        }
        match rx.recv_timeout(Duration::from_secs(shells::EVERY)) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        }
    });
    Ok(())
}
