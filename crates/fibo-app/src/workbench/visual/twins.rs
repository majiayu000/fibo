//! `Dense-*-twins`: two conversations of one to-do with the same long first words, so each
//! row's name ends in which Agent runs it (`· Codex`, `· Claude`) and the column is too
//! narrow for the whole name.
use super::*;

const WORDS: &str = "Eval 建立 agent 的评估：质量、费用、效率每个方面都需要单独打分并写回结果";

impl Workbench {
    pub(super) fn visual_twins(&mut self, flag: Option<&str>) {
        if flag != Some("twins") {
            return;
        }
        let Some(task) = self.workspace.tasks.first().map(|t| t.id.clone()) else {
            return;
        };
        for (id, runtime) in [("dense-s4", Agent::Codex), ("dense-s5", Agent::Claude)] {
            if let Some(s) = self
                .workspace
                .agent_sessions
                .iter_mut()
                .find(|s| s.id == id)
            {
                s.name.clear();
                s.task = Some(task.clone());
                s.runtime = runtime;
            }
            if let Some(c) = self
                .workspace
                .content
                .get_mut(id)
                .map(std::sync::Arc::make_mut)
            {
                c.title = WORDS.into();
                for q in &mut c.asks {
                    q.text = WORDS.into();
                }
            }
        }
    }
}
