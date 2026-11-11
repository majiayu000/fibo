//! 逐个验收: the finished turns in 要你处理, one at a time on 逐个清理's page.
//!
//! Only 等你验收 goes in. Enter closes the terminal, so a question (等你回答), a failure
//! (出错了), a wakeup (等后台) or a turn that left a server running stays out: one key must
//! not skip an answer or end a server. A session that leaves 等你验收 while queued stays in the queue, can only be
//! opened or skipped, and is never marked.
use super::board::Column;
use super::*;

/// What 逐个验收 may mark and queue.
pub(super) fn reviewable(status: Status, left_behind: bool) -> bool {
    status == Status::Review && !left_behind
}

/// The terminal to close once the mark is in: the session's live one, unless closing it
/// would break into a turn or leave a question unanswered.
pub(super) fn terminal_to_close(session: Option<&str>, interrupted: bool) -> Option<String> {
    session.filter(|_| !interrupted).map(str::to_owned)
}

impl Workbench {
    /// What 逐个验收 may mark and queue, of this workspace's sessions.
    pub(super) fn session_reviewable(&self, s: &AgentSession) -> bool {
        reviewable(
            status_of(&self.workspace, s),
            mooring_core::status::left_behind(&self.workspace, s).is_some(),
        )
    }
    pub(super) fn start_review(&mut self, at: u64, cx: &mut Context<Self>) {
        let queue = (self.column_sessions(Column::Needs, at).into_iter())
            .filter(|s| self.session_reviewable(s))
            .map(|s| s.id.clone())
            .collect();
        self.start_cleanup(queue, cx);
        self.overview.cleanup_review = true;
    }
    /// A form opened from the queue took the keyboard and left 鸟瞰: however it closed
    /// (saved, 取消, Esc), the queue's page and its keys come back.
    pub(super) fn return_to_queue(&mut self, cx: &mut Context<Self>) {
        if !self.overview.cleanup.is_empty() && self.tasks.form.is_none() && !self.nav.overview {
            self.nav.overview = true;
            self.nav.pending_focus = true;
            self.sync_panes(cx);
        }
    }
    /// The state that keeps this queued session from being marked here, if any.
    pub(super) fn review_blocked(&self, s: &AgentSession) -> Option<Status> {
        (self.overview.cleanup_review && !self.session_reviewable(s))
            .then(|| status_of(&self.workspace, s))
    }
    /// 完成 on the queue's page. In 逐个验收 the terminal closes after the host accepts the
    /// mark, as it refuses one made on a turn the window had not seen start yet.
    pub(super) fn complete_queued(&mut self, id: String, cx: &mut Context<Self>) {
        if self.overview.cleanup_review {
            self.overview.review_closing = Some(id.clone());
        }
        self.edit(
            Edit::SetOutcome {
                seen: None,
                id,
                outcome: Outcome::Completed,
            },
            cx,
        );
    }
    /// Runs on each new snapshot: closes the terminal of the session whose mark just landed.
    pub(super) fn close_reviewed(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.overview.review_closing.clone() else {
            return;
        };
        let Some(s) = (self.workspace.agent_sessions.iter()).find(|s| s.id == id) else {
            self.overview.review_closing = None;
            return;
        };
        if s.outcome != Outcome::Completed {
            return;
        }
        let live = live_terminal(&self.workspace, s).map(|t| t.id.clone());
        self.overview.review_closing = None;
        let interrupted = live
            .as_ref()
            .is_some_and(|t| interrupts(&self.running_agents(&Closing::Pane(t.clone()))));
        if let Some(terminal) = terminal_to_close(live.as_deref(), interrupted) {
            self.close_now(Closing::Pane(terminal), cx);
        }
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// For review_queue_evidence: what the table of states checks after each key.
    pub fn visual_review(&mut self, step: &str, cx: &mut Context<Self>) -> String {
        match step {
            "board" => self.show_overview(cx),
            "start" => self.start_review(now(), cx),
            "skip" => self.overview.cleanup_index += 1,
            "defersave" => {
                if let Some(form) = &self.tasks.form {
                    form.fields[0].update(cx, |f, _| f.set_text(not_ui!("等周一再看")));
                }
                self.save_form(false, cx);
            }
            "" => (),
            _ => panic!("unknown step {step}"),
        }
        cx.notify();
        let at = now();
        let queued = self.overview.cleanup.get(self.overview.cleanup_index);
        let current =
            queued.and_then(|id| (self.workspace.agent_sessions.iter()).find(|s| &s.id == id));
        let needs = self.column_sessions(Column::Needs, at);
        let row = |s: &AgentSession| {
            format!(
                "{}|{:?}|{}|live={}",
                self.workspace.session_title(s),
                s.outcome,
                status_of(&self.workspace, s).key(),
                live_terminal(&self.workspace, s).is_some()
            )
        };
        let sessions: Vec<String> = (self.workspace.agent_sessions.iter()).map(row).collect();
        format!(
            "overview={} queue={} index={} review={} current={:?} blocked={:?} defer_form={} button={} sessions={sessions:?}",
            self.nav.overview,
            self.overview.cleanup.len(),
            self.overview.cleanup_index,
            self.overview.cleanup_review,
            current.map(|s| self.workspace.session_title(s)),
            current
                .and_then(|s| self.review_blocked(s))
                .map(Status::key),
            self.tasks.form.is_some(),
            needs.iter().any(|s| self.session_reviewable(s)),
        )
    }
    /// The transcript of the queue's current session, to start a turn in it from outside.
    pub fn visual_review_log(&self) -> Option<String> {
        let id = self.overview.cleanup.get(self.overview.cleanup_index)?;
        (self.workspace.agent_sessions.iter())
            .find(|s| &s.id == id)
            .map(|s| s.transcript.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_finished_turn_can_be_marked_with_one_key() {
        for status in Status::ALL {
            assert_eq!(
                reviewable(status, false),
                status == Status::Review,
                "{status:?}"
            );
            assert!(
                !reviewable(status, true),
                "enter would end a server: {status:?}"
            );
        }
    }
    #[test]
    fn a_terminal_with_a_live_agent_stays_open() {
        assert_eq!(terminal_to_close(Some("t"), false).as_deref(), Some("t"));
        assert_eq!(terminal_to_close(Some("t"), true), None);
        assert_eq!(terminal_to_close(None, false), None);
    }
}
