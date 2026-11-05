//! The `Workspace` methods that change it; the ones that only read stay in `model.rs`.
use super::*;

fn validate_outcome(outcome: &Outcome) -> Result<(), String> {
    if let Outcome::Deferred { reason, until } = outcome
        && (reason.trim().is_empty() || *until == 0)
    {
        return Err("稍后再看需要原因和时间".into());
    }
    Ok(())
}
impl Workspace {
    /// An outcome edit made on what the sender saw: refused once the session's outcome
    /// changed or its Agent began another turn, unless it already is what was asked.
    fn check_seen(&self, id: &str, seen: &SeenSession, outcome: &Outcome) -> Result<(), String> {
        let session = self
            .agent_sessions
            .iter()
            .find(|s| s.id == id)
            .ok_or("Agent 会话不存在")?;
        if &session.outcome == outcome {
            return Ok(());
        }
        if session.outcome != seen.outcome {
            return Err("这个会话的结局刚被改过，看一眼再定".into());
        }
        let turn = self.content.get(id).map_or("", |c| c.turn_key.as_str());
        if turn != seen.turn {
            return Err("Agent 又开始了新的一轮，没有标记；看完这一轮再定".into());
        }
        Ok(())
    }
    pub fn change_outcome(
        &mut self,
        id: &str,
        outcome: Outcome,
        at: u64,
        reason: &str,
    ) -> Result<(), String> {
        validate_outcome(&outcome)?;
        let session = self
            .agent_sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("Agent 会话不存在")?;
        if session.outcome != outcome {
            self.outcome_history.push(OutcomeChange {
                session: id.into(),
                before: session.outcome.clone(),
                after: outcome.clone(),
                at,
                reason: reason.into(),
            });
            session.outcome = outcome;
        }
        Ok(())
    }
    pub fn expire_outcomes(&mut self, now: u64) -> bool {
        let due: Vec<_> = self
            .agent_sessions
            .iter()
            .filter_map(|s| match s.outcome {
                Outcome::Deferred { until, .. } if until <= now => Some((s.id.clone(), until)),
                _ => None,
            })
            .collect();
        let mut changed = !due.is_empty();
        for (id, at) in due {
            self.change_outcome(&id, Outcome::Open, at, "暂存到期")
                .expect("collected session");
        }
        for outcome in self.task_outcomes.values_mut() {
            if matches!(outcome,Outcome::Deferred{until,..} if *until <= now) {
                *outcome = Outcome::Open;
                changed = true;
            }
        }
        changed
    }
    /// Called only after the host has proven the CLI identity and terminal link.
    /// Importing a log alone leaves terminal=None and never changes ownership.
    pub fn observe_agent(&mut self, mut observed: AgentSession) -> Result<(), String> {
        let prior = self.agent_sessions.iter().position(|s| {
            s.runtime == observed.runtime && s.cli_session_id == observed.cli_session_id
        });
        if let Some(index) = prior {
            let old = &self.agent_sessions[index];
            if observed.current
                && old.current
                && observed.terminal.is_some()
                && observed.terminal != old.terminal
            {
                return Err("同一 CLI 会话仍关联在另一个终端；先确认原关联结束".into());
            }
            observed.id = old.id.clone();
            observed.task = old.task.clone();
            observed.name = old.name.clone();
            observed.outcome = old.outcome.clone();
            observed.updated = old.updated.max(observed.updated);
            if observed.terminal.is_none() {
                observed.terminal = old.terminal.clone();
                observed.current = old.current;
                observed.binding = old.binding;
            }
        }
        if let Some(terminal_id) = &observed.terminal {
            let terminal = self
                .terminals
                .iter()
                .find(|t| &t.id == terminal_id)
                .ok_or("终端不存在")?;
            // Default only on the first proven association, not on moves/resumes.
            let first_binding = prior.is_none_or(|i| self.agent_sessions[i].terminal.is_none());
            if first_binding && observed.task.is_none() {
                observed.task = self
                    .pending_tasks
                    .get(terminal_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        self.terminal_tab(&terminal.id)
                            .and_then(|t| t.default_task.clone())
                    })
                    // A todo you closed takes no new conversation, even from its own tab.
                    .filter(|id| {
                        !matches!(
                            self.task_outcomes.get(id),
                            Some(Outcome::Completed | Outcome::Abandoned)
                        )
                    });
            }
            if observed.current {
                self.pending_tasks.remove(terminal_id);
                for old in &mut self.agent_sessions {
                    if old.terminal.as_ref() == Some(terminal_id) {
                        old.current = false;
                    }
                }
            }
        }
        let id = observed.id.clone();
        if let Some(index) = prior {
            self.agent_sessions[index] = observed;
        } else {
            self.agent_sessions.push(observed);
        }
        self.adopt_tab_name(&id);
        self.revision += 1;
        Ok(())
    }
    /// A name is kept once, on the conversation. A tab named before (or without) its
    /// Agent hands that name to the conversation it comes to hold alone, and stops
    /// keeping its own; tabs with several conversations or none keep theirs.
    pub fn adopt_tab_name(&mut self, session: &str) {
        let Some(s) = self.agent_sessions.iter().find(|s| s.id == session) else {
            return;
        };
        if !s.name.trim().is_empty() {
            return;
        }
        let Some(tab) = self
            .sole_session_tab(s)
            .filter(|t| !t.title.trim().is_empty())
            .map(|t| t.id.clone())
        else {
            return;
        };
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab) else {
            return;
        };
        let name = std::mem::take(&mut tab.title);
        if let Some(s) = self.agent_sessions.iter_mut().find(|s| s.id == session) {
            s.name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        }
    }
    /// Moves names pinned on tabs onto the conversation each holds alone; run once
    /// after loading a workspace saved before names had one owner.
    pub fn adopt_tab_names(&mut self) {
        let ids: Vec<String> = self
            .agent_sessions
            .iter()
            .filter(|s| s.current)
            .map(|s| s.id.clone())
            .collect();
        for id in ids {
            self.adopt_tab_name(&id);
        }
    }
    pub fn edit(&mut self, edit: Edit) -> Result<(), String> {
        match edit {
            Edit::Settings(settings) => {
                if !(1..=365).contains(&settings.stale_days)
                    || !(1..=1440).contains(&settings.away_minutes)
                {
                    return Err("天数须为 1–365，离开分钟数须为 1–1440".into());
                }
                self.settings = settings;
            }

            Edit::UpdateTask(task) => {
                let index = self
                    .tasks
                    .iter()
                    .position(|t| t.id == task.id)
                    .ok_or("待办不存在")?;
                // Reuse the creation boundary so validation stays in one place.
                let mut check = self.clone();
                check.tasks.remove(index);
                check.edit(Edit::AddTask(task.clone()))?;
                self.tasks[index] = task;
            }
            Edit::SetTaskOutcome { id, outcome, seen } => {
                if !self.tasks.iter().any(|t| t.id == id) {
                    return Err("待办不存在".into());
                }
                validate_outcome(&outcome)?;
                let now = self.task_outcomes.get(&id).cloned().unwrap_or_default();
                if seen.is_some_and(|seen| seen != now) && now != outcome {
                    return Err("这个待办的结局刚被改过，看一眼再定".into());
                }
                self.task_outcomes.insert(id, outcome);
            }
            Edit::SetPendingTask { terminal, task } => {
                if !self.terminals.iter().any(|t| t.id == terminal) {
                    return Err("终端不存在".into());
                }
                if task
                    .as_ref()
                    .is_some_and(|id| !self.tasks.iter().any(|t| &t.id == id))
                {
                    return Err("待办不存在".into());
                }
                self.pending_tasks.insert(terminal, task);
            }
            Edit::SaveSessionTask { session, task } => {
                if !self.agent_sessions.iter().any(|s| s.id == session) {
                    return Err("会话不存在".into());
                }
                let task_id = task.id.clone();
                self.edit(Edit::AddTask(task))?;
                self.agent_sessions
                    .iter_mut()
                    .find(|s| s.id == session)
                    .unwrap()
                    .task = Some(task_id);
            }
            Edit::AddSpace(space) => {
                if self.spaces.iter().any(|s| s.id == space.id) {
                    return Err("空间 ID 已存在".into());
                }
                if space.name.trim().is_empty() {
                    return Err("空间名称不能为空".into());
                }
                self.spaces.push(space);
            }
            Edit::UpdateSpace(space) => {
                if space.name.trim().is_empty() {
                    return Err("空间名称不能为空".into());
                }
                let Some(existing) = self.spaces.iter_mut().find(|s| s.id == space.id) else {
                    return Err("空间不存在".into());
                };
                *existing = space;
            }
            Edit::AddTask(task) => {
                if !self.spaces.iter().any(|s| s.id == task.space) {
                    return Err("空间不存在".into());
                }
                if self.tasks.iter().any(|t| t.id == task.id) {
                    return Err("待办 ID 已存在".into());
                }
                if task.title.trim().is_empty() {
                    return Err("待办标题不能为空".into());
                }
                for (i, criterion) in task.acceptance.iter().enumerate() {
                    if criterion.description.trim().is_empty()
                        || criterion.id.is_empty()
                        || task.acceptance[..i].iter().any(|c| c.id == criterion.id)
                    {
                        return Err("验收条目需要独立 ID 和非空说明".into());
                    }
                    if !criterion.evidence.iter().all(|e| e.record.attachable()) {
                        return Err("证据只认测试、提交和文件改动".into());
                    }
                }
                self.tasks.push(task);
            }
            edit @ (Edit::CreateTab { .. }
            | Edit::RenameTab { .. }
            | Edit::MoveTab { .. }
            | Edit::CloseTab { .. }
            | Edit::ClosePane { .. }
            | Edit::SplitPane { .. }
            | Edit::MovePane { .. }
            | Edit::ResizeSplit { .. }
            | Edit::ReplacePane { .. }) => self.edit_tabs(edit)?,
            Edit::RemoveSpace { id } => {
                if !self.spaces.iter().any(|s| s.id == id) {
                    return Err("空间不存在".into());
                }
                if self.spaces.len() == 1 {
                    return Err("至少要留一个空间".into());
                }
                let open = self
                    .tasks
                    .iter()
                    .filter(|t| t.space == id)
                    .filter(|t| {
                        !matches!(
                            self.task_outcomes.get(&t.id),
                            Some(Outcome::Completed | Outcome::Abandoned)
                        )
                    })
                    .count();
                if open > 0 {
                    return Err(format!(
                        "空间里还有 {open} 个没收尾的待办，先完成或放弃它们"
                    ));
                }
                self.tabs.retain(|t| t.space != id);
                self.spaces.retain(|s| s.id != id);
            }
            Edit::SetOutcome { id, outcome, seen } => {
                if let Some(seen) = seen {
                    self.check_seen(&id, &seen, &outcome)?;
                }
                self.change_outcome(&id, outcome, crate::content::now(), "用户操作")?;
            }
            Edit::AssignTask { id, task } => {
                if task
                    .as_ref()
                    .is_some_and(|id| !self.tasks.iter().any(|t| &t.id == id))
                {
                    return Err("待办不存在".into());
                }
                self.agent_sessions
                    .iter_mut()
                    .find(|s| s.id == id)
                    .ok_or("Agent 会话不存在")?
                    .task = task;
            }
            Edit::RenameSession { id, name } => {
                self.agent_sessions
                    .iter_mut()
                    .find(|s| s.id == id)
                    .ok_or("Agent 会话不存在")?
                    .name = name.split_whitespace().collect::<Vec<_>>().join(" ");
            }
        }
        self.revision += 1;
        Ok(())
    }
}
