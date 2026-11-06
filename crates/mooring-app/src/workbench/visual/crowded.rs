//! `Dense-*-crowded`: ten sessions to act on and eleven running, the numbers that once left
//! 在跑 listing two of its eleven rows.
use super::*;
use mooring_core::content::Turn;

impl Workbench {
    pub(super) fn visual_crowded(&mut self, flag: Option<&str>) {
        if flag != Some("crowded") || self.workspace.terminals.iter().any(|t| t.id == "crowd-t0") {
            return;
        }
        let Some(session) = self
            .workspace
            .agent_sessions
            .iter()
            .find(|s| s.id == "dense-s2")
            .cloned()
        else {
            return;
        };
        let Some(terminal) = self
            .workspace
            .terminals
            .iter()
            .find(|t| t.id == "dense-t2")
            .cloned()
        else {
            return;
        };
        let Some(content) = self.workspace.content.get("dense-s2").cloned() else {
            return;
        };
        // Only these rows, so the counts on the headings are the sample's.
        for c in self.workspace.content.values_mut() {
            c.hidden_reason = "visual".into();
        }
        let Some(tab) = self.workspace.tabs.iter().find(|t| t.id == "dense").cloned() else {
            return;
        };
        let now = now();
        for i in 0..21_u64 {
            let (sid, tid) = (format!("crowd-s{i}"), format!("crowd-t{i}"));
            let (need, at) = (i < 10, now - 60 * (i + 1));
            let title = if need {
                format!("要你看的第 {} 个", i + 1)
            } else {
                format!("在跑的第 {} 个", i - 9)
            };
            self.workspace.terminals.push(Terminal {
                id: tid.clone(),
                ..terminal.clone()
            });
            // Open in a tab, or the row counts as closed and lands under 今天.
            self.workspace.tabs.push(Tab {
                id: tid.clone(),
                layout: Some(mooring_core::layout::Layout::Leaf(tid.clone())),
                ..tab.clone()
            });
            self.workspace.agent_sessions.push(AgentSession {
                id: sid.clone(),
                cli_session_id: sid.clone(),
                terminal: Some(tid),
                current: true,
                updated: at,
                name: title.clone(),
                ..session.clone()
            });
            self.workspace.content.insert(
                sid,
                mooring_core::content::Content {
                    title,
                    turn: if need { Turn::Completed } else { Turn::Running },
                    completed_claim: need,
                    last_reply: if need {
                        "这一轮做完了。".into()
                    } else {
                        String::new()
                    },
                    turn_started: at,
                    updated: at,
                    hidden_reason: String::new(),
                    ..content.clone()
                },
            );
        }
        self.facts.forget();
    }
}
