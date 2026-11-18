//! `-unknown`: rows whose state this build cannot tell, for screenshots. A terminal here
//! that no conversation is tied to, and another machine listing one session in a state
//! word this build does not know and one Agent no conversation is tied to. None of them
//! may draw 停着's ring. A third session there runs with subagents under its row. Its host keeps the first in 置顶, so the column lists it there.
use super::Workbench;
use fibo_remote::protocol::{Card, Space};

impl Workbench {
    pub(super) fn visual_unknown_state(&mut self, flag: Option<&str>) {
        // Every page drops it first: one such machine, and none on the pages after.
        self.machines
            .retain(|m| m.view.lock().unwrap().spec.host != "studio");
        if flag != Some("unknown") {
            return;
        }
        let id = "dense-unknown".to_owned();
        let terminal = self.workspace.terminals.iter().find(|t| t.id == "dense-t0");
        let tab = self.workspace.tabs.iter().find(|t| t.id == "dense");
        let fresh = !self.workspace.terminals.iter().any(|t| t.id == id);
        if let (true, Some(mut terminal), Some(mut tab)) = (fresh, terminal.cloned(), tab.cloned())
        {
            terminal.id = id.clone();
            tab.id = id.clone();
            tab.layout = Some(fibo_core::layout::Layout::Leaf(id.clone()));
            self.workspace.terminals.push(terminal);
            self.workspace.tabs.push(tab);
            let mut recognition = self.store.recognition().clone();
            recognition.terminal_notes.insert(
                id,
                fibo_core::protocol::TerminalNote {
                    note: "codex · 看不到对话".into(),
                    why: "样例：没有关联对话的终端".into(),
                },
            );
            self.store.set_recognition(recognition);
        }
        let now = crate::model::time::now();
        let card = |id: &str, extra: serde_json::Value| -> Card {
            let mut base = serde_json::json!({
                "id": id, "space": "s1", "tab": id, "cwd": "/work/app",
                "phase": "idle", "agent": "codex", "needs_you": false,
                "updated": now.saturating_sub(600),
            });
            base.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            serde_json::from_value(base).unwrap()
        };
        let spaces = vec![Space {
            id: "s1".into(),
            name: "app".into(),
            cwd: "/work/app".into(),
        }];
        let terminals = vec![
            card(
                "r-new",
                serde_json::json!({
                    "session": "x", "title": "新版本才有的状态",
                    "status": "someday", "status_label": "新状态",
                }),
            ),
            card(
                "r-unbound",
                serde_json::json!({ "note": "codex · 看不到对话" }),
            ),
            // Known and running, with subagents its row lists as this machine's do.
            card(
                "r-subs",
                serde_json::json!({
                    "session": "y", "title": "跑完整套测试", "agent": "claude",
                    "phase": "working", "status": "running",
                    "subagents": [
                        { "id": "a1", "name": "找出慢的测试", "started": now - 300, "ended": now - 240 },
                        { "id": "a2", "name": "修 flaky 用例", "started": now - 200, "foreground": true },
                    ],
                }),
            ),
        ];
        let spec = crate::machines::Spec::new("studio".into());
        let studio = crate::machines::Remote::fixed(spec, spaces, terminals);
        studio.view.lock().unwrap().pins = Some(vec!["r-new".into()]);
        self.machines.push(studio);
        self.facts.forget();
    }
}
