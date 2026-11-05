//! 鸟瞰: every open session as grouped cards.
use super::*;

mod card;
mod groups;

/// The overview's filters and folds, the 「你在这里」 card and 逐个清理's queue.
pub(super) struct OverviewState {
    pub(super) since: u64,
    pub(super) filter_space: Option<String>,
    pub(super) changed_only: bool,
    /// Only this space's sessions whose terminal was closed, from the sidebar's 「已关闭」.
    pub(super) closed_only: bool,
    pub(super) here: Option<String>,
    /// The session whose 「你在这里」 card shows its secondary actions.
    pub(super) here_more: Option<String>,
    pub(super) show_stale: bool,
    pub(super) show_closed: bool,
    pub(super) cleanup: Vec<String>,
    pub(super) cleanup_index: usize,
    /// What each queued session became while this round of 逐个清理 ran, by session id.
    pub(super) cleanup_results: HashMap<String, Outcome>,
    pub(super) show_running: bool,
    pub(super) show_review: bool,
    /// The overview's scroll, so a group's pill can bring that group into view.
    pub(super) scroll: gpui::ScrollHandle,
    pub(super) columns: std::cell::RefCell<HashMap<super::board::Column, super::board::ColumnList>>,
    pub(super) show_hidden: bool,
    /// The board shows only this CLI's sessions and todos.
    pub(super) agent: Option<mooring_core::model::Agent>,
    /// The session whose detail drawer is open on the board.
    pub(super) drawer: Option<String>,
    /// The column a dragged card hangs over, and the column it came from.
    pub(super) drop_over: Option<(super::board::Column, super::board::Column)>,
    /// The card picked up off the board, or flying after its drop.
    pub(super) lift: Option<super::board_drawer::Lift>,
    pub(super) lift_seq: u64,
    /// A column shaking after it refused a card.
    pub(super) shake: Option<(super::board::Column, u64)>,
    /// The last width a column was drawn at, for a lifted card's first frame.
    pub(super) column_width: Option<gpui::Pixels>,
    pub(super) spots: super::board_drawer::BoardSpots,
    pub(super) toast: Option<super::board_drawer::Toast>,
    pub(super) toast_seq: u64,
    pub(super) card_menu: Option<String>,
    /// Opened by coming back after a while, not by the user: titled 「欢迎回来」.
    pub(super) welcome: bool,
    /// Holds keyboard focus while the overview shows, so Esc returns to the panes.
    pub(super) focus: gpui::FocusHandle,
    /// Where the top bar's toggle was drawn, for native click evidence.
    #[cfg(feature = "visual-test")]
    pub(super) toggle_at: Option<gpui::Bounds<gpui::Pixels>>,
}
impl Workbench {
    /// Swaps the panes for the overview in the same window: top bar, sidebar and tabs stay.
    pub(super) fn show_overview(&mut self, cx: &mut Context<Self>) {
        self.overview.closed_only = false;
        self.overview.welcome = false;
        self.overview.cleanup.clear();
        self.nav.overview = true;
        self.nav.task_detail = None;
        self.nav.pending_focus = true;
        self.sync_panes(cx);
        self.request(Request::Workspace, cx);
    }
    /// 逐个清理 over these sessions, in order.
    pub(super) fn start_cleanup(&mut self, queue: Vec<String>, cx: &mut Context<Self>) {
        self.overview.cleanup = queue;
        self.overview.cleanup_index = 0;
        self.overview.cleanup_results.clear();
        self.overview.here = None;
        cx.notify();
    }
    /// Notes where the toggle is drawn so native evidence can press the real button.
    #[cfg(feature = "visual-test")]
    pub(super) fn record_toggle(
        &self,
        button: gpui::Stateful<gpui::Div>,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let entity = cx.entity();
        button.relative().child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, _, _, cx| {
                    entity.update(cx, |this, _| this.overview.toggle_at = Some(bounds));
                },
            )
            .absolute()
            .size_full(),
        )
    }
    #[cfg(not(feature = "visual-test"))]
    pub(super) fn record_toggle(
        &self,
        button: gpui::Stateful<gpui::Div>,
        _: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        button
    }
    /// Back to the tab that was in view, its focused pane taking the keyboard again.
    pub(super) fn leave_overview(&mut self, cx: &mut Context<Self>) {
        self.overview.closed_only = false;
        self.overview.welcome = false;
        self.overview.cleanup.clear();
        self.overview.card_menu = None;
        self.nav.overview = false;
        self.nav.pending_focus = true;
        self.sync_panes(cx);
        cx.notify();
    }
    pub(super) fn overview_view(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        // The board is 鸟瞰; the list stays for closed terminals and the ended or hidden.
        if !self.overview.closed_only && !self.overview.show_closed && !self.overview.show_hidden {
            return self.board_view(window, cx);
        }
        let open: Vec<_> = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| {
                s.outcome.needs_review(now())
                    && self.session_hidden(s).is_none()
                    && self.matches_filter(s)
            })
            .collect();
        let closed_only = self.overview.closed_only;
        // Search sits in the top bar and the way back is the same 「鸟瞰全部」 toggle, the
        // tab strip or Esc, so the header keeps only what this page is.
        let header = div()
            .flex()
            .flex_wrap()
            .items_end()
            .gap(px(T::S16))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(T::S4))
                    .child(
                        div()
                            .text_size(px(T::TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::primary()))
                            .child(if closed_only {
                                "已关闭的会话"
                            } else if self.overview.welcome {
                                "欢迎回来"
                            } else {
                                "鸟瞰全部"
                            }),
                    )
                    .child(label(if closed_only {
                        let quiet = open
                            .iter()
                            .filter(|s| !closed_recently(&self.workspace, s))
                            .count();
                        format!(
                            "{} 个 · 终端已关，对话还在，点卡片在新画面里接着聊{} · Esc 回到工作区",
                            open.len(),
                            if quiet > 0 {
                                format!(" · 其中 {quiet} 个很久没动，左栏不再计数")
                            } else {
                                String::new()
                            }
                        )
                    } else {
                        format!(
                            "{} 个会话还没收尾 · 按要你做什么分好了组 · Esc 回到工作区",
                            open.len()
                        )
                    })),
            )
            .child(div().flex_1())
            .child(self.navigation_filters(cx));
        let mut before: Vec<gpui::AnyElement> = vec![header.into_any_element()];
        // Closed ones pile up: walk them one at a time instead of card menus.
        if closed_only && !open.is_empty() {
            let queue: Vec<_> = open.iter().map(|s| s.id.clone()).collect();
            before.push(
                div()
                    .flex()
                    .child(solid(button("cleanup-closed", "逐个清理 →")).on_click(
                        cx.listener(move |this, _, _, cx| this.start_cleanup(queue.clone(), cx)),
                    ))
                    .into_any_element(),
            );
        }
        if let Some(error) = self.error.clone() {
            before.push(
                div()
                    .text_color(Tone::Danger.ink())
                    .child(error)
                    .into_any_element(),
            );
        }
        // The sidebar stays beside the overview.
        let available = f32::from(window.viewport_size().width) - T::SIDEBAR - T::OVERVIEW_X * 2.;
        let card_width = |min: f32| {
            let columns = ((available + T::S10) / (min + T::S10)).floor().max(1.);
            ((available - T::S10 * (columns - 1.)) / columns).floor()
        };
        let mut middle: Vec<gpui::AnyElement> = vec![];
        if !closed_only {
            middle.push(
                self.todo_section(card_width(T::TODO_CARD_MIN), cx)
                    .into_any_element(),
            );
        }
        let at_now = now();
        let grouped: Vec<(Status, Vec<&AgentSession>)> = SECTIONS
            .into_iter()
            .map(|g| {
                (
                    g,
                    open.iter()
                        .copied()
                        .filter(|s| section(status_of(&self.workspace, s)) == g)
                        .collect(),
                )
            })
            .collect();
        // Empty groups get no section: their pill above already reads 0.
        let shown: Vec<_> = grouped.iter().filter(|(_, s)| !s.is_empty()).collect();
        let first_group = before.len() + 1 + middle.len();
        let place: Vec<(Status, usize)> = shown
            .iter()
            .enumerate()
            .map(|(i, (g, _))| (*g, first_group + i))
            .collect();
        let width = card_width(T::CARD_MIN);
        let sections: Vec<gpui::AnyElement> = shown
            .iter()
            .map(|(group, sessions)| {
                self.group_section(*group, sessions, width, at_now, cx)
                    .into_any_element()
            })
            .collect();
        let dist = self.distribution(&grouped, &place, cx);
        let mut view = div()
            .id("overview-scroll")
            .track_scroll(&self.overview.scroll)
            .size_full()
            .overflow_y_scroll()
            .font_features(super::diagnostics::tabular())
            .flex()
            .flex_col()
            .gap(px(T::S20))
            .px(px(T::OVERVIEW_X))
            .py(px(T::S20))
            .children(before)
            .child(dist)
            .children(middle)
            .children(sections);
        if closed_only {
            return view.when(open.is_empty(), |d| {
                d.child(label(
                    "这里没有关掉的会话。关掉画面后，它的对话会留在这里，随时能接着聊。",
                ))
            });
        }
        let completed = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| s.outcome == Outcome::Completed && self.session_hidden(s).is_none())
            .count();
        let abandoned = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| s.outcome == Outcome::Abandoned && self.session_hidden(s).is_none())
            .count();
        let deferred = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| {
                matches!(s.outcome, Outcome::Deferred { .. }) && self.session_hidden(s).is_none()
            })
            .count();
        view = view.child(
            div()
                .border_t(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .pt(px(T::S16))
                .child(
                    button(
                        "closed-toggle",
                        format!(
                            "稍后再看 {deferred} · 已完成 {completed} · 已放弃 {abandoned} · {}",
                            if self.overview.show_closed {
                                "收起"
                            } else {
                                "展开"
                            }
                        ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.overview.show_closed = !this.overview.show_closed;
                        cx.notify();
                    })),
                ),
        );
        if self.overview.show_closed {
            for s in self
                .workspace
                .agent_sessions
                .iter()
                .filter(|s| !s.outcome.needs_review(now()) && self.session_hidden(s).is_none())
            {
                let sid = s.id.clone();
                view = view.child(
                    div()
                        .p(px(T::S8))
                        .child(
                            button(
                                gpui::SharedString::from(format!("closed-{sid}")),
                                format!(
                                    "{} · {}",
                                    self.session_title(s),
                                    outcome_label(&s.outcome)
                                ),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.overview.here = Some(sid.clone());
                                    cx.notify();
                                },
                            )),
                        )
                        .when(self.overview.here.as_ref() == Some(&s.id), |d| {
                            d.child(self.here_card(s, false, cx))
                        }),
                );
            }
        }
        let hidden: Vec<_> = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| self.session_hidden(s).is_some())
            .collect();
        view = view.child(
            button(
                "toggle-hidden",
                format!(
                    "已隐藏 {} 个自动化会话 · {}",
                    hidden.len(),
                    if self.overview.show_hidden {
                        "收起"
                    } else {
                        "展开查看"
                    }
                ),
            )
            .border_0()
            .justify_start()
            .text_color(T::color(T::secondary()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.overview.show_hidden = !this.overview.show_hidden;
                cx.notify();
            })),
        );
        if self.overview.show_hidden {
            for s in hidden {
                let sid = s.id.clone();
                view = view.child(
                    div()
                        .p(px(T::S8))
                        .border_b(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::border()))
                        .child(
                            button(
                                gpui::SharedString::from(format!("hidden-{}", s.id)),
                                self.session_title(s),
                            )
                            .border_0()
                            .justify_start()
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.overview.here = Some(sid.clone());
                                    cx.notify();
                                },
                            )),
                        )
                        .child(label(format!(
                            "{} · {} · {} · {}",
                            self.session_hidden(s).unwrap_or(""),
                            s.runtime.executable(),
                            s.cli_session_id,
                            s.cwd
                        )))
                        .when(self.overview.here.as_ref() == Some(&s.id), |d| {
                            d.child(self.here_card(s, false, cx))
                        }),
                );
            }
        }
        view
    }
}

/// The one solid control of a view.
pub(super) fn solid(b: gpui::Stateful<gpui::Div>) -> gpui::Stateful<gpui::Div> {
    b.bg(T::color(T::accent()))
        .text_color(T::color(T::on_accent()))
        .font_weight(gpui::FontWeight::SEMIBOLD)
}
/// An ordinary button: a quiet fill, no outline.
pub(super) fn quiet(b: gpui::Stateful<gpui::Div>) -> gpui::Stateful<gpui::Div> {
    b.bg(T::color(T::button()))
        .text_color(T::color(T::primary()))
}
/// A section's title line: status dot, name, count, a note and actions at the right.
fn section_head(
    group: Option<Status>,
    title: &str,
    count: usize,
    note: Option<&str>,
    right: Vec<gpui::AnyElement>,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(T::S8))
        .when_some(group, |d, g| d.child(dot(g)))
        .child(
            div()
                .text_size(px(T::CARD_TITLE))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(group.map_or(T::color(T::primary()), |g| g.colors().1))
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_color(T::color(T::muted()))
                .child(count.to_string()),
        )
        .when_some(note, |d, note| {
            d.child(label(note.to_owned()).text_color(T::color(T::muted())))
        })
        .child(div().flex_1())
        .child(div().flex().gap(px(T::S6)).children(right))
}
/// The row a folded group shows instead of its cards.
fn folded_row(content: Vec<gpui::AnyElement>, right: Vec<gpui::AnyElement>) -> gpui::Div {
    div()
        .h(px(T::FOLDED_ROW))
        .px(px(T::S14))
        .flex()
        .items_center()
        .gap(px(T::S10))
        .rounded(px(T::CARD_RADIUS))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::border()))
        .children(content)
        .child(div().flex_1())
        .child(div().flex().gap(px(T::S6)).children(right))
}
