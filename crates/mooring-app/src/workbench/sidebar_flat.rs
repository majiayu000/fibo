//! 会话优先's column drawn to docs/sidebar-redesign/flat.html size for size: the brand
//! row, a search field, the space chips, one list whose rows lead with their state and
//! end with their space and time, a key hint and a one-line foot.
use super::sidebar_list::{ASKS, End, Listed, bucket};
use super::*;
use crate::ui::theme::Flat as F;
use mooring_core::sidebar::Bucket;

fn ink(v: u32) -> gpui::Hsla {
    T::color(v)
}

/// The fold mark at a heading's right end: a chevron down while open, right when folded.
pub(super) fn fold_mark(open: bool) -> gpui::Svg {
    gpui::svg()
        .path(if open {
            "icons/chevron-down.svg"
        } else {
            "icons/chevron-right.svg"
        })
        .size(px(F::STATE + 2.))
        .flex_shrink_0()
        .text_color(ink(F::third()))
}

/// A row's state at its head: a lit dot for what asks you or failed, an open ring with a
/// gap while it runs, a ring for a turn to check (filled while you have not looked at
/// it), a gray ✓ for what you marked complete and a gray ring for the rest.
pub(super) fn flat_state(g: Status, unseen: bool) -> gpui::AnyElement {
    let dot = g.colors().0;
    let svg = |path: &'static str, color: gpui::Hsla| {
        gpui::svg()
            .path(path)
            .size(px(F::STATE))
            .flex_shrink_0()
            .text_color(color)
            .into_any_element()
    };
    // By state, not bucket: 要你处理 also holds 等你验收 and 等后台 (11-03), which keep
    // their ring once you have looked.
    match g {
        Status::Waiting | Status::Error => div()
            .size(px(F::STATE))
            .flex_shrink_0()
            .rounded_full()
            .bg(dot)
            .shadow(vec![gpui::BoxShadow {
                color: dot.opacity(0.75),
                offset: gpui::point(px(0.), px(0.)),
                blur_radius: px(F::STATE),
                spread_radius: px(0.),
                inset: false,
            }])
            .into_any_element(),
        Status::Running => svg("icons/state-run.svg", dot),
        Status::Review | Status::Background if unseen => div()
            .size(px(F::STATE))
            .flex_shrink_0()
            .rounded_full()
            .bg(dot)
            .into_any_element(),
        Status::Review | Status::Background => svg("icons/state-ring.svg", dot),
        // What you marked complete is done with: a muted ✓, apart from rings that stopped.
        Status::Closed => svg("icons/state-done.svg", ink(F::done())),
        _ => svg("icons/state-ring.svg", ink(F::idle())),
    }
}

impl Workbench {
    /// The column's top: the traffic lights' strip with the fold button at its right end
    /// (folded, the window keeps it beside the lights, on the same line).
    pub(super) fn flat_lights(&self, cx: &Context<Self>) -> gpui::Div {
        div()
            .flex_shrink_0()
            .h(px(T::TOP))
            .pr(px(T::S12))
            .flex()
            .items_center()
            .child(self.title_drag_area(cx))
            .child(self.sidebar_toggle(cx))
    }
    /// MOORING and the machine the column shows.
    pub(super) fn flat_brand(&self) -> gpui::Div {
        let letters = "MOORING".chars().map(|ch| div().child(ch.to_string()));
        let (machine, up) = self.column_machine();
        div()
            .flex_shrink_0()
            .h(px(F::BRAND_ROW))
            .pl(px(F::EDGE))
            .pr(px(T::S12))
            .flex()
            .items_center()
            .gap(px(T::S8))
            .font_family(T::UI_FONT)
            .text_size(px(F::BRAND_TEXT))
            .line_height(px(F::LINE_SMALL))
            .child(
                div()
                    .flex()
                    .gap(px(F::BRAND_TRACK))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(ink(F::bright()))
                    .children(letters),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_color(ink(F::third()))
                    .child(div().size(px(F::HOST_DOT)).rounded_full().bg(if up {
                        Tone::Ok.dot()
                    } else {
                        ink(F::idle())
                    }))
                    .child(machine),
            )
    }
    /// A quiet 26 px icon button.
    pub(super) fn flat_icon(
        &self,
        id: &'static str,
        path: &'static str,
        name: &'static str,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .role(gpui::accesskit::Role::Button)
            .aria_label(name)
            .size(px(F::ICON_BUTTON))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(F::ROW_RADIUS))
            .cursor_pointer()
            .hover(|s| s.bg(ink(F::selected())))
            .child(
                gpui::svg()
                    .path(path)
                    .size(px(F::ICON))
                    .text_color(ink(F::second())),
            )
            .tooltip(move |_, cx| cx.new(|_| super::render::TextTip(name.into())).into())
    }
    /// A field-like button that opens ⌘P.
    pub(super) fn flat_search(&self, cx: &Context<Self>) -> gpui::Div {
        div().flex_shrink_0().pt(px(T::S12)).px(px(T::S12)).child(
            div()
                .id("global-search")
                .role(gpui::accesskit::Role::Button)
                .aria_label("搜索会话、空间或原话")
                .h(px(F::SEARCH))
                .px(px(T::S10))
                .flex()
                .items_center()
                .gap(px(T::S8))
                .rounded(px(F::FIELD_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(ink(F::field_line()))
                .cursor_pointer()
                .hover(|s| s.bg(ink(F::hover())))
                .child(
                    gpui::svg()
                        .path("icons/search.svg")
                        .size(px(F::SEARCH_ICON))
                        .text_color(ink(F::third())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(T::UI_FONT)
                        .text_size(px(T::SMALL))
                        .line_height(px(F::LINE_TEXT))
                        .text_color(ink(F::third()))
                        .child("找会话、空间或原话"),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(T::mono(cx).family)
                        .text_size(px(F::TIME))
                        .line_height(px(F::LINE_SMALL))
                        .text_color(ink(F::third()))
                        .child(crate::keymap::keycap(cx, crate::keymap::Action::Search)),
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx))),
        )
    }
    /// A section's name and count: `要你处理 3`.
    pub(super) fn flat_group(title: &str, count: usize) -> gpui::Div {
        div()
            .pt(px(T::S14))
            .pb(px(F::CHIP_GAP))
            .px(px(F::EDGE))
            .flex()
            .items_center()
            .gap(px(T::S6))
            .font_family(T::UI_FONT)
            .text_size(px(F::HEAD_TEXT))
            .line_height(px(F::HEAD_LINE))
            .child(
                div()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(ink(F::second()))
                    .child(title.to_owned()),
            )
            .child(div().text_color(ink(F::third())).child(count.to_string()))
    }
    /// Folded sessions, one quiet line that opens them: `很久没动 3 个 ›`.
    pub(super) fn flat_more(
        &self,
        key: &str,
        text: String,
        open: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let key = key.to_owned();
        div()
            .id(gpui::SharedString::from(format!("sidebar-fold-{key}")))
            .mt(px(T::S2))
            .mx(px(T::S8))
            .px(px(T::S10))
            .py(px(F::ROW_PAD_Y))
            .flex()
            .items_center()
            .gap(px(T::S8))
            .rounded(px(F::ROW_RADIUS))
            .cursor_pointer()
            .hover(|s| s.bg(ink(F::hover())))
            .font_family(T::UI_FONT)
            .text_size(px(T::SMALL))
            .line_height(px(F::LINE_TEXT))
            .text_color(ink(F::third()))
            .child(flat_state(Status::Stale, false))
            .child(div().flex_1().min_w_0().truncate().child(text))
            .child(self.sidebar.fold_mark(&key, open))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_sidebar_group(&key, open, cx)))
            .into_any_element()
    }
    /// Closed sessions past the ones listed under 更早: one line that opens them in 鸟瞰.
    pub(super) fn flat_closed_more(&self, count: usize, cx: &Context<Self>) -> gpui::AnyElement {
        div()
            .id("sidebar-closed-sessions")
            .mx(px(T::S8))
            .px(px(T::S10))
            .py(px(F::ONE_PAD_Y))
            .rounded(px(F::ROW_RADIUS))
            .cursor_pointer()
            .hover(|s| s.bg(ink(F::hover())))
            .font_family(T::UI_FONT)
            .text_size(px(F::SUB))
            .line_height(px(F::SUB_LINE))
            .text_color(ink(F::third()))
            .child(format!("还有 {count} 个已关闭 · 在鸟瞰里找"))
            .on_click(cx.listener(|this, _, _, cx| this.show_closed(cx)))
            .into_any_element()
    }
    /// One session: its state, its title, its space and its time; what it asks you, when
    /// it asks, on a second line.
    pub(super) fn flat_row(
        &self,
        (session, g, attention): Listed,
        current: Option<&str>,
        detail: Option<String>,
        end: End,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let open = session.clone();
        let selected = current == Some(session.id.as_str());
        let done = g == Status::Closed;
        let unfolded = done && self.layout.done_open.as_deref() == Some(session.id.as_str());
        // Subagents take the second line, except on a row that asks you something.
        let subs = self
            .workspace
            .content
            .get(&session.id)
            .map(|c| c.subagents.as_slice())
            .filter(|s| !s.is_empty() && bucket(g) != Bucket::Attention);
        let detail = if subs.is_some() { None } else { detail };
        let subs_live = matches!(g, Status::Running | Status::Background);
        let subs_open = self.subagents_open(&session.id, selected);
        let two = detail.is_some() || unfolded || subs.is_some();
        let title_ink = match bucket(g) {
            Bucket::Attention | Bucket::Running | Bucket::Done => F::title(),
            Bucket::Stopped | Bucket::Fresh => F::quiet(),
            Bucket::Stale => F::second(),
        };
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
                .text_size(px(F::TITLE))
                .line_height(px(F::TITLE_LINE))
                .text_color(ink(title_ink))
                .child(self.session_title(session)),
        };
        // Narrowed to one space, every row's space is the same and goes unsaid.
        let space = self
            .space_filter()
            .is_none()
            .then(|| session_space(&self.workspace, session))
            .flatten()
            .and_then(|id| self.workspace.spaces.iter().find(|s| s.id == id))
            .map(|s| self.space_tag(&s.id, &s.name));
        let time = match end {
            End::Text(text) => text,
            End::Nothing => String::new(),
            // Only a running turn ticks; a stopped, waking or closed row shows its age.
            End::State => match self.turn_started(session).filter(|_| g == Status::Running) {
                Some(at) => {
                    self.tick_turn(cx);
                    super::layout::turn_clock(now().saturating_sub(at))
                }
                None => short_ago(self.session_updated(session), now()),
            },
        };
        let asks_ink = g.colors().1;
        div()
            .id(gpui::SharedString::from(format!("session-{}", session.id)))
            .relative()
            .mx(px(T::S8))
            .px(px(T::S10))
            .py(px(if two { F::ROW_PAD_Y } else { F::ONE_PAD_Y }))
            .flex()
            .when(!two, |d| d.items_center())
            .gap(px(F::ROW_GAP))
            .rounded(px(F::ROW_RADIUS))
            .cursor_pointer()
            .font_family(T::UI_FONT)
            .when(selected, |d| d.bg(ink(F::selected())))
            .hover(|s| s.bg(ink(F::selected())))
            .child(
                div().flex_shrink_0().when(two, |d| d.mt(px(T::S4))).child(
                    self.turning_mark(g, "icons/state-run.svg", F::STATE, cx)
                        .unwrap_or_else(|| flat_state(g, attention)),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(T::S8))
                            .child(title)
                            .children(space)
                            .child(
                                div()
                                    .min_w(px(F::TIME_WIDTH))
                                    .flex_shrink_0()
                                    .flex()
                                    .justify_end()
                                    .font_family(T::mono(cx).family)
                                    .text_size(px(F::TIME))
                                    .line_height(px(F::LINE_SMALL))
                                    .text_color(ink(F::third()))
                                    .child(time),
                            ),
                    )
                    .when_some(detail, |d, text| {
                        d.child(
                            div()
                                .mt(px(T::S2))
                                .min_w_0()
                                .truncate()
                                .text_size(px(F::SUB))
                                .line_height(px(F::SUB_LINE))
                                .text_color(asks_ink)
                                .child(text.replacen(ASKS, "问：", 1)),
                        )
                    })
                    .when_some(subs, |d, subs| {
                        d.child(self.subagent_line(&session.id, subs, subs_live, subs_open, cx))
                            .when(subs_open, |d| {
                                d.child(self.subagent_tree(session, done, subs, subs_live, cx))
                            })
                    })
                    .when(unfolded, |d| d.child(self.done_card(session, cx))),
            )
            // The selected row's edge, drawn over it so it never moves the row's text.
            .when(selected, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(F::ROW_RADIUS))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(ink(F::selected_line())),
                )
            })
            .when(self.peek_home(&session.id), |d| {
                d.child(super::peek::peek_home_outline())
            })
            .when(self.marking(&session.id), |d| {
                d.child(super::complete::spot(
                    &self.layout.spots,
                    session.id.clone(),
                    (T::S8, T::S4),
                    |s| &mut s.rows,
                ))
            })
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
                    this.click_row(&open, done, window, cx);
                }),
            )
            .into_any_element()
    }
    /// A row's click, or a click on a branch under it.
    pub(super) fn click_row(
        &mut self,
        session: &AgentSession,
        done: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Done with: a click unfolds what it ended with instead of resuming it.
        if done {
            let id = Some(session.id.clone());
            self.layout.done_open = if self.layout.done_open == id {
                None
            } else {
                id
            };
            cx.notify();
            return;
        }
        // Any other row folds the unfolded ✓ away.
        self.layout.done_open = None;
        self.peek_or_open(session, window, cx)
    }
    /// Under an unfolded ✓ row: the first line it ended with, and the two ways back in.
    fn done_card(&self, session: &AgentSession, cx: &Context<Self>) -> gpui::Div {
        let said = self
            .workspace
            .content
            .get(&session.id)
            .and_then(|c| {
                c.last_reply
                    .lines()
                    .map(mooring_core::content::plain_line)
                    .find(|l| !l.is_empty())
            })
            .unwrap_or_else(|| "没有留下回复".into());
        let (resume, undo) = (session.clone(), session.clone());
        let action = |id: String, text: &'static str| {
            div()
                .id(gpui::SharedString::from(id))
                .px(px(T::S8))
                .h(px(F::SUB_LINE + T::S6))
                .flex()
                .items_center()
                .rounded(px(T::CONTROL_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(ink(F::selected_line()))
                .text_size(px(F::SUB))
                .text_color(ink(F::second()))
                .cursor_pointer()
                .hover(|s| s.bg(ink(F::selected())).text_color(ink(F::title())))
                .child(text)
        };
        div()
            .mt(px(T::S4))
            .flex()
            .flex_col()
            .gap(px(T::S6))
            .child(
                div()
                    .text_size(px(F::SUB))
                    .line_height(px(F::SUB_LINE))
                    .text_color(ink(F::third()))
                    .line_clamp(2)
                    .child(said),
            )
            .child(
                div()
                    .flex()
                    .gap(px(T::S6))
                    .child(
                        action(format!("done-resume-{}", session.id), "继续对话").on_click(
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.layout.done_open = None;
                                this.peek_or_open(&resume, window, cx)
                            }),
                        ),
                    )
                    .child(
                        action(format!("done-undo-{}", session.id), "撤销完成").on_click(
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.layout.done_open = None;
                                this.settle_session(&undo, Outcome::Open, false, cx)
                            }),
                        ),
                    ),
            )
    }
    /// The key to the next session that needs you.
    pub(super) fn flat_keys(&self, cx: &Context<Self>) -> gpui::Div {
        div()
            .flex_shrink_0()
            .px(px(T::S16))
            .py(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S4))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(ink(F::line()))
            .font_family(T::UI_FONT)
            .text_size(px(F::TIME))
            .line_height(px(F::LINE_SMALL))
            .text_color(ink(F::third()))
            .child(
                div()
                    .px(px(T::S4))
                    .rounded(px(T::S4))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(ink(F::key_line()))
                    .font_family(T::mono(cx).family)
                    .text_size(px(F::KEY_TEXT))
                    .text_color(ink(F::second()))
                    .child(crate::keymap::keycap(
                        cx,
                        crate::keymap::Action::NextWaiting,
                    )),
            )
            .child("下一个要你的")
    }
    /// The foot: this machine and what it lists; the space's todos, the view, pairing a
    /// phone and settings.
    pub(super) fn flat_foot(&self, sessions: usize, cx: &Context<Self>) -> gpui::Div {
        let todos = self
            .workspace
            .tasks
            .iter()
            .filter(|t| t.space == self.nav.space)
            .count();
        let todos_open = self.tasks.todos_open;
        let view_open = self.sidebar.view_menu.is_some();
        let icon = |id, path, name| self.flat_icon(id, path, name);
        div()
            .flex_shrink_0()
            .pl(px(T::S16))
            .pr(px(T::S10))
            .pt(px(T::S8))
            .pb(px(T::S10))
            .flex()
            .items_center()
            .gap(px(T::S4))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(ink(F::line()))
            .font_family(T::UI_FONT)
            .text_size(px(F::FOOT_TEXT))
            .line_height(px(F::LINE_TEXT))
            .text_color(ink(F::second()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(self.column_foot(sessions)),
            )
            .child(
                div()
                    .id("todos-toggle")
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(if todos_open {
                        "收起待办"
                    } else {
                        "展开待办"
                    })
                    .h(px(F::ICON_BUTTON))
                    .px(px(T::S6))
                    .flex()
                    .items_center()
                    .rounded(px(F::ROW_RADIUS))
                    .cursor_pointer()
                    .when(todos_open, |d| {
                        d.bg(ink(F::selected())).text_color(ink(F::bright()))
                    })
                    .hover(|s| s.bg(ink(F::selected())))
                    .child(format!("待办 {todos}"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.tasks.todos_open = !this.tasks.todos_open;
                        cx.notify();
                    })),
            )
            .child(
                icon("sidebar-foot-view", "icons/sliders.svg", "选择视图")
                    .when(view_open, |d| d.bg(ink(F::selected())))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.sidebar.view_menu = match this.sidebar.view_menu {
                                Some(_) => None,
                                None => Some(event.position),
                            };
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .child(
                icon("sidebar-phones", "icons/phone.svg", "连接手机或其他电脑")
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_phones(cx))),
            )
            .child(
                icon("sidebar-settings", "icons/gear.svg", "设置")
                    .on_click(cx.listener(|this, _, window, cx| this.show_settings(window, cx))),
            )
    }
}
