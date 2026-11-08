//! 逐个清理: one stale session at a time.
use super::overview::{quiet, solid};
use super::*;

/// What a step of the queue looks like: done, put off, dropped, the one in hand, or not yet.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Completed,
    Deferred,
    Abandoned,
    Current,
    Pending,
}
impl Step {
    fn fill(self) -> gpui::Hsla {
        match self {
            Self::Completed => T::color(T::secondary()),
            Self::Deferred => Status::Deferred.colors().0,
            Self::Abandoned => T::color(T::muted()).opacity(0.5),
            Self::Current => T::color(T::primary()),
            Self::Pending => T::color(T::chip()),
        }
    }
}

impl Workbench {
    /// 逐个清理's keys, the same actions as its buttons: ⏎ 完成, C 继续, L 稍后再看,
    /// ⌫ 放弃. Only while a session is shown; anything else falls through.
    pub(super) fn cleanup_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(s) = self
            .overview
            .cleanup
            .get(self.overview.cleanup_index)
            .and_then(|id| self.workspace.agent_sessions.iter().find(|s| &s.id == id))
            .cloned()
        else {
            return false;
        };
        if self.review_blocked(&s).is_some() && matches!(key, "enter" | "backspace" | "l") {
            return true;
        }
        match key {
            "enter" => self.complete_queued(s.id.clone(), cx),
            "backspace" => self.edit(
                Edit::SetOutcome {
                    seen: None,
                    id: s.id.clone(),
                    outcome: Outcome::Abandoned,
                },
                cx,
            ),
            "c" => {
                self.overview.cleanup.clear();
                self.open_session(&s, window, cx);
            }
            "l" => {
                self.overview.here = Some(s.id.clone());
                self.show_form(FormKind::Defer, window, cx);
            }
            _ => return false,
        }
        cx.notify();
        true
    }
    fn cleanup_step(&self, index: usize, id: &str) -> Step {
        if index == self.overview.cleanup_index {
            return Step::Current;
        }
        match self.overview.cleanup_results.get(id) {
            Some(Outcome::Completed) => Step::Completed,
            Some(Outcome::Deferred { .. }) => Step::Deferred,
            Some(Outcome::Abandoned) => Step::Abandoned,
            _ => Step::Pending,
        }
    }
    pub(super) fn cleanup_view(&self, cx: &Context<Self>) -> gpui::Stateful<gpui::Div> {
        let queue = &self.overview.cleanup;
        let steps: Vec<Step> = queue
            .iter()
            .enumerate()
            .map(|(i, id)| self.cleanup_step(i, id))
            .collect();
        let count = |step: Step| steps.iter().filter(|s| **s == step).count();
        let (completed, deferred, abandoned) = (
            count(Step::Completed),
            count(Step::Deferred),
            count(Step::Abandoned),
        );
        let untouched = queue.len() - completed - deferred - abandoned;
        let position = (self.overview.cleanup_index + 1).min(queue.len());
        let mut bar = div()
            .flex()
            .h(px(T::CLEANUP_STEP))
            .when(queue.len() <= T::CLEANUP_STEP_GAPS, |d| d.gap(px(T::S2)));
        for step in &steps {
            bar = bar.child(
                div()
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .rounded(px(T::S2))
                    .bg(step.fill()),
            );
        }
        let swatch = |step: Step, text: String| {
            div()
                .flex()
                .items_center()
                .gap(px(T::S6))
                .child(
                    div()
                        .w(px(T::SWATCH))
                        .h(px(T::CLEANUP_STEP))
                        .rounded(px(T::S2))
                        .bg(step.fill())
                        .when(step == Step::Pending, |d| {
                            d.border(px(T::BORDER_WIDTH))
                                .border_color(T::color(T::control()))
                        }),
                )
                .child(text)
        };
        let mut page = div()
            .w_full()
            .max_w(px(T::CLEANUP_PAGE_WIDTH))
            .flex()
            .flex_col()
            .gap(px(T::S16))
            .font_features(super::diagnostics::tabular())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S12))
                    .child(
                        button("cleanup-exit", t!("← 返回鸟瞰"))
                            .px(px(T::S10))
                            .text_color(T::color(T::body()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.overview.cleanup.clear();
                                this.nav.overview = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(T::FORM_TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(if self.overview.cleanup_review {
                                t!("逐个验收")
                            } else if self.overview.closed_only {
                                t!("清理已关闭的会话")
                            } else {
                                t!("清理很久没动的会话")
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        label(t!(
                            "第 {position} / {len} 个 · 剩 {untouched}",
                            position,
                            len = queue.len(),
                            untouched
                        ))
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::secondary())),
                    ),
            )
            .child(bar)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(T::S14))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::secondary()))
                    .child(swatch(Step::Completed, t!("完成 {completed}", completed)))
                    .child(swatch(Step::Deferred, t!("稍后再看 {deferred}", deferred)))
                    .child(swatch(Step::Abandoned, t!("放弃 {abandoned}", abandoned)))
                    .child(swatch(Step::Pending, t!("还没处理 {untouched}", untouched))),
            );
        if let Some(s) = queue
            .get(self.overview.cleanup_index)
            .and_then(|id| self.workspace.agent_sessions.iter().find(|s| &s.id == id))
        {
            let now = now();
            let updated = self.session_updated(s);
            // Where this one's quiet time sits among the whole queue's, oldest at the right.
            let ages: Vec<u64> = queue
                .iter()
                .filter_map(|id| self.workspace.agent_sessions.iter().find(|s| &s.id == id))
                .map(|s| now.saturating_sub(self.session_updated(s)))
                .collect();
            let (lo, hi) = (
                ages.iter().copied().min().unwrap_or(0),
                ages.iter().copied().max().unwrap_or(0),
            );
            let share = if hi > lo {
                now.saturating_sub(updated).saturating_sub(lo) as f32 / (hi - lo) as f32
            } else {
                0.
            };
            let mut card = div()
                .px(px(T::S20))
                .py(px(T::S16))
                .rounded(px(T::POPOVER_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(T::card_border()))
                .bg(T::color(T::popover()))
                .flex()
                .flex_col()
                .gap(px(T::S12))
                .child(
                    div()
                        .text_size(px(T::TITLE))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(T::color(T::primary()))
                        .child(self.session_title(s)),
                )
                .child(
                    label(format!(
                        "{} · {} · {}",
                        s.runtime.executable(),
                        if s.terminal.is_none() {
                            t!("外部·只读")
                        } else {
                            t!("托管")
                        },
                        s.cwd
                    ))
                    .truncate()
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::secondary())),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(T::S10))
                        .text_size(px(T::META))
                        .text_color(T::color(T::muted()))
                        .child(t!("最后活动"))
                        .child(
                            div()
                                .flex_1()
                                .h(px(T::S4))
                                .rounded(px(T::S2))
                                .bg(T::color(T::chip()))
                                .child(
                                    div().relative().h_full().w(gpui::relative(share)).child(
                                        div()
                                            .absolute()
                                            .right(px(-T::S2))
                                            .top(px(-(T::AGE_MARKER - T::S4) / 2.))
                                            .w(px(T::S2))
                                            .h(px(T::AGE_MARKER))
                                            .rounded(px(T::BORDER_WIDTH))
                                            .bg(T::color(T::primary())),
                                    ),
                                ),
                        )
                        .child(ago(updated, now)),
                );
            if let Some(h) = self.workspace.content.get(&s.id).map(|c| c.here()) {
                // The title already is the first ask when nothing better names the session.
                let title = self.session_title(s);
                // As in the 你在这里 card: no empty evidence line, no template next step.
                for (name, text) in [
                    (t!("你要它"), h.ask),
                    (t!("做成了"), h.done),
                    (t!("停在"), h.stopped),
                ]
                .into_iter()
                .filter(|(name, text)| {
                    (*name != t!("你要它") || text.trim() != title.trim())
                        && (*name != t!("做成了") || text != mooring_core::content::NO_DONE)
                }) {
                    card = card.child(
                        div()
                            .flex()
                            .gap(px(T::S14))
                            .child(
                                label(name)
                                    .w(px(T::CLEANUP_LABEL))
                                    .flex_shrink_0()
                                    .text_size(px(T::BODY_SIZE))
                                    .text_color(T::color(T::muted())),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_color(T::color(T::body()))
                                    .child(text),
                            ),
                    );
                }
            }
            page = page.child(card);
            let blocked = self.review_blocked(s);
            if let Some(status) = blocked {
                let left = mooring_core::status::left_behind(&self.workspace, s);
                page = page.child(label(match left {
                    Some(left) => t!("{left}，这里标记会关掉它：打开看看或跳过", left),
                    None => t!(
                        "它现在{label}，这里不能标记：打开看看或跳过",
                        label = status.label()
                    ),
                }));
            }
            let sid = s.id.clone();
            let mut actions = div().flex().flex_wrap().gap(px(T::S8));
            if blocked.is_none() {
                actions = actions.child(solid(button(not_ui!("完成"), t!("完成 ⏎"))).on_click(
                    cx.listener(move |this, _, _, cx| this.complete_queued(sid.clone(), cx)),
                ));
            }
            let resume = s.clone();
            actions = actions.child(quiet(button("cleanup-continue", t!("继续 C"))).on_click(
                cx.listener(move |this, _, window, cx| {
                    this.overview.cleanup.clear();
                    this.open_session(&resume, window, cx);
                }),
            ));
            if blocked.is_none() {
                let sid = s.id.clone();
                actions =
                    actions.child(quiet(button("cleanup-defer", t!("稍后再看… L"))).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.overview.here = Some(sid.clone());
                            this.show_form(FormKind::Defer, window, cx);
                        }),
                    ));
                let sid = s.id.clone();
                actions = actions.child(quiet(button("cleanup-abandon", t!("放弃 ⌫"))).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.edit(
                            Edit::SetOutcome {
                                seen: None,
                                id: sid.clone(),
                                outcome: Outcome::Abandoned,
                            },
                            cx,
                        )
                    }),
                ));
            }
            page = page.child(actions);
        } else {
            page = page.child(
                div()
                    .text_color(T::color(T::secondary()))
                    .child(t!("这一轮已处理完，可以返回鸟瞰。")),
            );
        }
        page = page.child(
            div()
                .flex()
                .gap(px(T::S8))
                .child(
                    button("cleanup-previous", t!("← 上一个"))
                        .text_color(T::color(T::secondary()))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.overview.cleanup_index =
                                this.overview.cleanup_index.saturating_sub(1);
                            cx.notify();
                        })),
                )
                .child(
                    button("cleanup-skip", t!("跳过 →"))
                        .text_color(T::color(T::secondary()))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.overview.cleanup_index =
                                (this.overview.cleanup_index + 1).min(this.overview.cleanup.len());
                            cx.notify();
                        })),
                ),
        );
        div()
            .id("cleanup-page")
            .size_full()
            .bg(T::color(T::pane()))
            .text_color(T::color(T::primary()))
            .overflow_y_scroll()
            .px(px(T::S20))
            .py(px(T::S20))
            .flex()
            .flex_col()
            .items_center()
            .child(page)
    }
}
