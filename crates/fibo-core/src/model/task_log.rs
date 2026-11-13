//! What a todo keeps beyond its words: when it was written down and closed, the
//! conversation it was noted in, and the progress Agents wrote back. Kept apart from
//! `Task` so the user's own words stay what the user wrote.
use super::*;

/// The longest note an Agent may write. Progress is a conclusion and a next step, not a
/// transcript; more is refused rather than cut, so nothing is silently lost.
pub const NOTE_LIMIT: usize = 1000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskLog {
    /// None for a todo written before this was kept.
    #[serde(default)]
    pub created: Option<u64>,
    /// When you completed or abandoned it; empty again when you reopen it.
    #[serde(default)]
    pub closed: Option<u64>,
    /// The conversation it was noted in. Where it came from, not who works on it: that
    /// conversation may be about something else, and closing this must not close it.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub notes: Vec<TaskNote>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskNote {
    pub at: u64,
    /// The conversation that wrote it, when it was written from one.
    pub session: Option<String>,
    pub text: String,
}

/// The short form of a todo's id that commands print and take: its first eight characters,
/// or the whole id when another todo starts the same (the app's ids share a long prefix).
pub fn short_id<'a>(w: &Workspace, id: &'a str) -> &'a str {
    let Some(start) = id.get(..8) else {
        return id;
    };
    let shared = w
        .tasks
        .iter()
        .any(|t| t.id != id && t.id.starts_with(start));
    if shared { id } else { start }
}

impl Workspace {
    /// What an Agent is handed for a todo: its words, the progress so far, and how to
    /// write back. A later conversation starts from what the earlier ones found.
    /// `note` is the command that writes back, as the caller can reach it, and the id goes
    /// in whole: an Agent may run commands from a daemon, whose terminal is not this one.
    pub fn task_handoff(&self, task: &Task, note: &str) -> String {
        let mut parts = vec![task.prompt()];
        let notes: Vec<String> = self
            .task_logs
            .get(&task.id)
            .map(|log| log.notes.iter().map(|n| format!("- {}", n.text)).collect())
            .unwrap_or_default();
        if !notes.is_empty() {
            parts.push(t!("目前进展：\n{notes}", notes = notes.join("\n")));
        }
        parts.push(t!(
            "（这是 Fibo 待办。每做完一段就运行 {note} --task {id} \"做了什么、剩什么、要用户定什么\"，一两句就够。完成由用户标记。）",
            note,
            id = &task.id,
        ));
        parts.join("\n\n")
    }
    /// The todo a terminal works for: its conversation's, else what its next conversation
    /// would take. The same order a binding consumes them in.
    pub fn terminal_task(&self, terminal: &str) -> Option<&Task> {
        let id = match self.current_agent(terminal) {
            Some(session) => session.task.clone()?,
            None => match self.pending_tasks.get(terminal) {
                Some(pending) => pending.clone()?,
                None => self.terminal_tab(terminal)?.default_task.clone()?,
            },
        };
        self.tasks.iter().find(|t| t.id == id)
    }
    pub(super) fn add_task_note(
        &mut self,
        task: String,
        session: Option<String>,
        text: String,
    ) -> Result<(), String> {
        if !self.tasks.iter().any(|t| t.id == task) {
            return Err(t!("待办不存在").into());
        }
        if session
            .as_ref()
            .is_some_and(|id| !self.agent_sessions.iter().any(|s| &s.id == id))
        {
            return Err(t!("会话不存在").into());
        }
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Err(t!("进展不能为空").into());
        }
        if text.chars().count() > NOTE_LIMIT {
            return Err(t!(
                "进展超过 {limit} 字，没有写入；只写结论和下一步",
                limit = NOTE_LIMIT,
            ));
        }
        self.task_logs
            .entry(task)
            .or_default()
            .notes
            .push(TaskNote {
                at: crate::time::now(),
                session,
                text,
            });
        Ok(())
    }
    /// Adds a todo noted in a conversation, which is remembered as where it came from.
    pub(super) fn add_task_from(
        &mut self,
        task: Task,
        session: Option<String>,
    ) -> Result<(), String> {
        if session
            .as_ref()
            .is_some_and(|id| !self.agent_sessions.iter().any(|s| &s.id == id))
        {
            return Err(t!("会话不存在").into());
        }
        let id = task.id.clone();
        self.edit(Edit::AddTask(task))?;
        self.task_logs.entry(id).or_default().source = session;
        Ok(())
    }
    pub(super) fn stamp_task_created(&mut self, id: &str) {
        self.task_logs.entry(id.to_owned()).or_default().created = Some(crate::time::now());
    }
    pub(super) fn stamp_task_closed(&mut self, id: &str, outcome: &Outcome) {
        let closed =
            matches!(outcome, Outcome::Completed | Outcome::Abandoned).then(crate::time::now);
        self.task_logs.entry(id.to_owned()).or_default().closed = closed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> Workspace {
        let mut w = Workspace::default();
        w.spaces.push(Space {
            id: "s".into(),
            name: "空间".into(),
            cwd: "/tmp".into(),
        });
        w
    }
    fn task(id: &str) -> Task {
        Task {
            id: id.into(),
            space: "s".into(),
            title: "沙箱起不来".into(),
            description: "同时开 3 个就失败".into(),
            acceptance: vec![],
            agent: Agent::Claude,
        }
    }

    #[test]
    fn a_later_conversation_is_handed_what_the_earlier_ones_found() {
        let mut w = workspace();
        w.edit(Edit::AddTask(task("t1234567890"))).unwrap();
        let first = w.task_handoff(&w.tasks[0], "/app/mooring todo note --runtime /rt");
        assert!(first.starts_with("沙箱起不来\n\n同时开 3 个就失败"));
        assert!(!first.contains("目前进展"));
        assert!(
            first.contains("/app/mooring todo note --runtime /rt --task t1234567890 "),
            "the whole id and where the command is, not this terminal's PATH: {first}"
        );
        w.edit(Edit::AddTaskNote {
            task: "t1234567890".into(),
            session: None,
            text: "  查清了是容量不够  ".into(),
        })
        .unwrap();
        let next = w.task_handoff(&w.tasks[0], "fibo todo note");
        assert!(next.contains("目前进展：\n- 查清了是容量不够"), "{next}");
        assert_eq!(w.tasks[0].description, "同时开 3 个就失败", "原话不变");
    }

    #[test]
    fn a_short_id_is_only_short_when_it_names_one_todo() {
        let mut w = workspace();
        w.edit(Edit::AddTask(task("35883-1793-1"))).unwrap();
        w.edit(Edit::AddTask(task("35883-1793-2"))).unwrap();
        w.edit(Edit::AddTask(task("d545a741-aaaa"))).unwrap();
        assert_eq!(short_id(&w, "35883-1793-1"), "35883-1793-1");
        assert_eq!(short_id(&w, "d545a741-aaaa"), "d545a741");
    }

    #[test]
    fn a_note_too_long_or_empty_is_refused_whole() {
        let mut w = workspace();
        w.edit(Edit::AddTask(task("t"))).unwrap();
        for text in ["   ".to_owned(), "长".repeat(NOTE_LIMIT + 1)] {
            let refused = w.edit(Edit::AddTaskNote {
                task: "t".into(),
                session: None,
                text,
            });
            assert!(refused.is_err());
        }
        assert!(w.task_logs["t"].notes.is_empty());
        assert!(
            w.edit(Edit::AddTaskNote {
                task: "gone".into(),
                session: None,
                text: "x".into(),
            })
            .is_err()
        );
    }

    #[test]
    fn writing_down_and_closing_are_timed_and_reopening_clears_it() {
        let mut w = workspace();
        w.edit(Edit::AddTask(task("t"))).unwrap();
        assert!(w.task_logs["t"].created.is_some());
        assert_eq!(w.task_logs["t"].closed, None);
        let set = |w: &mut Workspace, outcome| {
            w.edit(Edit::SetTaskOutcome {
                id: "t".into(),
                outcome,
                seen: None,
            })
            .unwrap()
        };
        set(&mut w, Outcome::Completed);
        assert!(w.task_logs["t"].closed.is_some());
        set(&mut w, Outcome::Open);
        assert_eq!(w.task_logs["t"].closed, None);
        set(&mut w, Outcome::Abandoned);
        assert!(w.task_logs["t"].closed.is_some());
    }

    #[test]
    fn noting_a_todo_in_a_conversation_does_not_tie_that_conversation_to_it() {
        let mut w = crate::sample::every_state();
        let tasks = w.tasks.len();
        let mut noted = task("noted");
        noted.space = w.spaces[0].id.clone();
        assert!(
            w.edit(Edit::AddTaskFrom {
                task: noted.clone(),
                session: Some("gone".into()),
            })
            .is_err()
        );
        assert_eq!(w.tasks.len(), tasks, "refused whole");
        let session = w.agent_sessions[0].clone();
        w.edit(Edit::AddTaskFrom {
            task: noted,
            session: Some(session.id.clone()),
        })
        .unwrap();
        assert_eq!(w.task_logs["noted"].source, Some(session.id.clone()));
        let after = w
            .agent_sessions
            .iter()
            .find(|s| s.id == session.id)
            .unwrap();
        assert_eq!(
            after.task, session.task,
            "where it came from, not who works on it"
        );
    }
}
