//! 鸟瞰's status groups: the distribution bar, each group's section and its age chart.
use super::*;

/// 很久没动's mini chart: last-activity ages, each bucket ending below its bound.
const DAY_SECS: u64 = 86_400;
const AGE_BUCKETS: [(u64, &str); 7] = [
    (DAY_SECS, "1 天内"),
    (3 * DAY_SECS, "1–3 天"),
    (7 * DAY_SECS, "3–7 天"),
    (14 * DAY_SECS, "1–2 周"),
    (30 * DAY_SECS, "2–4 周"),
    (90 * DAY_SECS, "1–3 月"),
    (u64::MAX, "3 月以上"),
];

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
    pub(in crate::workbench) fn distribution(
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
            .aria_label(gpui::SharedString::from(format!(
                "{total} 个会话按状态分布"
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
                .tooltip(super::diagnostics::tip(vec![
                    group.label().to_owned(),
                    format!("{n} 个 · {:.0}%", n as f32 / total.max(1) as f32 * 100.),
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
            Status::Error => Some("先看这些：Agent 停在了报错上"),
            Status::Review => Some("等你核对后标记完成"),
            Status::Stale if !self.overview.closed_only => Some("左栏不再计数"),
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
                "展开核对"
            } else {
                "展开"
            };
            let toggle = quiet(button(toggle_id, if folded { open_label } else { "收起" }))
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
                    let mut counts = [("最近 24 小时", 0), ("最近 7 天", 0), ("更早", 0)];
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
                    label("不用管，跑完会挪到「等你验收」")
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::muted()))
                        .into_any_element(),
                ],
                _ => {
                    right.push(
                        solid(button("cleanup", "逐个清理 →"))
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
    /// How long each stale session has been quiet, in widening buckets (a day, days, weeks,
    /// months) so a fresh backlog and an old tail both show, oldest at the right.
    fn age_chart(&self, sessions: &[&AgentSession], now: u64) -> Vec<gpui::AnyElement> {
        if sessions.is_empty() {
            return vec![];
        }
        let mut counts = [0usize; AGE_BUCKETS.len()];
        for s in sessions {
            let age = now.saturating_sub(self.session_updated(s));
            let i = AGE_BUCKETS
                .iter()
                .position(|(below, _)| age < *below)
                .unwrap_or(AGE_BUCKETS.len() - 1);
            counts[i] += 1;
        }
        let peak = counts.iter().copied().max().unwrap_or(1).max(1);
        let mut chart = div()
            .id("stale-ages")
            .flex()
            .items_end()
            .gap(px(T::S2))
            .h(px(T::AGE_CHART_HEIGHT))
            .aria_label("按最后活动时间分布：从 1 天内到 3 个月以上");
        let hue = Status::Stale.colors().0;
        for (i, (n, (_, when))) in counts.iter().zip(AGE_BUCKETS).enumerate() {
            chart = chart.child(
                div()
                    .id(gpui::SharedString::from(format!("stale-age-{i}")))
                    .w(px(T::AGE_BAR_WIDTH))
                    .h(px(
                        (T::AGE_CHART_HEIGHT * *n as f32 / peak as f32).max(T::S2)
                    ))
                    .rounded(px(T::BORDER_WIDTH))
                    .bg(hue)
                    .opacity(if *n == 0 { 0.25 } else { 0.7 })
                    .tooltip(super::diagnostics::tip(vec![
                        when.to_string(),
                        format!("{n} 个"),
                    ])),
            );
        }
        let summary = counts
            .iter()
            .zip(AGE_BUCKETS)
            .filter(|(n, _)| **n > 0)
            .map(|(n, (_, when))| format!("{when} {n}"))
            .collect::<Vec<_>>()
            .join(" · ");
        vec![
            chart.into_any_element(),
            label(format!("按最后活动：{summary}"))
                .text_size(px(T::SMALL))
                .text_color(T::color(T::muted()))
                .into_any_element(),
        ]
    }
}
