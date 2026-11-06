//! The pane header's details: its state, clock, place and branch, and the turn bar.
use super::*;
use mooring_core::git_head::read_branch;

/// A running turn's clock is redrawn once a second while one is on screen, and not at all
/// otherwise; one pending redraw serves every pane.
static TURN_TICK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const TURN_TICK_EVERY: Duration = Duration::from_secs(1);
/// Each directory's branch with when it was read.
type Branches = HashMap<String, (std::time::Instant, Option<String>)>;
/// Branches by directory, read off the render path and reread at most this often.
static BRANCHES: std::sync::LazyLock<std::sync::Mutex<Branches>> =
    std::sync::LazyLock::new(Default::default);
const BRANCH_TTL: Duration = Duration::from_secs(10);
impl Workbench {
    /// A pane's status as the sidebar shows it, and whether it asks for attention; a
    /// conversation already closed out and no longer live shows none.
    pub(in crate::workbench) fn pane_status(&self, id: &str) -> Option<(Status, bool)> {
        let s = self.pane_session(id)?;
        if live_terminal(&self.workspace, s).is_none() && !s.outcome.needs_review(now()) {
            return None;
        }
        Some(session_status(&self.workspace, &self.attention.unseen, s))
    }
    /// `claude · 5 分钟`, or `claude · 本轮 3:12` while a turn runs in a live terminal.
    pub(super) fn pane_meta(
        &self,
        session: Option<&AgentSession>,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let meta = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(T::S6))
            .text_size(px(T::META))
            .text_color(T::color(T::muted()))
            .font_features(T::tabular());
        let Some(s) = session else {
            return meta.child("Shell");
        };
        let content = self.workspace.content.get(&s.id);
        let started = content
            .filter(|_| live_terminal(&self.workspace, s).is_some())
            .and_then(mooring_core::content::Content::running_since);
        let when = match started {
            Some(at) => {
                self.tick_turn(cx);
                format!("本轮 {}", turn_clock(now().saturating_sub(at)))
            }
            None => crate::model::time::short_ago(content.map_or(s.updated, |c| c.updated), now()),
        };
        meta.child(super::board::agent_mark(s.runtime, T::color(T::muted())))
            .child(s.runtime.executable().to_string())
            .child(div().opacity(T::ZERO_OPACITY).child("·"))
            .child(when)
    }
    pub(in crate::workbench) fn tick_turn(&self, cx: &Context<Self>) {
        super::pace::wake(&TURN_TICK, TURN_TICK_EVERY, cx);
    }
    /// The cached branch for `dir`; a missing or old entry is read in the background and
    /// redraws the window only if it changed.
    pub(in crate::workbench) fn git_branch(&self, dir: &str, cx: &Context<Self>) -> Option<String> {
        if dir.is_empty() {
            return None;
        }
        let mut map = BRANCHES.lock().ok()?;
        let old = match map.get(dir) {
            Some((at, branch)) if at.elapsed() < BRANCH_TTL => return branch.clone(),
            Some((_, branch)) => branch.clone(),
            None => None,
        };
        // Marked as read now, so the redraws before the read lands do not read again.
        map.insert(dir.to_owned(), (std::time::Instant::now(), old.clone()));
        drop(map);
        let dir = dir.to_owned();
        cx.spawn(async move |this, cx| {
            let at = PathBuf::from(&dir);
            let branch = cx.background_spawn(async move { read_branch(&at) }).await;
            let changed = BRANCHES.lock().is_ok_and(|mut map| {
                let before = map.insert(dir, (std::time::Instant::now(), branch.clone()));
                before.and_then(|(_, b)| b) != branch
            });
            if changed {
                let _ = this.update(cx, |_, cx| cx.notify());
            }
        })
        .detach();
        old
    }
    /// The directory the pane's program is in: as the terminal reports it, else where its
    /// conversation or its terminal started.
    pub(super) fn pane_cwd(
        &self,
        id: &str,
        session: Option<&AgentSession>,
        cx: &Context<Self>,
    ) -> Option<String> {
        self.layout
            .panes
            .get(id)
            .and_then(|p| p.read(cx).cwd().map(|c| c.to_string_lossy().into_owned()))
            .or_else(|| session.map(|s| s.cwd.clone()).filter(|c| !c.is_empty()))
            .or_else(|| {
                self.workspace
                    .terminals
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.cwd.clone())
            })
            .filter(|c| !c.is_empty())
    }
    /// How long the pane's state has held, for its title: `本轮 3:12` while a turn runs in
    /// a live terminal, else the time since its last change (`34 分钟`).
    pub(super) fn pane_when(
        &self,
        session: Option<&AgentSession>,
        cx: &Context<Self>,
    ) -> Option<String> {
        let s = session?;
        let content = self.workspace.content.get(&s.id);
        let started = content
            .filter(|_| live_terminal(&self.workspace, s).is_some())
            .and_then(mooring_core::content::Content::running_since);
        Some(match started {
            Some(at) => {
                self.tick_turn(cx);
                format!("本轮 {}", turn_clock(now().saturating_sub(at)))
            }
            None => crate::model::time::short_ago(content.map_or(s.updated, |c| c.updated), now()),
        })
    }
    /// The files this turn changed, from the session's own record of its edits.
    pub(in crate::workbench) fn turn_files(&self, s: &AgentSession) -> usize {
        let Some(c) = self.workspace.content.get(&s.id) else {
            return 0;
        };
        c.turn_files().count()
    }
    /// The todo a pane's work counts toward: its conversation's own, else its tab's.
    pub(in crate::workbench) fn pane_task(
        &self,
        id: &str,
        s: Option<&AgentSession>,
    ) -> Option<&Task> {
        s.and_then(|s| s.task.as_ref())
            .or_else(|| {
                self.workspace
                    .terminal_tab(id)
                    .and_then(|t| t.default_task.as_ref())
            })
            .and_then(|tid| self.workspace.tasks.iter().find(|t| &t.id == tid))
    }
    /// Under a pane's title while its turn waits for you to check it: `这一轮改了 3 个文件 ·
    /// 验收标准 1/3`, then 标记完成, the only solid button in the window. To say what is
    /// wrong, type in the terminal below. A part with nothing to say is left out.
    pub(super) fn turn_bar(
        &self,
        id: &str,
        s: &AgentSession,
        g: Status,
        cx: &Context<Self>,
    ) -> gpui::Div {
        // The session's own state ink, as its title's chip has it.
        let ink = g.colors().1;
        let files = self.turn_files(s);
        let task = self.pane_task(id, Some(s));
        let mut facts = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(T::S6))
            .overflow_hidden()
            .font_features(T::tabular());
        facts = facts.child(
            div()
                .flex_shrink_0()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(ink)
                .child(if files > 0 {
                    format!("这一轮改了 {files} 个文件")
                } else {
                    "这一轮做完了".to_owned()
                }),
        );
        let content = self.workspace.content.get(&s.id);
        if let Some(lines) = content.map(|c| c.turn_lines()).filter(|(a, r)| a + r > 0) {
            facts = facts.child(line_counts(lines));
        }
        if let Some(task) = task.filter(|t| !t.acceptance.is_empty()) {
            let tid = task.id.clone();
            facts = facts
                .child(div().text_color(T::color(T::muted())).child("·"))
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("turn-criteria-{id}")))
                        .flex_shrink_0()
                        .cursor_pointer()
                        .text_color(T::color(T::secondary()))
                        .hover(|d| d.text_color(T::color(T::primary())))
                        .child(format!(
                            "验收标准 {}/{}",
                            task.proven(),
                            task.acceptance.len()
                        ))
                        .tooltip(|_, cx| {
                            cx.new(|_| super::render::TextTip("打开待办详情".into()))
                                .into()
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.nav.task_detail = Some(tid.clone());
                            cx.notify();
                        })),
                );
        }
        let terminal = id.to_owned();
        let accept = s.id.clone();
        let open = self.changes_shown(id);
        let toggle = id.to_owned();
        let shown = s.id.clone();
        // 看改动: what this turn did to its files, over the top half of the terminal.
        let changes = (files > 0).then(|| {
            button(
                gpui::SharedString::from(format!("turn-changes-{id}")),
                "看改动",
            )
            .h(px(T::PANE_ACTION))
            .px(px(T::S10))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            .text_color(T::color(T::primary()))
            .when(open, |d| d.bg(T::color(T::selection())))
            .on_click(cx.listener(move |this, _, window, cx| {
                let session = (this.workspace.agent_sessions.iter())
                    .find(|s| s.id == shown)
                    .cloned();
                if let Some(session) = session {
                    this.toggle_changes(&toggle, &session, window, cx);
                }
            }))
        });
        let bar = div()
            .h(px(T::TURN_BAR))
            .flex_shrink_0()
            .pl(px(T::S14))
            .pr(px(T::S10))
            .flex()
            .items_center()
            .gap(px(T::S8))
            .border_b(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            // A faint wash of the state's hue, as in the v3 design: the bar reads as the
            // turn's verdict, not as one more toolbar.
            .bg(g.colors().2.opacity(0.35))
            .text_size(px(T::SMALL))
            .child(facts)
            .children(changes)
            .child(super::forms::solid(
                button(
                    gpui::SharedString::from(format!("accept-pane-{id}")),
                    "标记完成",
                )
                .h(px(T::PANE_ACTION))
                .px(px(T::S10))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.layout.changes.remove(&terminal);
                    this.complete_pane(terminal.clone(), accept.clone(), cx)
                })),
            ));
        div().flex_shrink_0().flex().flex_col().child(bar)
    }
    /// `~/code/app · main`: where the pane's program is and on which branch.
    pub(super) fn pane_place(
        &self,
        id: &str,
        session: Option<&AgentSession>,
        cx: &Context<Self>,
    ) -> Option<String> {
        let cwd = self.pane_cwd(id, session, cx)?;
        Some(match self.git_branch(&cwd, cx) {
            Some(b) => format!("{} · {b}", home_relative(&cwd)),
            None => home_relative(&cwd),
        })
    }
    /// The title's hover card lines about the conversation: what it was asked, and where
    /// it stopped in full (the question, the error, else the latest reply's first line).
    pub(super) fn pane_story(&self, s: &AgentSession, g: Status) -> Vec<String> {
        let ask = self
            .workspace
            .content
            .get(&s.id)
            .and_then(|c| c.asks.first())
            .map(|q| {
                format!(
                    "你要它 {}",
                    mooring_core::content::short(q.text.trim(), 240)
                )
            });
        let at = self
            .attention_text(s, g)
            .map(|t| mooring_core::content::short(&mooring_core::content::plain_line(&t), 240))
            .or_else(|| self.row_detail(s, g))
            .map(|d| {
                if d.starts_with("停在") || d.starts_with("正在") {
                    d
                } else {
                    format!("停在 {d}")
                }
            });
        ask.into_iter().chain(at).collect()
    }
    /// Shown only while another window types into this pane: the way to take it back.
    pub(super) fn pane_footer(&self, id: &str, takeover: String, cx: &Context<Self>) -> gpui::Div {
        div()
            .h(px(T::FOOTER))
            .flex_shrink_0()
            .px(px(T::S10))
            .flex()
            .items_center()
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .text_size(px(T::PANE_FOOTER_SIZE))
            .text_color(T::color(T::muted()))
            .child(div().flex_1().child("另一个窗口在输入"))
            .child(
                button(
                    gpui::SharedString::from(format!("takeover-{id}")),
                    "点击接管输入",
                )
                .flex_shrink_0()
                .h(px(T::ICON_HEIGHT))
                .px(px(T::S6))
                .text_size(px(T::PANE_FOOTER_SIZE))
                .text_color(T::color(T::link()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    let pane = cx.new(|cx| Pane::new(this.store.socket(), &takeover, true, cx));
                    window.focus(&pane.focus_handle(cx), cx);
                    Self::adopt_pane(&pane, cx);
                    this.layout.panes.insert(takeover.clone(), pane);
                    cx.notify();
                })),
            )
    }
}
