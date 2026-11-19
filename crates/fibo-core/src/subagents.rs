//! The subagents a session lists under its sidebar row: what one is, which of them the
//! session keeps, and which ones did not finish.
use crate::content::{Content, Turn};

pub use fibo_remote::subagent::{Subagent, Workflow, WorkflowStep};

/// What core says about a workflow run: its one-line summary and how a stop registry
/// or the run's end closes its steps.
pub trait WorkflowRun {
    fn workflow_summary(&self, live: bool) -> Option<String>;
    fn workflow_progress(&self, live: bool) -> Option<String>;
    fn workflow_registry(&mut self, registry: &Option<(u64, Vec<String>)>);
    fn end_workflow(&mut self);
}
impl WorkflowRun for Subagent {
    fn workflow_summary(&self, live: bool) -> Option<String> {
        Some(format!("{} · {}", self.name, self.workflow_progress(live)?))
    }
    fn workflow_progress(&self, live: bool) -> Option<String> {
        let w = self.workflow.as_ref()?;
        let live = live && self.running();
        let done = w.completed(live);
        let total = w.total;
        let state = if self.stopped {
            crate::t!("已停")
        } else if live {
            crate::status::Status::Running.label()
        } else {
            crate::t!("完成")
        };
        if !w.detailed {
            return Some(crate::t!("{state} · {total} 步", state, total));
        }
        Some(crate::t!(
            "{state} · {phase} 阶段 · {done}/{total} 完成",
            state,
            phase = w.phase,
            done,
            total
        ))
    }
    fn workflow_registry(&mut self, registry: &Option<(u64, Vec<String>)>) {
        if self.workflow.is_none() {
            return;
        }
        // Known only by its run directory, the registry cannot name it either way.
        let named = self.workflow.as_ref().is_some_and(|w| w.run != self.id);
        if let Some((at, tasks)) = registry
            && named
            && self.running()
            && self.started <= *at
            && !tasks.contains(&self.id)
        {
            self.ended = Some((*at).max(self.started));
            self.stopped = self
                .workflow
                .as_ref()
                .is_some_and(|w| w.steps.iter().any(|s| s.agent.running()));
        }
        self.end_workflow();
    }
    /// Closing the run preserves results, and ends only the remaining attempts.
    fn end_workflow(&mut self) {
        if let (Some(w), Some(at)) = (&mut self.workflow, self.ended) {
            for step in &mut w.steps {
                if step.agent.running() && (self.stopped || !w.journal) {
                    step.agent.ended = Some(at.max(step.agent.started));
                    step.agent.stopped = self.stopped;
                }
            }
        }
    }
}

/// What a `<task-notification>` says: the tasks it names (one can name several, when a
/// resumed session reports what the last one left behind) and whether they did not finish.
/// Anything else, a prompt that merely quotes such a tag included, says nothing.
fn notification(text: &str) -> Option<(Vec<&str>, bool)> {
    let text = text.trim_start().strip_prefix("<task-notification>")?;
    let ids = text
        .split("<task-id>")
        .skip(1)
        .filter_map(|t| t.split_once("</task-id>"))
        .map(|(id, _)| id)
        .collect();
    let (_, status) = text.split_once("<status>")?;
    let (status, _) = status.split_once("</status>")?;
    Some((ids, matches!(status, "failed" | "killed" | "stopped")))
}

impl Content {
    pub fn workflow_stop(&mut self, event: &str, facts: &crate::protocol::HookFacts, at: u64) {
        if event == "Stop" && !facts.subagent {
            self.workflow_tasks = facts.workflow_tasks.clone().map(|tasks| (at, tasks));
            for s in &mut self.subagents {
                s.workflow_registry(&self.workflow_tasks);
            }
        }
    }
    /// Reads one line of the parent's log for a task notification. A later good report of
    /// the same task, one continued after it stopped, takes the mark back.
    pub fn note_task_notification(&mut self, text: &str) {
        let Some((ids, stopped)) = notification(text) else {
            return;
        };
        for id in ids {
            if !self.finished_subagents.iter().any(|s| s == id) {
                self.finished_subagents.push(id.to_owned());
            }
            let at = self.stopped_subagents.iter().position(|s| s == id);
            match (at, stopped) {
                (None, true) => self.stopped_subagents.push(id.to_owned()),
                (Some(at), false) => {
                    self.stopped_subagents.remove(at);
                }
                _ => (),
            }
        }
    }
    /// Keeps the subagents the sidebar lists: this turn's, and older ones still running.
    /// After a Stop that reported its tasks, no more run than it counted; the oldest
    /// extras are the ones whose end was never written (a killed session leaves its
    /// subagents unfinished on disk). One a notification reported as stopped has ended,
    /// whatever its meta says.
    pub fn keep_subagents(&mut self, all: Vec<Subagent>) {
        let mut kept: Vec<Subagent> = all
            .into_iter()
            .map(|mut s| {
                if let Some(w) = &s.workflow
                    && w.run == s.id
                    && let Some((_, task, name)) =
                        self.workflow_runs.iter().find(|(run, ..)| *run == w.run)
                {
                    if s.name == s.id && !name.is_empty() {
                        s.name = name.clone();
                    }
                    s.id = task.clone();
                }
                if self.stopped_subagents.contains(&s.id) {
                    s.stopped = true;
                    s.ended.get_or_insert(self.updated.max(s.started));
                }
                if s.workflow.is_some() {
                    if self.finished_subagents.contains(&s.id) {
                        s.ended.get_or_insert(self.updated.max(s.started));
                    }
                    s.workflow_registry(&self.workflow_tasks);
                }
                s
            })
            .filter(|s| s.running() || s.started >= self.turn_started || s.workflow.is_some())
            .collect();
        if self.turn != Turn::Running
            && let Some(b) = self.background
        {
            let running = kept.iter().filter(|s| s.running()).count();
            let extra = running.saturating_sub(b.agents as usize);
            for s in kept.iter_mut().filter(|s| s.running()).take(extra) {
                s.ended = Some(self.updated.max(s.started));
                s.end_workflow();
            }
        }
        // Ended before this turn: gone with the turn that ran them.
        kept.retain(|s| {
            s.running()
                || s.started >= self.turn_started
                || s.workflow.is_some() && s.ended.is_some_and(|at| at >= self.turn_started)
        });
        self.subagents = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Background;
    fn sub(started: u64, ended: Option<u64>) -> Subagent {
        Subagent {
            id: format!("a{started}"),
            name: format!("a{started}"),
            started,
            ended,
            ..Default::default()
        }
    }
    #[test]
    fn a_turn_keeps_its_own_subagents_and_older_ones_still_running() {
        let mut c = Content {
            turn: Turn::Running,
            turn_started: 100,
            updated: 150,
            ..Default::default()
        };
        c.keep_subagents(vec![
            sub(10, Some(20)),
            sub(50, None),
            sub(120, Some(130)),
            sub(140, None),
        ]);
        let names: Vec<_> = c.subagents.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a50", "a120", "a140"]);
        // The turn stopped and its Stop counted one agent still at work: the older
        // unfinished one is taken as ended, and having started before this turn, it goes.
        c.turn = Turn::Completed;
        c.background = Some(Background {
            agents: 1,
            ..Default::default()
        });
        c.keep_subagents(vec![sub(50, None), sub(120, Some(130)), sub(140, None)]);
        let names: Vec<_> = c
            .subagents
            .iter()
            .map(|s| (s.name.as_str(), s.running()))
            .collect();
        assert_eq!(names, [("a120", false), ("a140", true)]);
    }
    #[test]
    fn a_notification_marks_the_subagents_it_names_as_stopped() {
        let mut c = Content {
            turn: Turn::Running,
            turn_started: 100,
            updated: 150,
            ..Default::default()
        };
        // The shape a resumed session writes: several ids, one status.
        c.note_task_notification(
            "<task-notification>\n<task-id>a120</task-id>\n<task-id>a50</task-id>\n<status>stopped</status>\n<summary>2 background agents didn't finish</summary>",
        );
        c.note_task_notification(
            "<task-notification>\n<task-id>a140</task-id>\n<status>completed</status>",
        );
        // Quoted in a prompt, a tag reports nothing.
        c.note_task_notification(
            "看看这个 <task-notification><task-id>a140</task-id><status>failed</status>",
        );
        c.keep_subagents(vec![sub(50, None), sub(120, None), sub(140, None)]);
        // a50 ended with the notification and started before this turn: gone with it.
        let got: Vec<_> = c
            .subagents
            .iter()
            .map(|s| (s.name.as_str(), s.stopped, s.ended))
            .collect();
        assert_eq!(got, [("a120", true, Some(150)), ("a140", false, None)]);

        // Continued after it stopped, and finished.
        c.note_task_notification(
            "<task-notification><task-id>a120</task-id><status>completed</status>",
        );
        c.keep_subagents(vec![sub(120, None)]);
        assert!(!c.subagents[0].stopped && c.subagents[0].running());
    }
}
