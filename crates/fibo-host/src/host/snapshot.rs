//! The workspace sent to clients, with current runtime observations overlaid.
use super::*;

impl Host {
    pub(super) fn snapshot(&self) -> Workspace {
        let mut w = self.retained();
        self.overlay(&mut w);
        w
    }
    /// What changed since `base`, worked out under the host's lock. Content is shared with
    /// the host's own, so what no one changed compares by pointer and nothing is copied.
    pub(super) fn brief(
        &self,
        base: Option<&fibo_core::workspace_delta::Index>,
        fields: bool,
        hashes: &mut fibo_core::workspace_delta::Hashes,
    ) -> serde_json::Result<(
        fibo_core::workspace_delta::Delta,
        fibo_core::workspace_delta::Index,
    )> {
        fibo_core::workspace_delta::diff_with(self.snapshot(), base, fields, hashes)
    }
    /// Runtime observations over the log's word: only ever on a current session's content.
    fn overlay(&self, w: &mut Workspace) {
        // A job left in the background keeps a stopped turn working; a server does not.
        self.overlay_shells(w);
        // A spinning title is a turn running even where the log shows none, as when only
        // the tail of a long turn was read. Never the other way: a still title proves
        // nothing the log has not said.
        for s in w.agent_sessions.iter().filter(|s| s.current) {
            let busy = |cues: &HashMap<String, (String, u64)>| {
                s.terminal
                    .as_ref()
                    .and_then(|t| cues.get(t))
                    .filter(|(id, _)| *id == s.id)
                    .map(|(_, since)| *since)
            };
            let (since, cue, detail) = match (busy(&self.title_busy), busy(&self.screen_busy)) {
                (Some(since), _) => (since, "标题", "终端标题在转圈，日志里还没看到这一轮"),
                (None, Some(since)) => (
                    since,
                    "画面在跑",
                    "画面上有在跑的计时行，日志里还没看到这一轮",
                ),
                (None, None) => continue,
            };
            let Some(c) = w.content.get_mut(&s.id) else {
                continue;
            };
            let working = matches!(
                c.turn,
                fibo_core::content::Turn::Running | fibo_core::content::Turn::Waiting
            ) || c.background.is_some_and(|b| b.keeps_working());
            // A dialog answered on the screen: the prompt is gone and the turn is ticking
            // again, started after the dialog was reported. No hook says so until the
            // approved tool finishes, which can be minutes.
            let asking = s
                .terminal
                .as_ref()
                .and_then(|t| self.screen_waits.get(t))
                .is_some_and(|e| e.question().is_some());
            let answered =
                c.turn == fibo_core::content::Turn::Waiting && !asking && since >= c.signal_at;
            // A turn already running is left as the host holds it, not copied.
            if working && !answered {
                continue;
            }
            let c = std::sync::Arc::make_mut(c);
            if !working {
                c.turn = fibo_core::content::Turn::Running;
                c.turn_started = since;
                c.signal = cue.into();
                c.signal_at = since;
                c.signal_detail = detail.into();
            }
            if answered {
                c.turn = fibo_core::content::Turn::Running;
                c.end_wait(fibo_core::content::Turn::Waiting, since);
                c.waiting.clear();
                c.choices.clear();
                c.signal = cue.into();
                c.signal_at = since;
                c.signal_detail = "确认框已答，画面上的计时行又出现了".into();
            }
        }
        // A prompt visible on a live agent's screen outranks a log that has not caught up.
        // The source stays marked so it is never mistaken for a transcript record.
        for s in w.agent_sessions.iter().filter(|s| s.current) {
            let Some(seen) = s.terminal.as_ref().and_then(|t| self.screen_waits.get(t)) else {
                continue;
            };
            let Some(c) = w.content.get_mut(&s.id).map(std::sync::Arc::make_mut) else {
                continue;
            };
            c.approval = seen.approval().unwrap_or_default().to_owned();
            if let Some(t) = &s.terminal {
                self.overlay_http_ask(t, c);
            }
            match seen.question() {
                Some(question) if c.turn != fibo_core::content::Turn::Waiting => {
                    c.turn = fibo_core::content::Turn::Waiting;
                    c.waiting = format!("{question}（画面上的提示）");
                    // The episode the log or a hook will record for this same dialog.
                    c.waiting_episode += 1;
                    c.signal = "画面".into();
                    c.signal_detail = seen.detail();
                }
                Some(_) => (),
                None => c.screen_check = seen.fallback(),
            }
        }
        // What the program itself reports outranks every guess above and the log: it is
        // the program's own word, not a reading of its output.
        for s in w.agent_sessions.iter().filter(|s| s.current) {
            let Some(r) = s
                .terminal
                .as_ref()
                .and_then(|t| self.recognition.terminal_reports.get(t))
            else {
                continue;
            };
            let Some(c) = w.content.get_mut(&s.id).map(std::sync::Arc::make_mut) else {
                continue;
            };
            fibo_core::program_state::apply(c, r);
        }
        self.keep_ended_turns(w);
    }
    /// The workspace with only the sessions clients see (in a terminal, or recent) and
    /// their content. Sessions are filtered before they are copied: the store keeps every
    /// session it ever bound, and a long-used install had 5068 of them for 1540 sent, all
    /// copied on each reply to be thrown away. Content is shared, not copied. Every field
    /// is named, so one added to the workspace fails to build here rather than going
    /// missing from the reply.
    fn retained(&self) -> Workspace {
        let Workspace {
            revision,
            last_left,
            settings,
            outcome_history,
            task_outcomes,
            task_logs,
            pending_tasks,
            lost_agents,
            agent_launch,
            agent_starts,
            quiet_spaces,
            space_last_cwd,
            pinned_terminals,
            spaces,
            tasks,
            tabs,
            terminals,
            agent_sessions,
            content,
        } = &self.workspace;
        let agent_sessions: Vec<AgentSession> = (agent_sessions.iter())
            .filter(|s| s.terminal.is_some() || self.index.recent(s.updated))
            .cloned()
            .collect();
        let ids: std::collections::HashSet<&str> =
            agent_sessions.iter().map(|s| s.id.as_str()).collect();
        let content = (content.iter())
            .filter(|(id, _)| ids.contains(id.as_str()))
            .map(|(id, c)| (id.clone(), std::sync::Arc::clone(c)))
            .collect();
        Workspace {
            revision: *revision,
            last_left: *last_left,
            settings: settings.clone(),
            outcome_history: outcome_history.clone(),
            task_outcomes: task_outcomes.clone(),
            task_logs: task_logs.clone(),
            pending_tasks: pending_tasks.clone(),
            lost_agents: lost_agents.clone(),
            agent_launch: agent_launch.clone(),
            agent_starts: agent_starts.clone(),
            quiet_spaces: quiet_spaces.clone(),
            space_last_cwd: space_last_cwd.clone(),
            pinned_terminals: pinned_terminals.clone(),
            spaces: spaces.clone(),
            tasks: tasks.clone(),
            tabs: tabs.clone(),
            terminals: terminals.clone(),
            agent_sessions,
            content,
        }
    }
}
