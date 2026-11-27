//! What the palette finds for a query, and moving through and opening the hits.
use super::*;

impl Workbench {
    /// Every session search can show, newest first.
    pub(super) fn searchable(&self) -> Vec<&AgentSession> {
        let mut visible: Vec<&AgentSession> = self
            .workspace
            .agent_sessions
            .iter()
            // A space put aside is still found: search is one way back to it.
            .filter(|s| sessions::automation_hidden_in(&self.workspace, s).is_none())
            .collect();
        visible.sort_by_key(|s| std::cmp::Reverse(self.workspace.session_updated(s)));
        visible
    }
    /// The status its row shows, when that is one of the four the palette counts. Only
    /// sessions still open count, as on 鸟瞰.
    fn counted_group(&self, s: &AgentSession) -> Option<Status> {
        let (group, _) = session_status(&self.workspace, &self.attention.unseen, s);
        (s.outcome.needs_review(now())
            && COUNTED.contains(&group)
            && !self.workspace.session_quiet(s))
        .then_some(group)
    }
    pub(super) fn status_counts(&self) -> Vec<(Status, &'static str, usize)> {
        let groups: Vec<Status> = self
            .searchable()
            .into_iter()
            .filter_map(|s| self.counted_group(s))
            .collect();
        COUNTED
            .iter()
            .map(|g| (*g, g.label(), groups.iter().filter(|x| *x == g).count()))
            .collect()
    }
    /// A group's first rows and a 「再显示」 row for the rest.
    fn section(&self, title: &'static str, note: String, mut hits: Vec<Hit>) -> Section {
        let total = hits.len();
        let limit = COLLAPSED + self.search.shown.get(title).copied().unwrap_or(0);
        if total > limit {
            hits.truncate(limit);
            hits.push(Hit::More {
                section: title,
                hidden: total - limit,
            });
        }
        Section {
            title: Some(title),
            note,
            total,
            hits,
        }
    }
    /// What the palette lists for `query` (already trimmed): actions, recent sessions by
    /// time and the todos when empty, otherwise matching sessions, quotes and todos.
    pub(super) fn search_sections(&self, query: &str) -> Vec<Section> {
        let visible = self.searchable();
        let session = |s: &AgentSession, quote: Option<String>| Hit::Session {
            id: s.id.clone(),
            quote,
        };
        let mut sections = Vec::new();
        if query.is_empty() {
            let group = self.search.group;
            if group.is_none() {
                let mut actions = vec![Hit::NewSession];
                if !self.needs_you().is_empty() {
                    actions.push(Hit::NextWaiting);
                }
                sections.push(Section {
                    title: None,
                    note: String::new(),
                    total: actions.len(),
                    hits: actions,
                });
            }
            let at = now();
            let mut runs: Vec<(&'static str, Vec<Hit>)> = Vec::new();
            for s in visible
                .into_iter()
                .filter(|s| !self.workspace.session_quiet(s))
                .filter(|s| group.is_none() || self.counted_group(s) == group)
            {
                let title = recency(self.workspace.session_updated(s), at);
                match runs.last_mut() {
                    Some((last, hits)) if *last == title => hits.push(session(s, None)),
                    _ => runs.push((title, vec![session(s, None)])),
                }
            }
            for (title, hits) in runs {
                sections.push(self.section(title, hits.len().to_string(), hits));
            }
            if group.is_none() && !self.workspace.tasks.is_empty() {
                let tasks: Vec<Hit> = self
                    .workspace
                    .tasks
                    .iter()
                    .map(|t| Hit::Task(t.id.clone()))
                    .collect();
                sections.push(self.section(t!("待办"), tasks.len().to_string(), tasks));
            }
            return sections;
        }
        let needle = SearchQuery::new(query);
        let latest_ask = |s: &AgentSession| {
            self.workspace.content.get(&s.id).and_then(|c| {
                c.asks
                    .iter()
                    .rev()
                    .find(|q| needle.matches(&q.text))
                    .map(|q| q.text.clone())
            })
        };
        let sessions: Vec<Hit> = visible
            .iter()
            .filter(|s| {
                // The first ask stays findable after the conversation is renamed.
                let first = self
                    .workspace
                    .content
                    .get(&s.id)
                    .and_then(|c| c.asks.first().filter(|_| c.first_ask_known))
                    .map_or("", |q| q.text.as_str());
                needle.matches(&format!(
                    "{} {first} {} {}",
                    self.session_title(s),
                    s.cli_session_id,
                    s.cwd
                ))
            })
            .map(|s| session(s, latest_ask(s)))
            .collect();
        let tasks: Vec<Hit> = self
            .workspace
            .tasks
            .iter()
            .filter(|t| needle.matches(&format!("{} {}", t.title, t.description)))
            .map(|t| Hit::Task(t.id.clone()))
            .collect();
        let pane = self.nav.focused.clone();
        let actions: Vec<Hit> = pane
            .map(|p| self.actions_for(&p))
            .unwrap_or_default()
            .into_iter()
            .filter(|a| needle.matches(&format!("{} {}", a.title, a.command)))
            .filter_map(|a| {
                fibo_core::action::all()
                    .iter()
                    .position(|b| std::ptr::eq(a, b))
                    .map(Hit::Action)
            })
            .collect();
        // 「视图」 or 「左栏」 lists every view; a view's own name finds it too.
        let views: Vec<Hit> = self
            .sidebar_presets()
            .filter(|p| {
                // Chinese words always match; the current language's word too.
                let words = format!("{} {}", not_ui!("左栏 侧栏 视图"), t!("左栏视图"));
                needle.matches(&format!(
                    "{words} {} {}",
                    p.name,
                    fibo_core::i18n::say(&p.name)
                ))
            })
            .map(|p| Hit::View(p.id.clone()))
            .collect();
        let tools: Vec<Hit> = needle
            .matches(&format!(
                "{} {}",
                not_ui!("本机诊断 诊断 卡顿 占用 CPU GPU 内存 diagnostics"),
                t!("本机诊断")
            ))
            .then_some(Hit::Diagnostics)
            .into_iter()
            .chain(
                needle
                    .matches(&format!(
                        "{} {}",
                        not_ui!("Agent Hook hooks 钩子 安装 卸载 状态不准 审批 claude"),
                        t!("安装或卸载 Agent 的 Hook")
                    ))
                    .then_some(Hit::Hooks),
            )
            .chain(self.machine_hits(query))
            .chain(self.space_hits(query))
            .collect();
        let commands: Vec<Hit> = self.command_hits(&needle);
        // Typed queries select the first row without searching just to find its index.
        for (title, hits) in [
            (t!("命令"), commands),
            (t!("会话"), sessions),
            (t!("待办"), tasks),
            (t!("动作"), actions),
            (t!("左栏视图"), views),
            (t!("工具"), tools),
        ] {
            if !hits.is_empty() {
                sections.push(self.section(title, hits.len().to_string(), hits));
            }
        }
        if let Some(page) = self
            .search
            .message_result
            .as_ref()
            .filter(|page| page.query == query)
        {
            let ids: HashSet<&str> = visible.iter().map(|s| s.id.as_str()).collect();
            let hits: Vec<_> = page
                .hits
                .iter()
                .filter(|hit| ids.contains(hit.position.session.as_str()))
                .cloned()
                .map(Hit::Message)
                .collect();
            if !hits.is_empty() {
                sections.push(self.section(t!("聊天正文"), hits.len().to_string(), hits));
            }
        }
        sections
    }

    pub(super) fn search_messages(&mut self, query: &str, cx: &mut Context<Self>) {
        self.search.message_search = None;
        self.search.message_result = None;
        self.search.message_failed = false;
        self.search.opened_message = None;
        if query.is_empty() || !self.search.is_open() {
            return;
        }
        self.request_message_page(query.to_owned(), Vec::new(), true, cx);
    }

    pub(super) fn turn_message_page(&mut self, older: bool, cx: &mut Context<Self>) {
        if self.search.message_search.is_some() {
            return;
        }
        let Some(page) = self.search.message_result.as_ref() else {
            return;
        };
        let mut trail = page.trail.clone();
        if older {
            let Some(next) = &page.next else {
                return;
            };
            trail.push(next.clone());
        } else if trail.pop().is_none() {
            return;
        }
        self.request_message_page(page.query.clone(), trail, false, cx);
        self.pane_changed(cx);
    }

    fn request_message_page(
        &mut self,
        query: String,
        trail: Vec<MessagePosition>,
        typing: bool,
        cx: &mut Context<Self>,
    ) {
        self.search.message_failed = false;
        self.search.message_search = Some(cx.spawn(async move |this, cx| {
            if typing {
                cx.background_executor()
                    .timer(Duration::from_millis(160))
                    .await;
            }
            let asked = this.update(cx, |this, _| {
                let sessions = this
                    .searchable()
                    .into_iter()
                    .map(|s| s.id.clone())
                    .collect();
                this.store.send(Request::SearchMessages {
                    query: query.clone(),
                    sessions,
                    before: trail.last().cloned(),
                })
            });
            let Ok(asked) = asked else {
                return;
            };
            let reply = cx.background_executor().spawn(asked).await;
            let _ = this.update(cx, |this, cx| {
                if !this.search.is_open() || this.search_query(cx) != query {
                    return;
                }
                match reply {
                    Ok(Reply::MessageMatches { messages, next }) => {
                        this.search.message_result = Some(MessagePage {
                            query,
                            hits: messages,
                            trail,
                            next,
                        });
                        if !typing {
                            this.search.shown.remove(t!("聊天正文"));
                            this.search.index = this
                                .search_sections(&this.search_query(cx))
                                .iter()
                                .flat_map(|s| &s.hits)
                                .position(|hit| matches!(hit, Hit::Message(_)))
                                .unwrap_or(0);
                            this.search.scroll.scroll_to_item(this.search.index);
                        }
                    }
                    _ => this.search.message_failed = true,
                }
                this.search.message_search.take();
                this.pane_changed(cx);
            });
        }));
    }

    /// Where the selection starts: on the first session, so its preview shows at once;
    /// ↑ reaches 新建会话 above it.
    pub(super) fn first_recent_session_row(&self) -> usize {
        self.search_sections("")
            .iter()
            .flat_map(|s| &s.hits)
            .position(|h| matches!(h, Hit::Session { .. }))
            .unwrap_or(0)
    }
    pub(super) fn search_query(&self, cx: &gpui::App) -> String {
        self.search.field.read(cx).text.trim().to_owned()
    }
    fn selected_hit(&self, cx: &gpui::App) -> Option<Hit> {
        self.search_sections(&self.search_query(cx))
            .into_iter()
            .flat_map(|s| s.hits)
            .nth(self.search.index)
    }
    /// ↑ ↓ move through every row; Tab and ⇧Tab jump between groups.
    pub(super) fn search_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        let k = &event.keystroke;
        if self.search.opened_message.is_some() || self.search.field.read(cx).composing() {
            return;
        }
        if matches!(k.key.as_str(), "pageup" | "pagedown") {
            self.turn_message_page(k.key == "pagedown", cx);
            cx.stop_propagation();
            return;
        }
        if !matches!(k.key.as_str(), "up" | "down" | "tab") {
            return;
        }
        // Recency and the available actions can change between painting and this key.
        let sizes: Vec<_> = self
            .search_sections(&self.search_query(cx))
            .iter()
            .map(|s| s.hits.len())
            .collect();
        let len = sizes.iter().sum();
        self.search.index = match k.key.as_str() {
            "up" => step(self.search.index, len, -1),
            "down" => step(self.search.index, len, 1),
            "tab" => section_jump(&sizes, self.search.index, k.modifiers.shift),
            _ => return,
        };
        self.search.scroll.scroll_to_item(self.search.index);
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn open_hit(&mut self, hit: Hit, window: &mut Window, cx: &mut Context<Self>) {
        // 「再显示」 unfolds in place; the palette stays open on the same row.
        if let Hit::More { section, .. } = hit {
            *self.search.shown.entry(section).or_default() += MORE_STEP;
            cx.notify();
            return;
        }
        if !matches!(&hit, Hit::Message(_)) {
            self.search.hide();
        }
        match hit {
            Hit::NewSession => {
                self.nav.overview = false;
                self.launch(false, None, cx);
            }
            Hit::NextWaiting => self.next_waiting(window, cx),
            Hit::View(id) => self.choose_sidebar_preset(&id, cx),
            Hit::Machine(index) => self.switch_machine_quiet(index, cx),
            Hit::Space(id) => self.switch_space_quiet(&id, cx),
            Hit::Diagnostics => self.show_diagnostics(window, cx),
            Hit::Hooks => self.show_settings_page(settings::Page::General, window, cx),
            Hit::Settings(page) => self.show_settings_page(page, window, cx),
            Hit::Command(action) => {
                self.shortcut(action, window, cx);
            }
            Hit::Action(i) => {
                if let (Some(action), Some(pane)) =
                    (fibo_core::action::all().get(i), self.nav.focused.clone())
                {
                    self.run_action(&pane, action, cx);
                }
            }
            Hit::Message(hit) => {
                self.open_search_message(hit, cx);
                return;
            }
            Hit::Session { id, .. } => {
                if let Some(s) = self.workspace.agent_sessions.iter().find(|s| s.id == id) {
                    let s = s.clone();
                    let space = self.workspace.session_space(&s).map(str::to_owned);
                    self.close_peek(cx);
                    self.leave_host(cx);
                    self.reveal_machine(None, space.as_deref());
                    self.open_session(&s, window, cx);
                }
            }
            Hit::Task(id) => {
                if let Some(task) = self.workspace.tasks.iter().find(|t| t.id == id) {
                    self.nav.space = task.space.clone();
                    self.nav.overview = false;
                    self.nav.task_detail = Some(id);
                    self.sync_panes(cx);
                }
            }
            Hit::More { .. } => {}
        }
        self.nav.pending_focus = true;
        cx.notify();
    }
    /// ↵ opens the selected row; with nothing found it starts a session, as the empty
    /// result's button says.
    pub(super) fn open_selected_hit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.opened_message.is_some() {
            return;
        }
        match self.selected_hit(cx) {
            Some(hit) => self.open_hit(hit, window, cx),
            None if self.search.message_search.is_none()
                && !self.search.message_failed
                && !self.search_query(cx).is_empty() =>
            {
                self.open_hit(Hit::NewSession, window, cx)
            }
            None => {}
        }
    }
    pub(in crate::workbench) fn set_query(&mut self, text: &str, cx: &mut Context<Self>) {
        self.search.field.update(cx, |f, cx| {
            f.set_text(text);
            cx.notify();
        });
    }
}
