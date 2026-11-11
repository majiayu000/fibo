//! Session grouping, the 「你在这里」 card, the "needs you" queue and navigation into a session.
use super::*;

mod here_card;

const ROUTE_PENDING: &str = tk!("会话还没载入，载入后自动打开");

/// Which finished turns the user has not looked at yet.
pub(super) struct Attention {
    /// Last live phase of each session whose agent runs in a Mooring terminal.
    pub(super) phases: HashMap<String, Phase>,
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
        self.nav.space = tab.space.clone();
        self.nav.tab = Some(tab.id.clone());
        self.nav.overview = false;
        self.sync_panes(cx);
        if let Some(pane) = self.layout.panes.get(terminal_id) {
            window.focus(&pane.focus_handle(cx), cx);
        }
        cx.notify();
        true
    }
    /// See `Workspace::session_title`; every surface names a conversation the same way.
    pub(super) fn session_title(&self, s: &AgentSession) -> String {
        self.workspace.session_title(s)
    }
    /// What hovering a conversation's row shows: where its name comes from, what it
    /// was first and last asked, and where it runs.
    pub(super) fn session_tip(&self, s: &AgentSession) -> Vec<String> {
        use mooring_core::content::short;
        let mut lines = vec![match self.workspace.session_naming(s) {
            Some((_, source)) => t!(
                "名字来自{source} · 双击改名",
                source = source.label(s.runtime)
            ),
            None => t!("双击改名").into(),
        }];
        if let Some(c) = self.workspace.content.get(&s.id) {
            let line =
                |text: &str| short(&text.split_whitespace().collect::<Vec<_>>().join(" "), 120);
            if let Some(first) = c.asks.first().filter(|_| c.first_ask_known) {
                lines.push(t!("最初：{text}", text = line(&first.text)));
            }
            // Only when it is not the first ask shown above.
            if let Some(last) = c
                .asks
                .last()
                .filter(|_| c.asks.len() > 1 || !c.first_ask_known)
            {
                lines.push(t!("最近：{text}", text = line(&last.text)));
            }
        }
        lines.push(format!("{} · {}", s.runtime.name(), s.cwd));
        lines
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
        self.workspace
            .content
            .get(&s.id)
            .map(|c| mooring_core::content::short(&c.headline(), 100))
            .unwrap_or_else(|| t!("内容尚未索引").into())
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
    /// A Mooring terminal ran it and is closed now; it resumes in a new pane.
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
            self.enter_card(&session, window, cx);
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
            self.error = Some(mooring_core::i18n::say(ROUTE_PENDING).into());
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
            if self.error.as_deref() == Some(mooring_core::i18n::say(ROUTE_PENDING)) {
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
pub(super) fn evidence_tally(
    evidence: &[mooring_core::content::Evidence],
) -> (usize, usize, usize) {
    let count = |f: &dyn Fn(&mooring_core::content::Evidence) -> bool| {
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
    evidence: &[mooring_core::content::Evidence],
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
            "test" | "commit" => (mooring_core::content::short(&e.summary, 180), false),
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
