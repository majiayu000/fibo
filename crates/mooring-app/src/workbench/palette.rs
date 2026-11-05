//! The ⌘P search palette: a floating panel over the workbench with grouped results
//! on the left and the selected session's preview on the right
//! (Keepline repo, specs/terminal-workbench/design/palette-here.html).
use super::*;

mod list;
mod parts;
mod preview;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Hit {
    NewSession,
    NextWaiting,
    /// A session; `quote` is the latest of the user's own words that matched too.
    Session {
        id: String,
        quote: Option<String>,
    },
    /// The user's own words in a session whose title did not match.
    Quote {
        id: String,
        text: String,
    },
    Task(String),
    /// A plugin action, by its index in `mooring_core::action::all()`, run for the
    /// focused pane.
    Action(usize),
    /// A left-column view (目录, a plugin's), by preset id: picking it switches.
    View(String),
    /// 先不管 or 接着管 another machine, by its index in `Workbench::machines`.
    Machine(usize),
    /// 本机诊断: opens the CPU / GPU panel, otherwise only reachable from the sidebar foot.
    Diagnostics,
    /// 「再显示 N 个」: the rest of a folded group, `hidden` rows past what shows.
    More {
        section: &'static str,
        hidden: usize,
    },
}
pub(super) struct Section {
    pub title: Option<&'static str>,
    /// What the heading says at its right: the count, sometimes with a note.
    pub note: String,
    /// Every match, including the ones folded away behind 「再显示」.
    pub total: usize,
    pub hits: Vec<Hit>,
}
/// The palette's own state: whether it shows, its query field and the selected row.
pub(super) struct Search {
    open: bool,
    field: Entity<Field>,
    /// The selected row in the palette, counted across all groups.
    index: usize,
    scroll: gpui::ScrollHandle,
    /// The status count picked above an empty query: only that group is listed.
    group: Option<Status>,
    /// Rows a group shows past its first few, by group title, after 「再显示」.
    shown: HashMap<&'static str, usize>,
}
impl Search {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        let search = cx.new(|cx| {
            Field::new("搜索会话、待办或你说过的话", "", cx)
                .compact()
                .bare()
        });
        let mut last_query = String::new();
        cx.observe(&search, move |this, field, cx| {
            let query = field.read(cx).text.clone();
            if query != last_query {
                last_query = query;
                this.search.shown.clear();
                // A status pick narrows the recent list; typing searches everything.
                if !last_query.trim().is_empty() {
                    this.search.group = None;
                }
                this.search.index = this.first_session_row(last_query.trim());
                this.search.scroll.scroll_to_item(0);
            }
            cx.notify();
        })
        .detach();
        cx.subscribe_in(&search, window, |this, _, event, window, cx| match event {
            crate::ui::field::FieldEvent::Submit => this.open_selected_hit(window, cx),
            crate::ui::field::FieldEvent::Cancel => this.close_search(cx),
        })
        .detach();
        Self {
            open: false,
            field: search,
            index: 0,
            scroll: gpui::ScrollHandle::new(),
            group: None,
            shown: HashMap::new(),
        }
    }
    pub(super) fn is_open(&self) -> bool {
        self.open
    }
    /// Hides the palette without the refocus that closing it from its own keys does.
    pub(super) fn hide(&mut self) {
        self.open = false;
    }
    #[cfg(feature = "visual-test")]
    pub(super) fn field(&self) -> &Entity<Field> {
        &self.field
    }
    #[cfg(feature = "visual-test")]
    /// As a click on a status count: a picked group lists no actions, so its first
    /// session is the first row.
    pub(super) fn pick_group(&mut self, group: Option<Status>) {
        self.group = group;
        self.index = 0;
    }
}
/// Rows a group shows before 「再显示」, and how many more each press adds.
const COLLAPSED: usize = 6;
const MORE_STEP: usize = 20;
const EXCERPT: usize = 90;
/// The status counts above an empty query, as the top bar and 鸟瞰 name them.
const COUNTED: [Status; 4] = [
    Status::Waiting,
    Status::Error,
    Status::Running,
    Status::Review,
];
impl Workbench {
    /// Every session search can show, newest first.
    fn searchable(&self) -> Vec<&AgentSession> {
        let mut visible: Vec<&AgentSession> = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| self.session_hidden(s).is_none())
            .collect();
        visible.sort_by_key(|s| std::cmp::Reverse(self.workspace.session_updated(s)));
        visible
    }
    /// The status its row shows, when that is one of the four the palette counts. Only
    /// sessions still open count, as on 鸟瞰.
    fn counted_group(&self, s: &AgentSession) -> Option<Status> {
        let (group, _) = session_status(&self.workspace, &self.attention.unseen, s);
        let group = section(group);
        (s.outcome.needs_review(now()) && COUNTED.contains(&group)).then_some(group)
    }
    fn status_counts(&self) -> Vec<(Status, &'static str, usize)> {
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
                sections.push(self.section("待办", tasks.len().to_string(), tasks));
            }
            return sections;
        }
        let latest_ask = |s: &AgentSession| {
            self.workspace.content.get(&s.id).and_then(|c| {
                c.asks
                    .iter()
                    .rev()
                    .find(|q| contains(&q.text, query))
                    .map(|q| q.text.clone())
            })
        };
        let mut titled = HashSet::new();
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
                contains(
                    &format!(
                        "{} {first} {} {}",
                        self.session_title(s),
                        s.cli_session_id,
                        s.cwd
                    ),
                    query,
                )
            })
            .map(|s| {
                titled.insert(s.id.as_str());
                session(s, latest_ask(s))
            })
            .collect();
        // A session already listed carries its quote on its own row, not twice.
        let quotes: Vec<Hit> = visible
            .iter()
            .filter(|s| !titled.contains(s.id.as_str()))
            .filter_map(|s| {
                Some(Hit::Quote {
                    id: s.id.clone(),
                    text: latest_ask(s)?,
                })
            })
            .collect();
        let tasks: Vec<Hit> = self
            .workspace
            .tasks
            .iter()
            .filter(|t| contains(&format!("{} {}", t.title, t.description), query))
            .map(|t| Hit::Task(t.id.clone()))
            .collect();
        let pane = self.nav.focused.clone();
        let actions: Vec<Hit> = pane
            .map(|p| self.actions_for(&p))
            .unwrap_or_default()
            .into_iter()
            .filter(|a| contains(&format!("{} {}", a.title, a.command), query))
            .filter_map(|a| {
                mooring_core::action::all()
                    .iter()
                    .position(|b| std::ptr::eq(a, b))
                    .map(Hit::Action)
            })
            .collect();
        // 「视图」 or 「左栏」 lists every view; a view's own name finds it too.
        let views: Vec<Hit> = self
            .sidebar_presets()
            .filter(|p| contains(&format!("左栏 侧栏 视图 {}", p.name), query))
            .map(|p| Hit::View(p.id.clone()))
            .collect();
        let tools: Vec<Hit> = contains("本机诊断 诊断 卡顿 占用 CPU GPU 内存 diagnostics", query)
            .then_some(Hit::Diagnostics)
            .into_iter()
            .chain(self.machine_hits(query))
            .collect();
        for (title, hits) in [
            ("会话", sessions),
            ("原话", quotes),
            ("待办", tasks),
            ("动作", actions),
            ("左栏视图", views),
            ("工具", tools),
        ] {
            if !hits.is_empty() {
                let note = match title {
                    "原话" if !titled.is_empty() => format!("{} · 来自其他会话", hits.len()),
                    _ => hits.len().to_string(),
                };
                sections.push(self.section(title, note, hits));
            }
        }
        sections
    }
    /// Where the selection starts: on the first session, so its preview shows at once;
    /// ↑ reaches 新建会话 above it.
    fn first_session_row(&self, query: &str) -> usize {
        self.search_sections(query)
            .iter()
            .flat_map(|s| &s.hits)
            .position(|h| matches!(h, Hit::Session { .. } | Hit::Quote { .. }))
            .unwrap_or(0)
    }
    fn search_query(&self, cx: &gpui::App) -> String {
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
        if self.search.field.read(cx).composing() {
            return;
        }
        let k = &event.keystroke;
        let sizes: Vec<usize> = self
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
        self.search.open = false;
        match hit {
            Hit::NewSession => {
                self.nav.overview = false;
                self.launch(false, None, cx);
            }
            Hit::NextWaiting => self.next_waiting(window, cx),
            Hit::View(id) => self.choose_sidebar_preset(&id, cx),
            Hit::Machine(index) => self.switch_machine_quiet(index, cx),
            Hit::Diagnostics => self.show_diagnostics(window, cx),
            Hit::Action(i) => {
                if let (Some(action), Some(pane)) =
                    (mooring_core::action::all().get(i), self.nav.focused.clone())
                {
                    self.run_action(&pane, action, cx);
                }
            }
            Hit::Session { id, .. } | Hit::Quote { id, .. } => {
                if let Some(s) = self.workspace.agent_sessions.iter().find(|s| s.id == id) {
                    let s = s.clone();
                    self.enter_card(&s, window, cx);
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
        match self.selected_hit(cx) {
            Some(hit) => self.open_hit(hit, window, cx),
            None if !self.search_query(cx).is_empty() => self.open_hit(Hit::NewSession, window, cx),
            None => {}
        }
    }
    fn set_query(&mut self, text: &str, cx: &mut Context<Self>) {
        self.search.field.update(cx, |f, cx| {
            f.set_text(text);
            cx.notify();
        });
    }
    fn space_name(&self, id: &str) -> Option<String> {
        self.workspace
            .spaces
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
    }
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.open = true;
        self.search.group = None;
        self.search.shown.clear();
        self.search.index = self.first_session_row("");
        self.search.field.update(cx, |f, cx| {
            f.set_text("");
            cx.notify();
        });
        window.focus(&self.search.field.focus_handle(cx), cx);
        cx.notify();
    }
    pub(super) fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search.open = false;
        self.nav.pending_focus = true;
        cx.notify();
    }
}
