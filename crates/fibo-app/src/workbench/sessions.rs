//! Session grouping, the 「你在这里」 card, the "needs you" queue and navigation into a session.
use super::*;

mod here_card;
mod tip;

const ROUTE_PENDING: &str = tk!("会话还没载入，载入后自动打开");

/// Which finished turns the user has not looked at yet.
pub(super) struct Attention {
    /// Last live phase of each session whose agent runs in a Fibo terminal.
    pub(super) phases: HashMap<String, Phase>,
    /// The turn key each session had when last idle in view (or when first met idle).
    pub(super) seen: HashMap<String, String>,
    /// Sessions whose turn ended while their pane was out of view, until it is shown.
    pub(super) unseen: HashSet<String>,
}
/// Conversations being resumed: the next launch's command, and the terminals opened for it.
pub(super) struct Resuming {
    /// The next launch resumes a conversation: its command, cwd and session id.
    pub(super) resume: Option<(String, String, String)>,
    /// Terminals opened to resume a session, so clicking it again returns there
    /// instead of opening another tab while the resume has not taken.
    pub(super) resumed: HashMap<String, String>,
}
impl Workbench {
    /// Shows the tab holding `terminal_id` and focuses its pane; false when no tab holds it.
    pub(super) fn focus_terminal(
        &mut self,
        terminal_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.nav.focused = Some(terminal_id.to_owned());
        self.nav.pending_focus = true;
        let Some(tab) = self.workspace.terminal_tab(terminal_id) else {
            return false;
        };
        self.nav.zoomed = super::pane_sync::zoom_kept(self.nav.zoomed.take(), terminal_id);
        self.nav.space = tab.space.clone();
        self.nav.tab = Some(tab.id.clone());
        self.leave_host(cx);
        self.nav.overview = false;
        self.sync_panes(cx);
        if let Some(pane) = self.layout.panes.get(terminal_id) {
            window.focus(&pane.focus_handle(cx), cx);
        }
        self.ring_landing(terminal_id, cx);
        cx.notify();
        true
    }
    /// See `Workspace::session_title`; every surface names a conversation the same way.
    pub(super) fn session_title(&self, s: &AgentSession) -> String {
        self.workspace.session_title(s)
    }
    /// A sidebar row's title in `cell` (its size and ink already set): the words shorten
    /// before what tells two of one task apart (` · Codex`), which a narrow column keeps
    /// whole. Only such a row gets the extra boxes (R9).
    pub(super) fn session_title_cell(&self, s: &AgentSession, cell: gpui::Div) -> gpui::Div {
        let (words, tail) = self.workspace.session_title_parts(s);
        if tail.is_empty() {
            return cell.truncate().child(words);
        }
        cell.flex()
            .overflow_hidden()
            .whitespace_nowrap()
            .child(div().min_w_0().truncate().child(words))
            .child(div().flex_shrink_0().child(tail))
    }
    /// Double-click names a conversation; an empty name hands it back to its CLI and
    /// first ask.
    pub(super) fn rename_session(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.save_session_name(cx);
        let Some(session) = self.workspace.agent_sessions.iter().find(|s| s.id == id) else {
            return;
        };
        let automatic = self.session_title(&AgentSession {
            name: String::new(),
            ..session.clone()
        });
        let current = session.name.clone();
        let field = cx.new(|cx| {
            Field::new(t!("会话名"), &current, cx)
                .inline()
                .ends_on_click_out()
                .placeholder(&automatic)
        });
        field.update(cx, |f, _| f.select_all());
        let events = cx.subscribe(&field, |this, _, event, cx| match event {
            crate::ui::field::FieldEvent::Submit | crate::ui::field::FieldEvent::ClickOut => {
                this.save_session_name(cx);
                this.nav.pending_focus = true;
            }
            crate::ui::field::FieldEvent::Cancel => {
                this.layout.renaming_session = None;
                this.nav.pending_focus = true;
                cx.notify();
            }
        });
        // Clicking anywhere else keeps the name, as with tabs.
        let blur = cx.on_blur(&field.focus_handle(cx), window, |this, _, cx| {
            this.save_session_name(cx)
        });
        window.focus(&field.focus_handle(cx), cx);
        self.layout.renaming_session = Some((id.to_owned(), field, [events, blur]));
        cx.notify();
    }
    fn save_session_name(&mut self, cx: &mut Context<Self>) {
        let Some((id, field, _)) = self.layout.renaming_session.take() else {
            return;
        };
        let Some(session) = self.workspace.agent_sessions.iter().find(|s| s.id == id) else {
            return;
        };
        let name = field.read(cx).text.trim().to_owned();
        // Keeping the shown name unchanged must not pin today's automatic one.
        let name = if session.name.is_empty() && name == self.session_title(session) {
            String::new()
        } else {
            name
        };
        if name != session.name {
            self.edit(Edit::RenameSession { id, name }, cx);
        }
        cx.notify();
    }
    /// When its conversation last moved.
    pub(super) fn session_updated(&self, s: &AgentSession) -> u64 {
        self.workspace.session_updated(s)
    }
    pub(super) fn session_line(&self, s: &AgentSession) -> String {
        session_line_in(&self.workspace, s)
    }
    pub(super) fn save_session_task(&mut self, session: &AgentSession, cx: &mut Context<Self>) {
        let Some(c) = self.workspace.content.get(&session.id) else {
            return;
        };
        if c.asks.is_empty() {
            self.error = Some(t!("尾段没有用户原话，不能凭空建立待办").into());
            cx.notify();
            return;
        }
        let task = Task {
            id: id(),
            space: self.nav.space.clone(),
            title: self.session_title(session),
            description: c
                .asks
                .iter()
                .map(|q| q.text.as_str())
                .collect::<Vec<_>>()
                .join(t!("\n\n追加要求：")),
            acceptance: vec![],
            agent: session.runtime,
        };
        self.edit(
            Edit::SaveSessionTask {
                session: session.id.clone(),
                task,
            },
            cx,
        );
    }
    /// A Fibo terminal ran it and is closed now; it resumes in a new pane.
    pub(super) fn session_closed(&self, s: &AgentSession) -> bool {
        s.terminal.is_some() && !self.session_open(s)
    }
    /// Its terminal is running in a tab: the sidebar lists only these.
    pub(super) fn session_open(&self, s: &AgentSession) -> bool {
        session_open_in(&self.workspace, &self.resuming.resumed, s)
    }
    pub(super) fn session_hidden(&self, s: &AgentSession) -> Option<&str> {
        session_hidden_in(&self.workspace, s)
    }
    pub(super) fn enter_card(
        &mut self,
        s: &AgentSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.overview.card_menu = None;
        self.overview.filter_space = None;
        self.overview.changed_only = false;
        self.overview.closed_only = false;
        self.overview.show_stale =
            status_of(&self.workspace, s) == Status::Stale || self.overview.show_stale;
        self.overview.show_running =
            status_of(&self.workspace, s).working() || self.overview.show_running;
        if live_terminal(&self.workspace, s).is_some() {
            self.open_session(s, window, cx);
        } else {
            self.nav.overview = true;
            self.sync_panes(cx);
            self.overview.here = Some(s.id.clone());
            cx.notify();
        }
    }
    pub(super) fn session_actions(
        &self,
        session: &AgentSession,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let save = session.clone();
        let defer = session.id.clone();
        let mut menu = div()
            .id(gpui::SharedString::from(format!("actions-{}", session.id)))
            .w(px(T::MENU_WIDTH))
            .p(px(T::S6))
            .bg(T::color(T::raised()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            .rounded(px(T::CONTROL_RADIUS))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(T::S4))
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(button("card-save", t!("存为待办")).on_click(cx.listener(
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.overview.card_menu = None;
                    this.save_session_task(&save, cx);
                },
            )))
            .child(button("card-defer", t!("稍后再看")).on_click(cx.listener(
                move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.overview.card_menu = None;
                    this.overview.here = Some(defer.clone());
                    this.show_form(FormKind::Defer, window, cx);
                },
            )));
        for (key, label, outcome) in [
            ("complete", t!("标记完成"), Outcome::Completed),
            ("abandon", t!("放弃"), Outcome::Abandoned),
            ("reopen", t!("撤销结局"), Outcome::Open),
        ]
        .into_iter()
        .filter(|(_, _, o)| *o != session.outcome)
        {
            let id = session.id.clone();
            let settled = session.clone();
            menu = menu.child(
                button(gpui::SharedString::from(format!("card-{key}")), label).on_click(
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.overview.card_menu = None;
                        // 标记完成 on a session with its terminal open goes to that pane
                        // and plays the animation there, as the pane's own button does.
                        if outcome == Outcome::Completed
                            && let Some(t) = live_terminal(&this.workspace, &settled)
                        {
                            let t = t.id.clone();
                            if this.focus_terminal(&t, window, cx) {
                                this.settle_session(&settled, outcome.clone(), true, cx);
                                return;
                            }
                        }
                        this.edit(
                            Edit::SetOutcome {
                                seen: None,
                                id: id.clone(),
                                outcome: outcome.clone(),
                            },
                            cx,
                        );
                    }),
                ),
            );
        }
        menu
    }
    pub fn navigate_to(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.overview.cleanup.clear();
        self.tasks.form = None;
        self.search.hide();
        if let Some(session) = self
            .workspace
            .agent_sessions
            .iter()
            .find(|s| s.id == id)
            .cloned()
        {
            // Noted where it lands: a session still loading comes back here through
            // open_pending_route, and noting the click too would count one jump twice.
            self.note_jump(id, cx);
            self.enter_card(&session, window, cx);
            self.note_landed();
            // Report actual selected layout and focused pane, not a queued route intent.
            if !self.nav.overview
                && let Some(terminal) = live_terminal(&self.workspace, &session)
                && self.nav.focused.as_deref() == Some(terminal.id.as_str())
                && let Some(tab) = self.workspace.terminal_tab(&terminal.id)
                && self.nav.tab.as_deref() == Some(tab.id.as_str())
                && let Some(pane) = self.layout.panes.get(&terminal.id)
                && pane.focus_handle(cx).is_focused(window)
            {
                crate::desktop::route_applied(id, &terminal.id, &tab.id);
            }
        } else {
            self.request(Request::Workspace, cx);
            // After the request: starting one clears the error line.
            self.error = Some(fibo_core::i18n::say(ROUTE_PENDING).into());
            self.pending_route = Some((id.to_owned(), self.nav.page()));
        }
    }
    /// Opens the session a notification asked for once it has loaded; dropped if the
    /// user went to another page meanwhile, as their own choice wins.
    pub(super) fn open_pending_route(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, page)) = self.pending_route.take() else {
            return;
        };
        let moved = page != self.nav.page();
        if moved || self.workspace.agent_sessions.iter().any(|s| s.id == id) {
            if self.error.as_deref() == Some(fibo_core::i18n::say(ROUTE_PENDING)) {
                self.error = None;
            }
            if !moved {
                self.navigate_to(&id, window, cx);
            }
        } else {
            self.pending_route = Some((id, page));
        }
    }
}
/// The card's receipts by kind: commands that exited 0, file changes, and failures.
/// Test and commit receipts repeat their command's, so they are not counted again.
pub(super) fn evidence_tally(evidence: &[fibo_core::content::Evidence]) -> (usize, usize, usize) {
    let count = |f: &dyn Fn(&fibo_core::content::Evidence) -> bool| {
        evidence.iter().filter(|e| f(e)).count()
    };
    (
        count(&|e| e.kind == "command" && e.exit_code == Some(0)),
        count(&|e| e.kind == "file" && e.exit_code == Some(0)),
        count(&|e| e.kind == "error"),
    )
}
/// 做成了: confirmed receipts, newest first, at most four. A file reads as its path
/// inside the session's directory, once, with how many times it changed.
pub(super) fn done_items(
    evidence: &[fibo_core::content::Evidence],
    cwd: &str,
) -> Vec<(String, usize, bool)> {
    let mut items: Vec<(String, usize, bool)> = Vec::new();
    for e in evidence.iter().rev().filter(|e| e.exit_code == Some(0)) {
        let (text, file) = match e.kind.as_str() {
            "file" => {
                let path = e.summary.trim_start_matches(not_ui!("文件修改："));
                let inside = Path::new(path)
                    .strip_prefix(cwd)
                    .ok()
                    .and_then(|p| p.to_str())
                    .filter(|p| !p.is_empty());
                (inside.unwrap_or(path).to_owned(), true)
            }
            "test" | "commit" => (fibo_core::content::short(&e.summary, 180), false),
            _ => continue,
        };
        match items.iter().position(|(t, _, _)| *t == text) {
            Some(i) => items[i].1 += 1,
            None if items.len() < 4 => items.push((text, 1, file)),
            None => {}
        }
    }
    items
}
pub(super) fn session_open_in(
    w: &Workspace,
    resumed: &HashMap<String, String>,
    s: &AgentSession,
) -> bool {
    live_terminal(w, s)
        .or_else(|| resume_terminal(w, resumed, s))
        .is_some_and(|t| w.terminal_tab(&t.id).is_some())
}
/// 在跑's test content, for the board's and the sidebar's tests too.
#[cfg(test)]
pub(super) use tests::running_turn;
pub(super) fn session_line_in(w: &Workspace, s: &AgentSession) -> String {
    w.content
        .get(&s.id)
        .map(|c| fibo_core::content::short(&c.headline(), 100))
        .unwrap_or_else(|| t!("内容尚未索引").into())
}
/// What a running turn is at, for 在跑's board cards and 会话优先's rows (#516): its
/// latest tool or command this turn in words (`正在改 tabs.rs`), never the ask its title
/// already says, so a fresh turn shows nothing. It reads the content alone, not the clock:
/// it changes only when the log does and asks for no frame of its own.
pub(super) fn running_line_in(w: &Workspace, s: &AgentSession) -> Option<String> {
    let doing = w.content.get(&s.id)?.doing_recorded()?;
    Some(t!("正在{d}", d = doing))
}
/// Why the window's lists, counts and board leave a session out: it is automation, or its
/// space is put aside (先不管).
pub(super) fn session_hidden_in<'a>(w: &'a Workspace, s: &AgentSession) -> Option<&'a str> {
    automation_hidden_in(w, s).or_else(|| w.session_quiet(s).then_some(t!("先不管")))
}
/// Hidden as automation, whichever space it is in: the only hiding search looks past.
pub(super) fn automation_hidden_in<'a>(w: &'a Workspace, s: &AgentSession) -> Option<&'a str> {
    match w.content.get(&s.id) {
        Some(c) => c.hidden_for(s),
        // A hook can bind a new conversation before its first prompt writes anything.
        None if s.current && s.binding.is_some() => None,
        None => Some(t!("缺少交互入口证据#workbench")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibo_core::content::{Content, Evidence, Quote, Turn};

    /// 在跑's card and row said nothing of what the Agent was doing (#516). They say its
    /// latest record this turn, and nothing until it has one: the title is the ask.
    #[test]
    fn a_running_turn_says_its_latest_record() {
        let mut w = super::super::tests::fixture();
        let s = w.agent_sessions[0].clone();
        let record = |kind: &str, summary: &str, at| Evidence {
            kind: kind.into(),
            summary: summary.into(),
            exit_code: None,
            at,
            source: Default::default(),
        };
        let mut c = Content {
            turn: Turn::Running,
            turn_started: 100,
            asks: vec![Quote {
                text: "修复登录".into(),
                ..Default::default()
            }],
            evidence: vec![record("command", "cargo test -p fibo-core", 50)],
            ..Default::default()
        };
        assert_eq!(running_line_in(&w, &s), None, "no content yet");
        w.content
            .insert(s.id.clone(), std::sync::Arc::new(c.clone()));
        assert_eq!(
            running_line_in(&w, &s),
            None,
            "an earlier turn's record, then the ask"
        );
        c.evidence
            .push(record("file", "文件修改：crates/app/src/tabs.rs", 120));
        w.content
            .insert(s.id.clone(), std::sync::Arc::new(c.clone()));
        assert_eq!(running_line_in(&w, &s).as_deref(), Some("正在改 tabs.rs"));
        c.evidence.push(record(
            "command",
            "cd /x && cargo build -p app 2>&1 | tail",
            130,
        ));
        w.content.insert(s.id.clone(), std::sync::Arc::new(c));
        assert_eq!(
            running_line_in(&w, &s).as_deref(),
            Some("正在运行 cargo build -p app")
        );
    }

    /// A running turn that has changed `file` since it was asked, for 在跑's tests.
    pub(in crate::workbench) fn running_turn(file: &str) -> fibo_core::content::Content {
        use fibo_core::content::{Content, Evidence, Quote, Turn};
        Content {
            turn: Turn::Running,
            turn_started: 100,
            asks: vec![Quote {
                text: "fix the login".into(),
                ..Default::default()
            }],
            evidence: vec![Evidence {
                kind: "file".into(),
                summary: file.into(),
                exit_code: None,
                at: 120,
                source: Default::default(),
            }],
            ..Default::default()
        }
    }
}
