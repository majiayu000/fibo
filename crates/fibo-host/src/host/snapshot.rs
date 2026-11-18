//! The workspace sent to clients, with current runtime observations overlaid.
use super::*;

impl Host {
    pub(super) fn snapshot(&self) -> Workspace {
        let mut w = self.workspace.clone();
        w.agent_sessions
            .retain(|s| s.terminal.is_some() || self.index.recent(s.updated));
        let retained: std::collections::HashSet<_> =
            w.agent_sessions.iter().map(|s| s.id.as_str()).collect();
        w.content.retain(|id, _| retained.contains(id.as_str()));
        // A job left in the background keeps a stopped turn working; a server does not.
        self.overlay_shells(&mut w);
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
            if !working {
                c.turn = fibo_core::content::Turn::Running;
                c.turn_started = since;
                c.signal = cue.into();
                c.signal_at = since;
                c.signal_detail = detail.into();
            }
            // A dialog answered on the screen: the prompt is gone and the turn is ticking
            // again, started after the dialog was reported. No hook says so until the
            // approved tool finishes, which can be minutes.
            let asking = s
                .terminal
                .as_ref()
                .and_then(|t| self.screen_waits.get(t))
                .is_some_and(|e| e.question().is_some());
            if c.turn == fibo_core::content::Turn::Waiting && !asking && since >= c.signal_at {
                c.turn = fibo_core::content::Turn::Running;
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
            let Some(c) = w.content.get_mut(&s.id) else {
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
            let Some(c) = w.content.get_mut(&s.id) else {
                continue;
            };
            fibo_core::program_state::apply(c, r);
        }
        self.keep_ended_turns(&mut w);
        w
    }
}
