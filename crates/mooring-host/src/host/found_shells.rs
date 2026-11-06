//! Background commands of a CLI only whose screen says some run (`BackgroundCommands::Screen`).
//! Its child processes, matched against its own log, say which, and they get the same
//! judgment as listed shells. Only while a screen says so: nothing polls otherwise.
//! docs/decisions/2026-11-04-background-shell-own-client.md has the rules.
use super::*;
use crate::agents::BackgroundCommands;
use crate::background_shells as shells;
use mooring_core::content::{ShellTask, Turn};
use std::path::PathBuf;

/// (session, its CLI, its log) for the sessions to look for commands of.
pub(super) type Looking = Vec<(String, Agent, PathBuf)>;

impl Host {
    /// A quiet screen, read: does its CLI say a command was left running?
    pub(super) fn screen_background(&mut self, terminal: &str, tail: &str) -> io::Result<()> {
        let left = self.workspace.current_agent(terminal).is_some_and(|s| {
            match crate::agents::adapter(s.runtime).background_commands() {
                BackgroundCommands::Screen(left) => left(tail),
                BackgroundCommands::Hooks => false,
            }
        });
        let watch = &mut self.shell_watch;
        if left == watch.on_screen.contains_key(terminal) {
            return Ok(());
        }
        if left {
            watch.on_screen.insert(terminal.into(), Instant::now());
            watch.poke();
        } else {
            watch.on_screen.remove(terminal);
            if let Some(s) = self.workspace.current_agent(terminal) {
                let id = s.id.clone();
                self.shell_watch.forget(&id);
            }
        }
        let next = self.workspace.clone();
        self.commit(next)
    }
    /// A host just started has seen no screen go quiet, and a quiet screen stays quiet.
    pub(super) fn screens_to_read(&self) -> Vec<(String, LiveTerminal)> {
        self.workspace
            .agent_sessions
            .iter()
            .filter(|s| s.current)
            .filter_map(|s| {
                let id = s.terminal.as_ref()?;
                Some((id.clone(), self.terminals.get(id)?.clone()))
            })
            .collect()
    }
    /// Sessions whose log says the turn ended while the screen says a command runs.
    pub(super) fn looking(&self) -> Looking {
        self.shell_watch
            .on_screen
            .keys()
            .filter_map(|terminal| self.workspace.current_agent(terminal))
            .filter(|s| self.turn_over(&s.id))
            .map(|s| (s.id.clone(), s.runtime, PathBuf::from(&s.transcript)))
            .collect()
    }
    fn turn_over(&self, session: &str) -> bool {
        let content = self.workspace.content.get(session);
        content.is_some_and(|c| c.turn == Turn::Completed)
    }
    /// The commands a screen says `session` left running, for `overlay_shells`. A command no
    /// process was found for counts as a job until it has run past the limit.
    pub(super) fn found_tasks(&self, terminal: &str, session: &str) -> Option<Vec<ShellTask>> {
        let since = self.shell_watch.on_screen.get(terminal)?;
        if !self.turn_over(session) {
            return None;
        }
        let task = |id: &str, job: bool, why: &str| ShellTask {
            id: id.into(),
            description: "后台命令".into(),
            job,
            why: why.into(),
        };
        let found = self.shell_watch.found.get(session);
        Some(match found.filter(|f| !f.is_empty()) {
            Some(f) => f
                .iter()
                .map(|pid| {
                    task(
                        pid,
                        self.shell_watch.job(session, pid),
                        self.shell_watch.why(session, pid),
                    )
                })
                .collect(),
            None => vec![task(
                "screen",
                since.elapsed().as_secs() < shells::JOB_LIMIT,
                "屏幕说有后台终端，没对上具体命令",
            )],
        })
    }
    /// A CLI reports `working` for a turn that ended as long as a command runs, server or
    /// not, and its title may spin. The log and the commands know better.
    pub(super) fn keep_ended_turns(&self, w: &mut Workspace) {
        for terminal in self.shell_watch.on_screen.keys() {
            let Some(id) = w.current_agent(terminal).map(|s| s.id.clone()) else {
                continue;
            };
            if !self.turn_over(&id) {
                continue;
            }
            if let Some(c) = w.content.get_mut(&id).filter(|c| c.turn == Turn::Running) {
                c.turn = Turn::Completed;
            }
        }
    }
}
