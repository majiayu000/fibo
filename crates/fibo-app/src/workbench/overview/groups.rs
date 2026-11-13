//! 鸟瞰's sections: the status distribution, the todo row and one section per group.
use super::*;

impl Workbench {
    /// A group was asked for from the distribution: unfold it and bring it to the top.
    fn reveal_group(&mut self, group: Status, at: usize, cx: &mut Context<Self>) {
        match group {
            Status::Running => self.overview.show_running = true,
            Status::Stale => self.overview.show_stale = true,
            Status::Review => self.overview.show_review = true,
            _ => (),
        }
        self.overview.scroll.scroll_to_top_of_item(at);
        cx.notify();
    }
    fn group_folded(&self, group: Status) -> bool {
        !self.overview.closed_only
            && match group {
                Status::Running => !self.overview.show_running,
                Status::Stale => !self.overview.show_stale,
                Status::Review => !self.overview.show_review,
                _ => false,
            }
    }
    /// One segment per group with sessions, weighted by count, then a pill per group.
    pub(super) fn distribution(
        &self,
        grouped: &[(Status, Vec<&AgentSession>)],
        place: &[(Status, usize)],
        cx: &Context<Self>,
    ) -> gpui::Div {
        let at = |g: Status| place.iter().find(|(p, _)| *p == g).map(|(_, i)| *i);
        let total: usize = grouped.iter().map(|(_, s)| s.len()).sum();
        let filled: Vec<_> = grouped.iter().filter(|(_, s)| !s.is_empty()).collect();
        let mut bar = div()
            .id("overview-distribution")
            .flex()
            .gap(px(T::S2))
            .h(px(T::DIST_BAR))
            .aria_label(gpui::SharedString::from(t!(
                "{total} 个会话按状态分布",
                total
            )));
        let last = filled.len().saturating_sub(1);
        for (i, (group, sessions)) in filled.iter().enumerate() {
            let (group, n) = (*group, sessions.len());
            let hue = group.colors().0;
            let mut segment = div()
                .id(gpui::SharedString::from(format!("dist-{i}")))
                .h_full()
                .min_w(px(T::S4))
                .flex_grow(n as f32)
                .flex_basis(gpui::relative(0.))
                .cursor_pointer()
                .tooltip(super::super::diagnostics::tip(vec![
                    group.label().to_owned(),
                    t!(
                        "{n} 个 · {pct}%",
                        n,
                        pct = format!("{:.0}", n as f32 / total.max(1) as f32 * 100.)
                    ),
                ]));
            segment = if group == Status::Stale {
                segment
                    .bg(gpui::pattern_slash(hue, T::S2, T::S6))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(hue)
            } else {
                segment.bg(hue)
            };
            if i == 0 {
                segment = segment.rounded_l(px(T::CHIP_RADIUS));
            }
            if i == last {
                segment = segment.rounded_r(px(T::CHIP_RADIUS));
            }
            if let Some(at) = at(group) {
                segment = segment
                    .on_click(cx.listener(move |this, _, _, cx| this.reveal_group(group, at, cx)));
            }
            bar = bar.child(segment);
        }
        let mut legend = div().flex().flex_wrap().gap(px(T::S6));
        for (i, (group, sessions)) in grouped.iter().enumerate() {
            let (group, n) = (*group, sessions.len());
            let empty = n == 0;
            let mut pill = div()
                .id(gpui::SharedString::from(format!("pill-{i}")))
                .role(gpui::accesskit::Role::Button)
                .aria_label(gpui::SharedString::from(format!("{} {n}", group.label())))
                .h(px(T::PILL_HEIGHT))
                .px(px(T::S10))
                .flex()
                .items_center()
                .gap(px(T::S6))
                .rounded(px(T::PILL_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .text_size(px(T::SMALL))
                .text_color(T::color(if empty { T::muted() } else { T::body() }))
                .child(dot(group))
                .child(group.label())
                .child(if empty {
                    div().child(n.to_string())
                } else {
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(T::color(T::primary()))
                        .child(n.to_string())
                });
            if empty {
                pill = pill.border_dashed();
            } else if let Some(at) = at(group) {
                pill = pill
                    .cursor_pointer()
                    .hover(|s| s.bg(T::color(T::raised())))
                    .on_click(cx.listener(move |this, _, _, cx| this.reveal_group(group, at, cx)));
            }
            legend = legend.child(pill);
        }
        div()
            .flex()
            .flex_col()
            .gap(px(T::S10))
            .when(total > 0, |d| d.child(bar))
            .child(legend)
    }
    /// Todos not handed to an Agent yet: a light row each, 「开始」 at its right.
    pub(super) fn todo_section(&self, width: f32, cx: &Context<Self>) -> gpui::Div {
        let unstarted: Vec<_> = self
            .workspace
            .tasks
            .iter()
            .filter(|t| {
                self.workspace
                    .task_outcomes
                    .get(&t.id)
                    .unwrap_or(&Outcome::Open)
                    .needs_review(now())
                    && self
                        .overview
                        .filter_space
                        .as_ref()
                        .is_none_or(|id| id == &t.space)
                    && !self.overview.changed_only
                    && !self
                        .workspace
                        .agent_sessions
                        .iter()
                        .any(|s| s.task.as_ref() == Some(&t.id))
            })
            .collect();
        let mut cards = div().flex().flex_wrap().gap(px(T::S10));
        for task in &unstarted {
            let id = task.id.clone();
            cards = cards.child(
                div()
                    .w(px(width))
                    .min_w(px(T::TODO_CARD_MIN))
                    .px(px(T::S14))
                    .py(px(T::S12))
                    .rounded(px(T::CARD_RADIUS))
                    .bg(T::color(T::popover()))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::card_border()))
                    .flex()
                    .flex_col()
                    .gap(px(T::S8))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(T::S12))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(T::S2))
                                    .child(
                                        div()
                                            .truncate()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(T::color(T::primary()))
                                            .child(task.title.clone()),
                                    )
                                    .child(
                                        label(t!(
                                            "{executable} · {len} 条验收标准",
                                            executable = task.agent.executable(),
                                            len = task.acceptance.len()
                                        ))
                                        .text_color(T::color(T::muted())),
                                    ),
                            )
                            .child(
                                quiet(button(
                                    gpui::SharedString::from(format!("start-{id}")),
                                    t!("开始 →"),
                                ))
                                .flex_shrink_0()
                                .on_click(cx.listener(
                                    move |this, _, _, cx| this.ask_agent(id.clone(), false, cx),
                                )),
                            ),
                    )
                    .children(self.agent_picker(&task.id, cx)),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(T::S10))
            .child(section_head(
                None,
                t!("待办 · 还没开始"),
                unstarted.len(),
                Some(t!("命令只预填，按回车才启动")),
                vec![
                    quiet(button("overview-new-task", t!("＋ 新待办")))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.show_quick_task(window, cx)),
                        )
                        .into_any_element(),
                ],
            ))
            .when(!unstarted.is_empty(), |d| d.child(cards))
    }
    /// A group's head, its folded summary when it is folded, and its cards otherwise.
    pub(super) fn group_section(
        &self,
        group: Status,
        sessions: &[&AgentSession],
        width: f32,
        now: u64,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let folded = self.group_folded(group);
        let foldable = !self.overview.closed_only
            && matches!(group, Status::Running | Status::Stale | Status::Review);
        let note = match group {
            Status::Error => Some(t!("先看这些：Agent 停在了报错上")),
            Status::Review => Some(t!("等你核对后标记完成")),
            Status::Stale if !self.overview.closed_only => Some(t!("左栏不再计数")),
            _ => None,
        };
        let mut section = div().flex().flex_col().gap(px(T::S10)).child(section_head(
            Some(group),
            group.label(),
            sessions.len(),
            note,
            vec![],
        ));
        if foldable {
            let toggle_id = match group {
                Status::Running => "toggle-running",
                Status::Stale => "toggle-stale",
                _ => "toggle-review",
            };
            let open_label = if group == Status::Stale {
                t!("展开核对")
            } else {
                t!("展开")
            };
            let toggle = quiet(button(
                toggle_id,
                if folded { open_label } else { t!("收起") },
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                let flag = match group {
                    Status::Running => &mut this.overview.show_running,
                    Status::Stale => &mut this.overview.show_stale,
                    _ => &mut this.overview.show_review,
                };
                *flag = !*flag;
                cx.notify();
            }));
            let mut right = vec![toggle.into_any_element()];
            let content: Vec<gpui::AnyElement> = match group {
                Status::Review => {
                    let mut counts = [
                        (t!("最近 24 小时"), 0),
                        (t!("最近 7 天"), 0),
                        (t!("更早"), 0),
                    ];
                    for s in sessions {
                        let bucket = recency(self.session_updated(s), now);
                        if let Some(c) = counts.iter_mut().find(|(name, _)| *name == bucket) {
                            c.1 += 1;
                        }
                    }
                    vec![
                        label(
                            counts
                                .iter()
                                .map(|(name, n)| format!("{name} {n}"))
                                .collect::<Vec<_>>()
                                .join(" · "),
                        )
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::muted()))
                        .into_any_element(),
                    ]
                }
                Status::Running => vec![
                    label(t!(
                        "不用管，跑完会挪到「{state}」",
                        state = Status::Review.label()
                    ))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::muted()))
                    .into_any_element(),
                ],
                _ => {
                    right.push(
                        solid(button("cleanup", t!("逐个清理 →")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let queue = this
                                    .workspace
                                    .agent_sessions
                                    .iter()
                                    .filter(|s| {
                                        this.session_hidden(s).is_none()
                                            && status_of(&this.workspace, s) == Status::Stale
                                    })
                                    .map(|s| s.id.clone())
                                    .collect();
                                this.start_cleanup(queue, cx);
                            }))
                            .into_any_element(),
                    );
                    self.age_chart(sessions, now)
                }
            };
            section = section.child(folded_row(content, right));
        }
        if folded {
            return section;
        }
        let mut cards = div().flex().flex_wrap().gap(px(T::S10));
        for session in sessions {
            cards = cards.child(self.session_card(group, session, width, now, cx));
        }
        section.child(cards)
    }
}
