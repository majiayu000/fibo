//! 鸟瞰's 待办 · 还没开始 row of unstarted todos.
use super::*;

impl Workbench {
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
                                        label(format!(
                                            "{} · {} 条验收标准",
                                            task.agent.executable(),
                                            task.acceptance.len()
                                        ))
                                        .text_color(T::color(T::muted())),
                                    ),
                            )
                            .child(
                                quiet(button(
                                    gpui::SharedString::from(format!("start-{id}")),
                                    "开始 →",
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
                "待办 · 还没开始",
                unstarted.len(),
                Some("命令只预填，按回车才启动"),
                vec![
                    quiet(button("overview-new-task", "＋ 新待办"))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.show_quick_task(window, cx)),
                        )
                        .into_any_element(),
                ],
            ))
            .when(!unstarted.is_empty(), |d| d.child(cards))
    }
}
