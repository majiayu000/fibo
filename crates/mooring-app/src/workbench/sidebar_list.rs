//! The sessions part of the left column, arranged by the chosen preset: 经典, 状态, 目录
//! or a plugin's (`mooring_core::sidebar`). Every preset reads the same `session_status`,
//! so what needs you is the same row in all of them.
use super::*;
use mooring_core::sidebar::{self as presets, Bucket, Detail, Grouping, Preset};
/// How a row's second line opens when the session asks you something.
pub(super) const ASKS: &str = "问你：";

/// A session the column lists: its shown state and whether it still asks for a look.
pub(super) type Listed<'a> = (&'a AgentSession, Status, bool);
/// What a session row ends with: its state (or age, once seen), a text, or nothing.
pub(super) enum End {
    State,
    Text(String),
    Nothing,
}

/// 会话优先's sections: 要你处理 (asking, failed, or a finished turn to check), 在跑,
/// 停着 (stopped short, put off or just opened), and what has gone quiet, each newest
/// first. `when` is the turn's start for a running row, so two running rows never trade
/// places while they print, and its last update otherwise.
pub(super) struct RecentOrder<R> {
    pub need: Vec<R>,
    pub running: Vec<R>,
    pub stopped: Vec<R>,
    pub quiet: Vec<R>,
}
pub(super) fn recent_order<R: Copy>(rows: &[(R, Bucket, u64)]) -> RecentOrder<R> {
    let pick = |keep: &dyn Fn(Bucket) -> bool| {
        let mut picked: Vec<(R, u64)> = rows
            .iter()
            .filter(|(_, b, _)| keep(*b))
            .map(|(r, _, when)| (*r, *when))
            .collect();
        picked.sort_by_key(|(_, when)| std::cmp::Reverse(*when));
        picked.into_iter().map(|(r, _)| r).collect::<Vec<_>>()
    };
    RecentOrder {
        // What asks you or failed before the finished turns waiting for your check.
        need: [
            pick(&|b| b == Bucket::Attention),
            pick(&|b| b == Bucket::Done),
        ]
        .concat(),
        running: pick(&|b| b == Bucket::Running),
        stopped: pick(&|b| matches!(b, Bucket::Stopped | Bucket::Fresh)),
        quiet: pick(&|b| b == Bucket::Stale),
    }
}

/// Rows no longer moving, newest first, split at 24 hours: 今天 and 更早.
pub(super) fn split_today<R>(mut rows: Vec<(R, u64)>, now: u64) -> (Vec<R>, Vec<R>) {
    rows.sort_by_key(|(_, when)| std::cmp::Reverse(*when));
    let (today, older): (Vec<_>, Vec<_>) = rows
        .into_iter()
        .partition(|(_, when)| now.saturating_sub(*when) < 86_400);
    (
        today.into_iter().map(|(r, _)| r).collect(),
        older.into_iter().map(|(r, _)| r).collect(),
    )
}

pub(super) fn bucket(g: Status) -> Bucket {
    match g {
        Status::Waiting | Status::Error => Bucket::Attention,
        Status::Running => Bucket::Running,
        // A finished turn, seen or not: still yours to check. One that left a server or a
        // wakeup behind in the background is over too; its second line says what is left.
        Status::Review | Status::Background => Bucket::Done,
        // Marked complete, put off or interrupted: no longer moving on its own.
        Status::Paused | Status::Closed | Status::Deferred => Bucket::Stopped,
        Status::Fresh => Bucket::Fresh,
        Status::Stale => Bucket::Stale,
    }
}
/// The state a bucket's summary shows its dot in.
fn bucket_group(b: Bucket) -> Status {
    match b {
        Bucket::Attention => Status::Waiting,
        Bucket::Running => Status::Running,
        Bucket::Done => Status::Review,
        Bucket::Stopped => Status::Paused,
        Bucket::Fresh => Status::Fresh,
        Bucket::Stale => Status::Stale,
    }
}

/// The left column on this machine: the chosen preset, whether it is folded away, and
/// the groups you opened or closed, and the pinned terminals. The preset, the fold and
/// the pins are kept in `<config>/sidebar`; opened groups last until quit.
#[derive(Default)]
pub(super) struct SidebarPrefs {
    pub preset: String,
    pub collapsed: bool,
    /// Open (true) or closed, as you last left each group. A group you never touched
    /// follows its default, which may change with what is in it; one you touched stays
    /// as you left it (storing only "flipped" turned it over when the default moved).
    pub toggled: HashMap<String, bool>,
    /// WASM presets loaded so far, with their last answers.
    pub wasm: super::sidebar_wasm::Views,
    /// Terminals whose session sits in 置顶, in the order pinned; dropped when it ends.
    pub pinned: Vec<String>,
    /// The view menu opened from the ≑ beside 会话, where it was opened. Not saved.
    pub view_menu: Option<gpui::Point<gpui::Pixels>>,
    /// 会话优先 narrowed to one space's sessions by clicking its chip; None lists every
    /// space. Not saved: a new launch starts on everything.
    pub space_filter: Option<String>,
}
impl SidebarPrefs {
    fn path() -> Option<PathBuf> {
        crate::ui::status_colors::config_dir().map(|d| d.join("sidebar"))
    }
    pub fn load() -> Self {
        let text = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let mut lines = text.lines().map(str::trim);
        let preset = lines.next().unwrap_or_default().to_owned();
        let rest: Vec<&str> = lines.collect();
        Self {
            preset,
            collapsed: rest.contains(&"collapsed"),
            toggled: HashMap::new(),
            wasm: Default::default(),
            pinned: rest
                .iter()
                .filter_map(|l| l.strip_prefix("pin "))
                .map(str::to_owned)
                .collect(),
            view_menu: None,
            space_filter: None,
        }
    }
    /// Best effort: a choice that cannot be written still holds until quit.
    pub fn save(&self) {
        if let Some(path) = Self::path() {
            let _ = path.parent().map(std::fs::create_dir_all);
            let mut body = format!("{}\n", self.preset);
            if self.collapsed {
                body.push_str("collapsed\n");
            }
            for terminal in &self.pinned {
                body.push_str(&format!("pin {terminal}\n"));
            }
            // Written beside and renamed over: a crash mid-write keeps the old choices.
            let tmp = path.with_extension("tmp");
            let _ = std::fs::write(&tmp, body).and_then(|()| std::fs::rename(&tmp, &path));
        }
    }
    pub fn preset(&self) -> &'static Preset {
        presets::find(&self.preset)
    }
    /// Every preset but a flat list (经典) uses the denser look of sidebar-density.html:
    /// the spaces fold into one line, rows carry a state symbol and a time.
    pub fn dense(&self) -> bool {
        self.preset().group != Grouping::Flat
    }
    /// Whether a group is open: its default, flipped once per click.
    pub fn open(&self, key: &str, default: bool) -> bool {
        self.toggled.get(key).copied().unwrap_or(default)
    }
}

/// What a session row's right-click menu does.
#[derive(Clone)]
pub(super) enum SessionAction {
    Open,
    /// Pins or unpins this terminal.
    Pin(String),
    Rename,
    /// Closes this terminal's pane, asking first while its Agent runs.
    Close(String),
}

/// The last path segment, as a folder is named.
fn folder_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| cwd.to_owned())
}

impl Workbench {
    /// Opens a group shown closed, or closes one shown open, and keeps it that way.
    pub(super) fn toggle_sidebar_group(&mut self, key: &str, open: bool, cx: &mut Context<Self>) {
        self.sidebar.toggled.insert(key.to_owned(), !open);
        cx.notify();
    }
    pub(super) fn choose_sidebar_preset(&mut self, id: &str, cx: &mut Context<Self>) {
        self.sidebar.preset = id.to_owned();
        self.sidebar.toggled.clear();
        self.sidebar.save();
        cx.notify();
    }
    /// The session menu's 置顶: pins its terminal to 置顶, or takes it off.
    pub(super) fn toggle_pin(&mut self, terminal: &str, cx: &mut Context<Self>) {
        let pins = &mut self.sidebar.pinned;
        match pins.iter().position(|t| t == terminal) {
            Some(i) => {
                pins.remove(i);
            }
            None => pins.push(terminal.to_owned()),
        }
        self.sidebar.save();
        cx.notify();
    }
    /// A pin lasts as long as its terminal runs: one that ended or was closed comes off.
    /// Only against a workspace the host has sent, never the empty one before it.
    pub(super) fn prune_pins(&mut self) {
        if self.sidebar.pinned.is_empty() || self.workspace.revision == 0 || self.store.cut_off() {
            return;
        }
        let before = self.sidebar.pinned.len();
        let w = &self.workspace;
        self.sidebar.pinned.retain(|id| {
            w.terminals
                .iter()
                .any(|t| &t.id == id && t.state == ProcessState::Running)
        });
        if self.sidebar.pinned.len() != before {
            self.sidebar.save();
        }
    }
    /// ⌘B: the column folds to a rail and back, as Claude's desktop app does.
    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar.collapsed = !self.sidebar.collapsed;
        self.sidebar.save();
        cx.notify();
    }
    /// What a row's second line says: what it stopped at (the question it asks you, or
    /// the error), what it is doing in words (`正在改 tabs.rs`, never the raw command), or
    /// how its last turn ended. One line, cut to fit. The pane header's 停在 line reads
    /// the same.
    pub(super) fn row_detail(&self, s: &AgentSession, g: Status) -> Option<String> {
        let first = |t: &str| {
            let line = mooring_core::content::plain_line(t);
            (!line.is_empty()).then(|| line.chars().take(T::TAB_TIP_REASON_CHARS).collect())
        };
        let content = self.workspace.content.get(&s.id);
        match bucket(g) {
            // A question reads as one, in the v3 design's words: 「问你：侧栏参考哪两家？」.
            Bucket::Attention if g == Status::Waiting => self
                .attention_reason(s, g)
                .or_else(|| {
                    let text = self.attention_text(s, g)?;
                    Some(super::layout::ask_prompt(text.lines().next()?).to_owned())
                })
                .filter(|r| !r.is_empty())
                .map(|r| format!("{ASKS}{r}")),
            Bucket::Attention => self.attention_reason(s, g).map(|r| format!("停在 {r}")),
            Bucket::Running => content.and_then(|c| c.doing()).map(|d| format!("正在{d}")),
            _ if g == Status::Background => content.and_then(|c| c.background?.describe()),
            _ => content.and_then(|c| first(c.last_reply.lines().find(|l| !l.trim().is_empty())?)),
        }
    }
    /// Questions waiting in a row: how many the agent asked at once, at least one.
    fn asked(&self, s: &AgentSession) -> usize {
        self.workspace
            .content
            .get(&s.id)
            .map_or(1, |c| {
                c.waiting.lines().filter(|l| !l.trim().is_empty()).count()
            })
            .max(1)
    }
    fn session_row(
        &self,
        (session, g, attention): Listed,
        current: Option<&str>,
        detail: Option<String>,
        end: End,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        if self.sidebar.preset().group == Grouping::Recent {
            return self.flat_row((session, g, attention), current, detail, end, cx);
        }
        if self.sidebar.dense() {
            return self.dense_row((session, g, attention), current, detail, end, cx);
        }
        let open = session.clone();
        let seen = crate::ui::status_colors::StatusColors::get();
        // Every row and tab shows its CLI's mark, in its own color (v3 mock).
        let marks = true;
        let title = match self
            .layout
            .renaming_session
            .as_ref()
            .filter(|(id, ..)| id == &session.id)
        {
            Some((_, field, _)) => div()
                .flex_1()
                .min_w_0()
                .text_size(px(T::MONO_SIZE))
                .child(field.clone()),
            None => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(T::MONO_SIZE))
                .when(!attention, |d| d.text_color(T::color(seen.seen_title())))
                .child(self.session_title(session)),
        };
        // Rows that all wait for your next step tell apart by how long they have.
        let trailing = label(match end {
            End::Text(text) => text,
            End::Nothing => String::new(),
            End::State if g == Status::Review && !attention => {
                ago(self.session_updated(session), now())
            }
            End::State => g.label().to_owned(),
        })
        .flex_shrink_0()
        .text_size(px(T::CHIP_SIZE))
        .text_color(if attention {
            g.colors().1
        } else {
            T::color(seen.seen_status())
        });
        div()
            .id(gpui::SharedString::from(format!("session-{}", session.id)))
            .min_h(px(T::ROW))
            .px(px(T::S8))
            .flex()
            .flex_col()
            .justify_center()
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .when(current == Some(session.id.as_str()), |d| {
                d.bg(T::color(T::raised()))
            })
            .hover(|s| s.bg(T::color(T::raised())))
            .when(self.peek_home(&session.id), |d| {
                d.relative().child(super::peek::peek_home_outline())
            })
            .when(self.marking(&session.id), |d| {
                d.relative().child(super::complete::spot(
                    &self.layout.spots,
                    session.id.clone(),
                    (T::S8, 0.),
                    |s| &mut s.rows,
                ))
            })
            .child(
                div()
                    .h(px(T::ROW))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .when(marks, |d| {
                        d.child(super::board::agent_mark(
                            session.runtime,
                            T::color(T::muted()),
                        ))
                    })
                    .child(title)
                    .child(if attention { dot(g) } else { seen_dot(g) })
                    .child(trailing),
            )
            .when_some(detail, |d, text| {
                d.child(
                    div()
                        .mt(-px(T::S4))
                        .mb(px(T::S4))
                        .when(marks, |d| d.pl(px(T::BOARD_MARK + T::S8)))
                        .truncate()
                        .text_size(px(T::CHIP_SIZE))
                        .text_color(T::color(T::muted()))
                        .child(text),
                )
            })
            // The hover card would cover the right-click menu.
            .when(self.layout.session_menu.is_none(), |d| {
                d.tooltip(super::diagnostics::tip(self.session_tip(session)))
            })
            .on_mouse_down(
                gpui::MouseButton::Right,
                self.open_session_menu(session, cx),
            )
            // Straight to its pane; an exited one resumes in a new pane.
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.click_count() == 2 {
                        this.rename_session(&open.id, window, cx);
                        return;
                    }
                    this.peek_or_open(&open, window, cx)
                }),
            )
            .into_any_element()
    }
    /// Right-click on a session row: its menu at the pointer.
    pub(super) fn open_session_menu(
        &self,
        session: &AgentSession,
        cx: &Context<Self>,
    ) -> impl Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
        let id = session.id.clone();
        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
            this.layout.session_menu = Some((id.clone(), event.position));
            cx.stop_propagation();
            cx.notify();
        })
    }
    /// What the session menu lists, in order: 打开, then 置顶 / 取消置顶 for a session
    /// whose terminal runs (the pin being that terminal), 重命名, and 关闭终端.
    pub(super) fn session_menu_entries(&self, id: &str) -> Vec<(&'static str, SessionAction)> {
        let Some(session) = self.workspace.agent_sessions.iter().find(|s| s.id == id) else {
            return vec![];
        };
        let terminal = live_terminal(&self.workspace, session).map(|t| t.id.clone());
        let mut out = vec![("打开", SessionAction::Open)];
        if let Some(t) = &terminal {
            let pinned = self.sidebar.pinned.contains(t);
            out.push((
                if pinned { "取消置顶" } else { "置顶" },
                SessionAction::Pin(t.clone()),
            ));
        }
        out.push(("重命名", SessionAction::Rename));
        if let Some(t) = terminal {
            out.push(("关闭终端", SessionAction::Close(t)));
        }
        out
    }
    pub(super) fn session_menu_view(
        &self,
        id: &str,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let entries = self
            .session_menu_entries(id)
            .into_iter()
            .map(|(name, action)| {
                let id = id.to_owned();
                button(
                    gpui::SharedString::from(format!("session-menu-{name}")),
                    name,
                )
                .w_full()
                .justify_start()
                .border_0()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.layout.session_menu = None;
                    this.session_action(&id, &action, window, cx);
                    cx.notify();
                }))
                .into_any_element()
            })
            .collect();
        self.popup(position, entries, window, cx, |this| {
            this.layout.session_menu = None
        })
    }
    fn session_action(
        &mut self,
        id: &str,
        action: &SessionAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            SessionAction::Open => {
                if let Some(s) = self.workspace.agent_sessions.iter().find(|s| s.id == id) {
                    let s = s.clone();
                    self.open_session(&s, window, cx);
                }
            }
            SessionAction::Pin(terminal) => self.toggle_pin(terminal, cx),
            SessionAction::Rename => self.rename_session(id, window, cx),
            SessionAction::Close(terminal) => self.close_pane(terminal, cx),
        }
    }
    /// A row in the 状态 / 目录 presets (and plugin presets beside them), after
    /// sidebar-density.html: the CLI's mark, the title in the system font, then the
    /// running turn's clock or a short age (`4m`, `2h`) and the state symbol, both
    /// monospace. The second line comes for what needs you, what runs, and the selected row.
    fn dense_row(
        &self,
        (session, g, attention): Listed,
        current: Option<&str>,
        detail: Option<String>,
        end: End,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let open = session.clone();
        let seen = crate::ui::status_colors::StatusColors::get();
        let selected = current == Some(session.id.as_str());
        let mono = T::mono(cx).family;
        // Every row and tab shows its CLI's mark, in its own color (v3 mock).
        let marks = true;
        let title = match self
            .layout
            .renaming_session
            .as_ref()
            .filter(|(id, ..)| id == &session.id)
        {
            Some((_, field, _)) => div().flex_1().min_w_0().child(field.clone()),
            None => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(T::UI_FONT)
                .text_size(px(T::SMALL))
                .text_color(if attention || selected {
                    T::color(T::primary())
                } else {
                    T::color(seen.seen_title())
                })
                .child(self.session_title(session)),
        };
        let time = match end {
            End::Text(text) => text,
            End::Nothing => String::new(),
            End::State => match self.turn_started(session) {
                Some(at) => {
                    self.tick_turn(cx);
                    super::layout::turn_clock(now().saturating_sub(at))
                }
                None => short_ago(self.session_updated(session), now()),
            },
        };
        // A finished turn not seen yet is a filled green dot; once seen it keeps
        // 等你验收's ring and its green.
        let state = || {
            div()
                .w(px(T::S12))
                .flex_shrink_0()
                .flex()
                .justify_center()
                .font_family(mono.clone())
                .font_weight(gpui::FontWeight::BOLD)
                .text_size(px(T::SMALL))
                .child(crate::ui::widgets::status_mark_seen(g, attention))
        };
        div()
            .id(gpui::SharedString::from(format!("session-{}", session.id)))
            .px(px(T::S8))
            .py(px(T::S4))
            .flex()
            .flex_col()
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .when(selected, |d| d.bg(T::color(T::raised())))
            .hover(|s| s.bg(T::color(T::raised())))
            .when(self.peek_home(&session.id), |d| {
                d.relative().child(super::peek::peek_home_outline())
            })
            .when(self.marking(&session.id), |d| {
                d.relative().child(super::complete::spot(
                    &self.layout.spots,
                    session.id.clone(),
                    (T::S8, T::S4),
                    |s| &mut s.rows,
                ))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .when(marks, |d| {
                        d.child(super::board::agent_mark(
                            session.runtime,
                            T::color(T::muted()),
                        ))
                    })
                    .child(title)
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(mono.clone())
                            .font_features(T::tabular())
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::muted()))
                            .child(time),
                    )
                    .child(state()),
            )
            .when_some(detail, |d, text| {
                // A second line that opens with the state word shows that word in its ink;
                // a question is in 等你回答's ink throughout, with how many it asks.
                let asks = text.starts_with(ASKS);
                let (state, rest) = match text.strip_prefix(g.label()) {
                    Some(rest) => (Some(g.label()), rest.to_owned()),
                    None => (None, text),
                };
                // What the turn changed leads, as the v3 rows have it: +128 −14.
                let lines = self
                    .workspace
                    .content
                    .get(&session.id)
                    .map(|c| c.turn_lines())
                    .filter(|(a, r)| a + r > 0 && !asks);
                d.child(
                    div()
                        .when(marks, |d| d.pl(px(T::BOARD_MARK + T::S8)))
                        .flex()
                        .items_center()
                        .gap(px(T::S4))
                        .min_w_0()
                        .font_family(T::UI_FONT)
                        .text_size(px(T::CHIP_SIZE))
                        .text_color(if asks {
                            g.colors().1
                        } else {
                            T::color(T::secondary())
                        })
                        .when_some(lines, |d, lines| {
                            d.child(super::layout::line_counts(lines))
                                .child(div().text_color(T::color(T::muted())).child("·"))
                        })
                        .when_some(state, |d, word| {
                            d.child(div().flex_shrink_0().text_color(g.colors().1).child(word))
                        })
                        .child(div().flex_1().min_w_0().truncate().child(rest))
                        .when(asks && attention, |d| {
                            d.child(
                                div()
                                    .flex_shrink_0()
                                    .size(px(T::S14))
                                    .rounded_full()
                                    .bg(g.colors().0)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(T::color(T::terminal_bg()))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(self.asked(session).to_string()),
                            )
                        }),
                )
            })
            // The hover card would cover the right-click menu.
            .when(self.layout.session_menu.is_none(), |d| {
                d.tooltip(super::diagnostics::tip(self.session_tip(session)))
            })
            .on_mouse_down(
                gpui::MouseButton::Right,
                self.open_session_menu(session, cx),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.click_count() == 2 {
                        this.rename_session(&open.id, window, cx);
                        return;
                    }
                    this.peek_or_open(&open, window, cx)
                }),
            )
            .into_any_element()
    }
    /// A group's heading in the 状态 / 目录 presets: small, its count in monospace, no rule.
    pub(super) fn group_title(
        title: String,
        count: Option<usize>,
        cx: &Context<Self>,
    ) -> gpui::Div {
        Self::tinted_title(title, count, T::color(T::secondary()), cx)
    }
    /// A group's heading whose name is in `ink`: 要你处理 and 在跑 in their states' ink.
    pub(super) fn tinted_title(
        title: String,
        count: Option<usize>,
        ink: gpui::Hsla,
        cx: &Context<Self>,
    ) -> gpui::Div {
        div()
            .flex()
            .items_baseline()
            .gap(px(T::S6))
            .px(px(T::S8))
            .mt(px(T::S14))
            .mb(px(T::S2))
            .text_size(px(T::CHIP_SIZE))
            .child(
                div()
                    .font_family(T::UI_FONT)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(ink)
                    .child(title),
            )
            .when_some(count, |d, n| {
                d.child(
                    div()
                        .font_family(T::mono(cx).family)
                        .text_color(T::color(T::muted()))
                        .child(n.to_string()),
                )
            })
    }
    pub(super) fn section_title(title: String, count: usize) -> gpui::Div {
        div()
            .flex()
            .px(px(T::S8))
            .mt(px(T::S12))
            .mb(px(T::S2))
            .child(label(title).flex_1().text_size(px(T::CHIP_SIZE)))
            .child(label(count.to_string()).text_size(px(T::CHIP_SIZE)))
    }
    /// A 会话优先 section's heading that folds its rows away and opens them again:
    /// `要你处理 3 ⌄`. Every section starts open.
    fn flat_fold(
        &self,
        key: &str,
        title: &str,
        count: usize,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let open = self.sidebar.open(key, true);
        let key = key.to_owned();
        Self::flat_group(title, count)
            .id(gpui::SharedString::from(format!("sidebar-group-{key}")))
            .cursor_pointer()
            // At the line's right end, level with the rows' times below it.
            .child(div().ml_auto().child(super::sidebar_flat::fold_mark(open)))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_sidebar_group(&key, open, cx)))
            .into_any_element()
    }
    /// A group's heading in the chosen preset's look.
    pub(super) fn heading(&self, title: String, count: usize, cx: &Context<Self>) -> gpui::Div {
        if self.sidebar.dense() {
            Self::group_title(title, Some(count), cx)
        } else {
            Self::section_title(title, count)
        }
    }
    /// One row standing for folded buckets; a click opens them. Under a heading it reads
    /// `◇ 等你验收 4  ‖ 停下了 3  很久没动 165`, each with its state's symbol; inside a folder
    /// it is the quieter `另有 等你验收 2 · 停下了 1`.
    fn fold_row(
        &self,
        id: String,
        parts: &[(Bucket, usize)],
        open: bool,
        prefix: &str,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let key = id.clone();
        let mono = T::mono(cx).family;
        let row = div()
            .id(gpui::SharedString::from(format!("sidebar-fold-{id}")))
            .min_h(px(T::ROW))
            .px(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S4))
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .hover(|s| s.bg(T::color(T::raised())))
            .font_family(T::UI_FONT)
            .text_size(px(if prefix.is_empty() {
                T::META
            } else {
                T::CHIP_SIZE
            }))
            .text_color(T::color(if prefix.is_empty() {
                T::secondary()
            } else {
                T::muted()
            }));
        // The parts give way before the chevron does in a narrow column.
        let mut line = div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap_x(px(if prefix.is_empty() { T::S10 } else { T::S6 }));
        if !prefix.is_empty() {
            line = line.child(prefix.to_owned());
        }
        for (i, (b, n)) in parts.iter().enumerate() {
            let count = div()
                .font_family(mono.clone())
                .text_color(T::color(if prefix.is_empty() {
                    T::primary()
                } else {
                    T::muted()
                }))
                .child(n.to_string());
            let mut part = div().flex_shrink_0().flex().items_center().gap(px(T::S4));
            if prefix.is_empty() {
                if *b != Bucket::Stale {
                    part = part.child(glyph(bucket_group(*b)).font_family(mono.clone()));
                }
            } else if i > 0 {
                line = line.child("·");
            }
            line = line.child(part.child(b.label()).child(count));
        }
        row.child(line)
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(T::color(T::muted()))
                    .child(if open { "⌄" } else { "›" }),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_sidebar_group(&key, open, cx)))
            .into_any_element()
    }
    /// The session rows for the current space, arranged by the chosen preset; returns the
    /// rows, how many sessions were left for 鸟瞰, and whether the closed ones were placed
    /// by the preset (a WASM one) rather than left to the summary under the list.
    pub(super) fn session_list(
        &self,
        sessions: Vec<Listed>,
        closed: &super::sidebar_wasm::Closed,
        current: Option<&str>,
        cx: &Context<Self>,
    ) -> (Vec<gpui::AnyElement>, usize, bool) {
        let preset = self.sidebar.preset();
        let detail = |l: &Listed| match preset.detail {
            Detail::Never => None,
            Detail::Always => self.row_detail(l.0, l.1),
            Detail::Attention => (matches!(bucket(l.1), Bucket::Attention | Bucket::Running)
                || l.1 == Status::Background
                || current == Some(l.0.id.as_str()))
            .then(|| self.row_detail(l.0, l.1))
            .flatten(),
        };
        let mut out = vec![];
        // 置顶 comes first in every preset; its rows leave their usual group.
        let pinned_terminal = |l: &Listed| {
            l.0.terminal
                .as_ref()
                .filter(|t| {
                    self.sidebar.pinned.contains(t) && live_terminal(&self.workspace, l.0).is_some()
                })
                .cloned()
        };
        let (mut top, sessions): (Vec<Listed>, Vec<Listed>) = sessions
            .into_iter()
            .partition(|l| pinned_terminal(l).is_some());
        let all_pinned = !top.is_empty() && sessions.is_empty();
        if !top.is_empty() {
            top.sort_by_key(|l| {
                pinned_terminal(l).and_then(|t| self.sidebar.pinned.iter().position(|p| *p == t))
            });
            let recent = preset.group == Grouping::Recent;
            out.push(if recent {
                self.flat_fold("recent:pinned", "置顶", top.len(), cx)
            } else {
                self.heading("置顶".into(), top.len(), cx)
                    .mt(px(T::S14))
                    .into_any_element()
            });
            let open = !recent || self.sidebar.open("recent:pinned", true);
            for l in top {
                // The session you are in stays in sight while its group is folded.
                if open || current == Some(l.0.id.as_str()) {
                    let d = detail(&l);
                    out.push(self.session_row(l, current, d, End::State, cx));
                }
            }
        }
        let mut budget = preset.limit;
        let mut more = 0;
        let mut owns_closed = false;
        let mut push = |out: &mut Vec<gpui::AnyElement>, l: Listed, d: Option<String>, e: End| {
            if budget == 0 && current != Some(l.0.id.as_str()) {
                more += 1;
                return;
            }
            budget = budget.saturating_sub(1);
            out.push(self.session_row(l, current, d, e, cx));
        };
        match preset.group {
            Grouping::Flat if all_pinned => (),
            Grouping::Flat => {
                for l in sessions {
                    let d = detail(&l);
                    push(&mut out, l, d, End::State);
                }
            }
            Grouping::Status => {
                let folded: Vec<(Bucket, usize)> = Bucket::ALL
                    .into_iter()
                    .filter(|b| preset.folds(*b))
                    .map(|b| (b, sessions.iter().filter(|l| bucket(l.1) == b).count()))
                    .filter(|(_, n)| *n > 0)
                    .collect();
                let rest_open = self.sidebar.open("status:rest", false);
                for b in Bucket::ALL.into_iter().filter(|b| !preset.folds(*b)) {
                    let rows: Vec<_> = sessions.iter().filter(|l| bucket(l.1) == b).collect();
                    if rows.is_empty() {
                        continue;
                    }
                    out.push(
                        self.heading(b.label().into(), rows.len(), cx)
                            .into_any_element(),
                    );
                    for l in rows {
                        push(&mut out, *l, detail(l), End::State);
                    }
                }
                if !folded.is_empty() {
                    out.push(
                        div()
                            .mt(px(T::S12))
                            .child(self.fold_row("status:rest".into(), &folded, rest_open, "", cx))
                            .into_any_element(),
                    );
                }
                // Opened, in the summary's order: done, stopped, then gone quiet.
                let mut rest: Vec<_> = sessions
                    .iter()
                    .filter(|l| preset.folds(bucket(l.1)))
                    .collect();
                rest.sort_by_key(|l| Bucket::ALL.iter().position(|b| *b == bucket(l.1)));
                for l in rest {
                    // The session you are in stays in sight while its group is folded.
                    if rest_open || current == Some(l.0.id.as_str()) {
                        push(&mut out, *l, detail(l), End::State);
                    }
                }
            }
            Grouping::Recent => {
                let when = |l: &Listed| match bucket(l.1) {
                    Bucket::Running => self
                        .turn_started(l.0)
                        .unwrap_or_else(|| self.session_updated(l.0)),
                    _ => self.session_updated(l.0),
                };
                let rows: Vec<(Listed, Bucket, u64)> = sessions
                    .iter()
                    .map(|l| (*l, bucket(l.1), when(l)))
                    .collect();
                let RecentOrder {
                    need,
                    running,
                    stopped,
                    quiet,
                } = recent_order(&rows);
                // What it asks or why it failed; with neither to show, what it is at now.
                let second = |l: &Listed| {
                    self.row_detail(l.0, l.1).or_else(|| {
                        self.workspace
                            .content
                            .get(&l.0.id)
                            .map(|c| mooring_core::content::short(&c.headline(), 100))
                            .filter(|h| !h.trim().is_empty())
                    })
                };
                // Only what asks you or failed gets a second line: the question is what you
                // act on. A finished turn or a running one reads from its state mark alone,
                // so 要你处理 and 在跑 fit one screen even with dozens of sessions.
                for (key, title, rows, lines) in [
                    ("recent:need", "要你处理", need, true),
                    ("recent:running", "在跑", running, false),
                ] {
                    if rows.is_empty() {
                        continue;
                    }
                    out.push(self.flat_fold(key, title, rows.len(), cx));
                    let open = self.sidebar.open(key, true);
                    for l in rows {
                        if !open && current != Some(l.0.id.as_str()) {
                            continue;
                        }
                        // 等后台 says what it left running, or it reads as plain 等你验收.
                        let asks = bucket(l.1) == Bucket::Attention || l.1 == Status::Background;
                        let d = if lines && asks { second(&l) } else { None };
                        push(&mut out, l, d, End::State);
                    }
                }
                // What is no longer moving, open in a tab or closed: today's in full, the
                // rest folded into one line, as flat.html has it. These rows skip the
                // preset's limit, which is there to keep 要你处理 and 在跑 on one screen.
                let history: Vec<(Listed, u64)> = stopped
                    .into_iter()
                    .chain(quiet)
                    .chain(closed.rows.iter().copied())
                    .map(|l| (l, self.session_updated(l.0)))
                    .collect();
                let (today, older) = split_today(history, crate::model::time::now());
                if !today.is_empty() {
                    out.push(self.flat_fold("recent:today", "今天", today.len(), cx));
                    let open = self.sidebar.open("recent:today", true);
                    for l in today {
                        if open || current == Some(l.0.id.as_str()) {
                            out.push(self.session_row(l, current, None, End::State, cx));
                        }
                    }
                }
                // Older ones a module was not handed are still counted, and open in 鸟瞰.
                let unlisted = closed.total.saturating_sub(closed.rows.len());
                if !older.is_empty() || unlisted > 0 {
                    // Narrowed to one space the list is short, so it opens by itself.
                    let open = self
                        .sidebar
                        .open("recent:older", self.space_filter().is_some());
                    out.push(self.flat_more(
                        "recent:older",
                        format!("更早 {} 个 · 停着或做完了", older.len() + unlisted),
                        open,
                        cx,
                    ));
                    for l in older {
                        if open || current == Some(l.0.id.as_str()) {
                            out.push(self.session_row(l, current, None, End::State, cx));
                        }
                    }
                    if open && unlisted > 0 {
                        out.push(self.flat_closed_more(unlisted, cx));
                    }
                }
                owns_closed = true;
            }
            Grouping::Wasm => {
                owns_closed =
                    self.wasm_rows(preset, sessions, closed, current, &mut push, &mut out, cx);
            }
            Grouping::Directory => {
                // What needs you is listed once, on top, with its folder in its second
                // line; the folders below only count it.
                let pinned: Vec<_> = sessions
                    .iter()
                    .filter(|l| bucket(l.1) == Bucket::Attention)
                    .collect();
                if !pinned.is_empty() {
                    out.push(
                        self.heading(mooring_core::status::NEEDS_YOU.into(), pinned.len(), cx)
                            .into_any_element(),
                    );
                    for l in &pinned {
                        let d = detail(l).map(|d| format!("{} · {d}", folder_name(&l.0.cwd)));
                        push(&mut out, **l, d, End::State);
                    }
                }
                // Folders in the order their first session sorts: busy ones first.
                let mut folders: Vec<(&str, Vec<Listed>)> = vec![];
                for l in &sessions {
                    match folders.iter_mut().find(|(cwd, _)| *cwd == l.0.cwd) {
                        Some((_, rows)) => rows.push(*l),
                        None => folders.push((l.0.cwd.as_str(), vec![*l])),
                    }
                }
                // Open folders (something running) first, then those holding what needs
                // you, then the quiet ones; otherwise as their first session sorts.
                folders.sort_by_key(|(_, rows)| {
                    let has = |b: Bucket| rows.iter().any(|l| bucket(l.1) == b);
                    (!has(Bucket::Running), !has(Bucket::Attention))
                });
                if !folders.is_empty() {
                    out.push(Self::group_title("目录".into(), None, cx).into_any_element());
                }
                // A folder's rows sit in from its name.
                let indent = |out: &mut Vec<gpui::AnyElement>, from: usize| {
                    let rows: Vec<_> = out.drain(from..).collect();
                    out.extend(
                        rows.into_iter()
                            .map(|r| div().pl(px(T::S14)).child(r).into_any_element()),
                    );
                };
                for (cwd, rows) in folders {
                    let needs = rows
                        .iter()
                        .filter(|l| bucket(l.1) == Bucket::Attention)
                        .count();
                    let running = rows.iter().any(|l| bucket(l.1) == Bucket::Running);
                    let key = format!("dir:{cwd}");
                    let open = self.sidebar.open(&key, running);
                    out.push(self.folder_row(cwd, &key, rows.len(), needs, open, running, cx));
                    if !open {
                        continue;
                    }
                    let from = out.len();
                    let mut folded: Vec<(Bucket, usize)> = vec![];
                    for l in rows.iter().filter(|l| bucket(l.1) != Bucket::Attention) {
                        let b = bucket(l.1);
                        if preset.folds(b) {
                            match folded.iter_mut().find(|(f, _)| *f == b) {
                                Some((_, n)) => *n += 1,
                                None => folded.push((b, 1)),
                            }
                        } else {
                            push(&mut out, *l, detail(l), End::State);
                        }
                    }
                    folded.sort_by_key(|(b, _)| Bucket::ALL.iter().position(|a| a == b));
                    if !folded.is_empty() {
                        let more_key = format!("dir-more:{cwd}");
                        let more_open = self.sidebar.open(&more_key, false);
                        out.push(self.fold_row(more_key, &folded, more_open, "另有", cx));
                        if more_open {
                            for l in rows.iter().filter(|l| {
                                bucket(l.1) != Bucket::Attention && preset.folds(bucket(l.1))
                            }) {
                                push(&mut out, *l, detail(l), End::State);
                            }
                        }
                    }
                    indent(&mut out, from);
                }
            }
        }
        (out, more, owns_closed)
    }
    /// A folder's heading: its name, `1 需要你` in the error ink when something in it
    /// needs you, and its session count. A folder with nothing running or waiting is
    /// folded and dimmed; its path shows only while the pointer is on it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn folder_row(
        &self,
        cwd: &str,
        key: &str,
        count: usize,
        needs: usize,
        open: bool,
        active: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let key = key.to_owned();
        let quiet = !active && needs == 0;
        let row_group = gpui::SharedString::from(format!("sidebar-{key}"));
        let mono = T::mono(cx).family;
        div()
            .id(row_group.clone())
            .group(row_group.clone())
            .h(px(T::ROW))
            .px(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S6))
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .hover(|s| s.bg(T::color(T::raised())))
            .font_family(T::UI_FONT)
            .text_size(px(T::SMALL))
            .child(
                div()
                    .w(px(T::S12))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .child(if open { "▾" } else { "▸" }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(px(T::SIDEBAR * 0.6))
                    .truncate()
                    .when(!quiet, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                    .text_color(T::color(if quiet { T::muted() } else { T::primary() }))
                    .child(folder_name(cwd)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .opacity(0.)
                    .group_hover(row_group, |s| s.opacity(1.))
                    .child(home_relative(cwd)),
            )
            .when(needs > 0, |d| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(T::CHIP_SIZE))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(Tone::Danger.ink())
                        .child(format!("{needs} {}", mooring_core::status::NEEDS_YOU)),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(mono)
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_sidebar_group(&key, open, cx)))
            .into_any_element()
    }
    /// The switch over the list: one segment per preset.
    /// 会话 and its count, heading the list in every preset. The ≑ for choosing the
    /// view shows while the pointer is over the row, or while its menu is open, like the
    /// filter button in Claude's desktop sidebar; it sits on this one sidebar-wide row so
    /// it never reads as belonging to a single group.
    pub(super) fn sessions_header(&self, count: usize, cx: &Context<Self>) -> gpui::Div {
        let open = self.sidebar.view_menu.is_some();
        // Inside the heading's own padding, so its right edge stays in line with the
        // rows' 在跑 / 几分钟前 and no extra column opens on the right.
        let icon = div()
            .id("sidebar-view")
            .role(gpui::accesskit::Role::Button)
            .aria_label("选择视图")
            .my(px(-T::S4))
            .size(px(T::ICON_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .opacity(if open { 1. } else { T::VIEW_IDLE_OPACITY })
            .group_hover("sessions-header", |s| s.opacity(1.))
            .when(open, |d| d.bg(T::color(T::button_hover())))
            .hover(|s| s.bg(T::color(T::button_hover())))
            .child(
                gpui::svg()
                    .path("icons/sliders.svg")
                    .size(px(T::VIEW_GLYPH))
                    .text_color(T::color(T::secondary())),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    this.sidebar.view_menu = match this.sidebar.view_menu {
                        Some(_) => None,
                        None => Some(gpui::point(event.position.x, event.position.y + px(T::S12))),
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        let head = if self.sidebar.preset().group == Grouping::Recent {
            // 会话优先 has no heading: its sections name themselves, and the view menu
            // sits in the column's foot.
            div()
        } else if self.sidebar.dense() {
            // 会话 3 on the left as the dense presets write it; the ≑ at the right end.
            Self::group_title("会话".into(), Some(count), cx)
                .child(div().flex_1())
                .child(icon)
        } else {
            // 会话 … ≑ 8: the count keeps its column, the ≑ sits just before it.
            div()
                .flex()
                .items_center()
                .px(px(T::S8))
                .mt(px(T::S12))
                .mb(px(T::S2))
                .child(label("会话").flex_1().text_size(px(T::CHIP_SIZE)))
                .child(icon.mr(px(T::S4)))
                .child(label(count.to_string()).text_size(px(T::CHIP_SIZE)))
        };
        head.group("sessions-header")
    }
    /// The views to pick from, the current one ticked; picking one closes the menu.
    pub(super) fn view_menu_view(
        &self,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let chosen = self.sidebar.preset().id.clone();
        let entries = presets::all()
            .iter()
            .map(|p| {
                let id = p.id.clone();
                let on = p.id == chosen;
                div()
                    .id(gpui::SharedString::from(format!("sidebar-preset-{}", p.id)))
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(p.name.clone())
                    .h(px(T::ROW))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .rounded(px(T::CONTROL_RADIUS))
                    .text_size(px(T::SMALL))
                    .cursor_pointer()
                    .hover(|s| s.bg(T::color(T::button_hover())))
                    .child(div().flex_1().child(p.name.clone()))
                    .when(on, |d| {
                        d.child(div().text_color(T::color(T::link())).child("✓"))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sidebar.view_menu = None;
                        this.choose_sidebar_preset(&id, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        self.popup(position, entries, window, cx, |this| {
            this.sidebar.view_menu = None
        })
    }
}

#[cfg(test)]
mod recent_tests {
    use super::*;

    #[test]
    fn what_needs_you_leads_then_running_then_stopped_and_quiet_last() {
        let rows = [
            ("old-review", Bucket::Done, 10),
            ("asking", Bucket::Attention, 5),
            ("running", Bucket::Running, 30),
            ("quiet", Bucket::Stale, 1),
            ("failed", Bucket::Attention, 40),
            ("new-review", Bucket::Done, 50),
        ];
        let order = recent_order(&rows);
        assert_eq!(order.need, ["failed", "asking", "new-review", "old-review"]);
        assert_eq!(order.running, ["running"]);
        assert!(order.stopped.is_empty());
        assert_eq!(order.quiet, ["quiet"]);
    }

    #[test]
    fn history_splits_at_a_day_newest_first() {
        let now = 10 * 86_400;
        let rows = vec![
            ("two-days", now - 2 * 86_400),
            ("hour", now - 3_600),
            ("minute", now - 60),
            ("day-and-a-bit", now - 86_400),
        ];
        let (today, older) = split_today(rows, now);
        assert_eq!(today, ["minute", "hour"]);
        assert_eq!(older, ["day-and-a-bit", "two-days"]);
    }
}
