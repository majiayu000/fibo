//! The subagents a session lists under its sidebar row: what one is, which of them the
//! session keeps, and which ones did not finish.
use crate::content::{Content, Turn};
use serde::{Deserialize, Serialize};

/// One subagent a session started: its description (its type without one), and when it
/// started and ended.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subagent {
    /// The CLI's id for it, which a task notification names it by.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub started: u64,
    /// `None` while it runs.
    #[serde(default)]
    pub ended: Option<u64>,
    /// The parent turn waits on it.
    #[serde(default)]
    pub foreground: bool,
    /// It failed or was stopped before finishing.
    #[serde(default)]
    pub stopped: bool,
}
impl Subagent {
    pub fn running(&self) -> bool {
        self.ended.is_none()
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
    /// Reads one line of the parent's log for a task notification. A later good report of
    /// the same task, one continued after it stopped, takes the mark back.
    pub fn note_task_notification(&mut self, text: &str) {
        let Some((ids, stopped)) = notification(text) else {
            return;
        };
        for id in ids {
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
                if self.stopped_subagents.contains(&s.id) {
                    s.stopped = true;
                    s.ended.get_or_insert(self.updated.max(s.started));
                }
                s
            })
            .filter(|s| s.running() || s.started >= self.turn_started)
            .collect();
        if self.turn != Turn::Running
            && let Some(b) = self.background
        {
            let running = kept.iter().filter(|s| s.running()).count();
            let extra = running.saturating_sub(b.agents as usize);
            for s in kept.iter_mut().filter(|s| s.running()).take(extra) {
                s.ended = Some(self.updated.max(s.started));
            }
        }
        // Ended before this turn: gone with the turn that ran them.
        kept.retain(|s| s.running() || s.started >= self.turn_started);
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
