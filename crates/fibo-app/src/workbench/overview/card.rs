//! 鸟瞰's cards: one per session, and the 很久没动 age chart.
use super::*;

/// 很久没动's mini chart: last-activity ages, each bucket ending below its bound.
const DAY_SECS: u64 = 86_400;
const AGE_BUCKETS: [(u64, &str); 7] = [
    (DAY_SECS, tk!("1 天内")),
    (3 * DAY_SECS, tk!("1–3 天")),
    (7 * DAY_SECS, tk!("3–7 天")),
    (14 * DAY_SECS, tk!("1–2 周")),
    (30 * DAY_SECS, tk!("2–4 周")),
    (90 * DAY_SECS, tk!("1–3 月")),
    (u64::MAX, tk!("3 月以上")),
];

impl Workbench {
    /// How long each stale session has been quiet, in widening buckets (a day, days, weeks,
    /// months) so a fresh backlog and an old tail both show, oldest at the right.
    pub(super) fn age_chart(&self, sessions: &[&AgentSession], now: u64) -> Vec<gpui::AnyElement> {
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
            .aria_label(t!("按最后活动时间分布：从 1 天内到 3 个月以上"));
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
                    .tooltip(super::super::diagnostics::tip(vec![
                        fibo_core::i18n::say(when).to_string(),
                        t!("{n} 个", n),
                    ])),
            );
        }
        let summary = counts
            .iter()
            .zip(AGE_BUCKETS)
            .filter(|(n, _)| **n > 0)
            .map(|(n, (_, when))| format!("{} {n}", fibo_core::i18n::say(when)))
            .collect::<Vec<_>>()
            .join(" · ");
        vec![
            chart.into_any_element(),
            label(t!("按最后活动：{summary}", summary))
                .text_size(px(T::SMALL))
                .text_color(T::color(T::muted()))
                .into_any_element(),
        ]
    }
    pub(super) fn session_card(
        &self,
        group: Status,
        session: &AgentSession,
        width: f32,
        now: u64,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let closed_only = self.overview.closed_only;
        let session = session.clone();
        let enter = session.clone();
        let primary = session.clone();
        let menu_id = session.id.clone();
        let meta = format!(
            "{} · {} · {}{}{}",
            if session.terminal.is_none() {
                t!("外部·只读")
            } else {
                t!("托管")
            },
            session.runtime.executable(),
            (self.workspace.session_space(&session))
                .and_then(|id| self.workspace.spaces.iter().find(|s| s.id == id))
                .map_or(t!("外部"), |s| s.name.as_str()),
            if self
                .workspace
                .outcome_history
                .iter()
                .rev()
                .find(|h| h.session == session.id)
                .is_some_and(|h| h.reason == not_ui!("暂存到期"))
            {
                t!(" · 到期")
            } else {
                ""
            },
            if changed_since(&self.workspace, &session, self.overview.since) {
                t!(" · 新")
            } else {
                ""
            }
        );
        // The last thing said, in body text; a muted lead says what it is.
        let line = if self.workspace.content.contains_key(&session.id) {
            let lead = format!("{}  ", t!("最后一句"));
            gpui::StyledText::new(format!("{lead}{}", self.session_line(&session))).with_highlights(
                [(
                    0..lead.len(),
                    gpui::HighlightStyle {
                        color: Some(T::color(T::muted())),
                        ..Default::default()
                    },
                )],
            )
        } else {
            gpui::StyledText::new(self.session_line(&session))
        };
        let primary_continue =
            closed_only || matches!(group, Status::Waiting | Status::Error | Status::Paused);
        div()
            .id(gpui::SharedString::from(format!("card-{}", session.id)))
            .role(gpui::accesskit::Role::Button)
            .aria_label(gpui::SharedString::from(t!(
                "会话：{title}",
                title = self.session_title(&session)
            )))
            .relative()
            .w(px(width))
            .min_w(px(T::CARD_MIN))
            .min_h(px(T::CARD_MIN_HEIGHT))
            .px(px(T::S14))
            .pt(px(T::S12))
            .pb(px(T::S10))
            .rounded(px(T::CARD_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .bg(T::color(T::popover()))
            .flex()
            .flex_col()
            .gap(px(T::S6))
            .cursor_pointer()
            .hover(|s| s.border_color(T::color(T::control())))
            .on_click(cx.listener(move |this, _, window, cx| this.enter_card(&enter, window, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(T::CARD_TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::primary()))
                            .truncate()
                            .child(self.session_title(&session)),
                    )
                    .child(
                        button(
                            gpui::SharedString::from(format!("more-{}", session.id)),
                            "⋯",
                        )
                        .h(px(T::ICON_HEIGHT))
                        .px(px(T::S6))
                        .text_color(T::color(T::muted()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.overview.here = None;
                            this.overview.card_menu =
                                if this.overview.card_menu.as_ref() == Some(&menu_id) {
                                    None
                                } else {
                                    Some(menu_id.clone())
                                };
                            cx.notify();
                        })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .child(div().flex_1().min_w_0().truncate().child(meta))
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(ago(self.session_updated(&session), now)),
                    ),
            )
            .child(
                div()
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::body()))
                    .line_clamp(3)
                    .text_ellipsis()
                    .child(line),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .child(chip(group).font_weight(gpui::FontWeight::SEMIBOLD))
                    .child(div().flex_1())
                    .child(
                        button(
                            gpui::SharedString::from(format!("primary-{}", session.id)),
                            if closed_only {
                                t!("接着聊 →")
                            } else if primary_continue {
                                t!("继续 →")
                            } else {
                                t!("看进度")
                            },
                        )
                        .h(px(T::SEGMENT_HEIGHT))
                        .px(px(T::S8))
                        .text_color(T::color(T::link()))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.overview.card_menu = None;
                                if primary_continue {
                                    this.open_session(&primary, window, cx);
                                } else {
                                    this.overview.here = Some(primary.id.clone());
                                    cx.notify();
                                }
                            },
                        )),
                    ),
            )
            .when(self.overview.card_menu.as_ref() == Some(&session.id), |d| {
                d.child(
                    gpui::deferred(
                        self.session_actions(&session, cx)
                            .absolute()
                            .top(px(T::ROW + T::S12))
                            .right(px(T::S12))
                            .occlude(),
                    )
                    .priority(1),
                )
            })
            .when(self.overview.here.as_ref() == Some(&session.id), |d| {
                d.child(self.here_card(&session, false, cx))
            })
    }
}
