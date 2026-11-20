//! `-unknown`: rows whose state this build cannot tell, for screenshots. A terminal here
//! that no conversation is tied to, and another machine, studio, whose workbench holds a
//! few of this machine's sessions: one runs with subagents under its row, one Agent there
//! has no conversation, and its host keeps the first in 置顶, so the column lists it there.
use super::Workbench;
use gpui::{AppContext, Context, Window};

impl Workbench {
    pub(super) fn visual_unknown_state(
        &mut self,
        flag: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Every page drops it first: one such machine, and none on the pages after.
        self.machines
            .retain(|m| m.view.lock().unwrap().spec.host != "studio");
        self.visual_set_host_view("studio", None);
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
        // Studio's workspace: a running session of this machine's, with two subagents, and
        // two more, the first of them in its 置顶; and a terminal of its own that no
        // conversation is tied to.
        let mut ws = self.workspace.clone();
        let running = |s: &fibo_core::model::AgentSession| {
            crate::model::session_state::session_status(&ws, &self.attention.unseen, s).0
                == fibo_core::status::Status::Running
        };
        let open: Vec<_> = (ws.agent_sessions.iter())
            .filter(|s| self.session_open(s) && self.session_hidden(s).is_none())
            .collect();
        let busy = open.iter().find(|s| running(s)).map(|s| s.id.clone());
        let others: Vec<_> = (open.iter())
            .filter(|s| !running(s))
            .take(2)
            .map(|s| {
                (
                    s.id.clone(),
                    fibo_core::status::live_terminal(&ws, s).map(|t| t.id.clone()),
                )
            })
            .collect();
        let keep: Vec<String> = (busy.iter().cloned())
            .chain(others.iter().map(|(id, _)| id.clone()))
            .collect();
        let pin = others.iter().find_map(|(_, t)| t.clone());
        ws.agent_sessions.retain(|s| keep.contains(&s.id));
        ws.pinned_terminals = Some(pin.into_iter().collect());
        if let Some(id) = &busy
            && let Some(c) = ws.content.get_mut(id)
        {
            std::sync::Arc::make_mut(c).subagents = serde_json::from_value(serde_json::json!([
                { "id": "a1", "name": "找出慢的测试", "started": now - 300, "ended": now - 240 },
                { "id": "a2", "name": "修 flaky 用例", "started": now - 200, "foreground": true },
            ]))
            .unwrap();
        }
        let mut recognition = self.store.recognition().clone();
        recognition
            .terminal_notes
            .retain(|t, _| t == "dense-unknown");
        let socket = std::env::temp_dir().join("fibo-visual-studio.sock");
        let view = cx.new(|cx| Workbench::embedded(socket, ws.clone(), window, cx));
        view.update(cx, |w, _| w.store.set_recognition(recognition.clone()));
        let spaces = fibo_core::remote_cards::spaces(&ws).0;
        let terminals = fibo_core::remote_cards::cards(&ws, &recognition, now);
        let spec = crate::machines::Spec::new("studio".into());
        let studio = crate::machines::Remote::fixed(spec, spaces, terminals);
        studio.view.lock().unwrap().pins = ws.pinned_terminals.clone();
        self.machines.push(studio);
        self.visual_set_host_view("studio", Some(view));
        self.facts.forget();
    }
}
