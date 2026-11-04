//! The `Dense-first` screenshot samples of subagents under a session row.
use super::super::*;
use mooring_core::content::{Background, Subagent, Turn};

const HERE: &str = "这个诊断的 ui 感觉很丑啊";
const OTHER: &str = "标签命名依旧有问题";

/// Named subagents ten seconds apart, ended where `ended` says.
fn subs(names: &str, ended: impl Fn(usize) -> bool) -> Vec<Subagent> {
    let now = mooring_core::content::now();
    names
        .split(',')
        .enumerate()
        .map(|(i, name)| Subagent {
            name: name.into(),
            started: now - 400 + 10 * i as u64,
            ended: ended(i).then(|| now - 120 + i as u64),
            ..Default::default()
        })
        .collect()
}

impl Workbench {
    /// `-sub`: four subagents under the session you are in (one ended, its list open) and
    /// two under 标签命名依旧有问题, whose turn ended with one still running, which keeps
    /// it 在跑. `-many`: twenty-three under the one you are in; `-manyall` with all of them
    /// listed. `-marks`: every mark at once, with the second session's list open too.
    pub(in super::super) fn sample_subagents(&mut self, v: &str) {
        let many = "tasks,spaces,shortcuts,plugins,market,phone link,relay,quota,file events,marks,handoff,legacy,CLI,board,sidebar,overview,palette,settings,keymap,statusbar,tabs,peek,cleanup"
            .split(',')
            .map(|a| format!("Audit {a} drift"))
            .collect::<Vec<_>>()
            .join(",");
        let mut other_row = None;
        for (id, c) in self.workspace.content.iter_mut() {
            match c.title.as_str() {
                HERE if v == "sub" => {
                    c.subagents = subs(
                        "Check IME/keyboard charters,Check PTY/process lifecycle charters,Check agent status/hook charters,Check remote/phone & render charters",
                        |i| i == 0,
                    )
                }
                HERE if v == "marks" => {
                    let mut list = subs(
                        "Explore sidebar rows,Check PTY/process lifecycle,Audit plugin drift,Check IME/keyboard charters",
                        |i| i >= 2,
                    );
                    list[0].foreground = true;
                    list[2].stopped = true;
                    c.subagents = list;
                }
                HERE => c.subagents = subs(&many, |i| i % 8 == 0),
                OTHER if v == "sub" || v == "marks" => {
                    c.turn = Turn::Completed;
                    c.completed_claim = false;
                    c.background = Some(Background {
                        agents: 1,
                        ..Default::default()
                    });
                    c.subagents = subs(
                        "Explore tab naming paths,Audit phone link protocol drift",
                        |i| i % 2 == 0,
                    );
                    other_row = Some(id.clone());
                }
                _ => (),
            }
        }
        if let (Some(id), "marks") = (other_row, v) {
            self.layout.subagents_toggled.insert(id, true);
        }
        if v == "manyall"
            && let Some(s) = self
                .workspace
                .agent_sessions
                .iter()
                .find(|s| s.terminal.as_deref() == Some("dense-t2"))
        {
            self.layout.subagents_all.insert(s.id.clone());
        }
    }
}
