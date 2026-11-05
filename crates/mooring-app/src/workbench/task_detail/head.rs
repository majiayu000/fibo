//! The todo detail's head: title, status and the main buttons.
use super::super::forms::{plain, solid};
use super::*;

impl Workbench {
    /// Title, status and where it runs; the main button follows the state: back to its
    /// pane while one runs, 标记完成 once every criterion is proven, 开始 with no session.
    pub(super) fn detail_head(
        &self,
        task: &Task,
        status: TaskStatus,
        has_sessions: bool,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let tid = task.id.clone();
        let outcome = self
            .workspace
            .task_outcomes
            .get(&tid)
            .cloned()
            .unwrap_or(Outcome::Open);
        let space = self.workspace.spaces.iter().find(|s| s.id == task.space);
        let sep = || div().opacity(0.5).child("·");
        let mut meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(T::S8))
            .text_size(px(T::SMALL))
            .text_color(T::color(T::secondary()))
            .child(
                div()
                    .h(px(T::STATE_CHIP))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .rounded(px(T::STATE_CHIP_RADIUS))
                    .bg(T::color(T::chip()))
                    .text_color(T::color(T::primary()))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(ring(status))
                    .child(status.label()),
            );
        if outcome == Outcome::Open {
            meta = meta.child(
                div().flex().gap(px(T::S4)).child("结局").child(
                    div()
                        .text_color(T::color(T::primary()))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child("未定"),
                ),
            );
        }
        if let Outcome::Deferred { reason, until } = &outcome {
            meta = meta
                .child(sep())
                .child(format!("{} 再看 · {reason}", upcoming(*until, now())));
        }
        meta = meta.child(sep()).child(task.agent.name());
        if let Some(space) = space {
            meta = meta.child(sep()).child(space.name.clone());
            if outcome == Outcome::Open {
                meta = meta.child(sep()).child(
                    div()
                        .font(T::mono(cx))
                        .text_size(px(T::META))
                        .min_w_0()
                        .truncate()
                        .child(space_dir_label(space)),
                );
            }
        }
        let settled = matches!(status, TaskStatus::Completed | TaskStatus::Abandoned);
        let running = self.workspace.task_terminal(&tid).is_some();
        let outcome_button = |name: &'static str, to: Outcome| {
            let tid = tid.clone();
            button(name, name).on_click(cx.listener(move |this, _, _, cx| {
                this.edit(
                    Edit::SetTaskOutcome {
                        seen: None,
                        id: tid.clone(),
                        outcome: to.clone(),
                    },
                    cx,
                )
            }))
        };
        let back = || {
            let tid = tid.clone();
            button("task-back", "回到画面 →").on_click(cx.listener(move |this, _, _, cx| {
                this.nav.task_detail = None;
                this.ask_agent(tid.clone(), false, cx);
            }))
        };
        let another = || {
            let tid = tid.clone();
            button("task-another", "再开一个会话 ▾")
                .on_click(cx.listener(move |this, _, _, cx| this.ask_agent(tid.clone(), true, cx)))
        };
        let edit = {
            let task = task.clone();
            button("task-edit", "编辑").on_click(
                cx.listener(move |this, _, window, cx| this.edit_task_form(&task, window, cx)),
            )
        };
        let divider = || {
            div()
                .w(px(T::BORDER_WIDTH))
                .h(px(T::S20))
                .mx(px(T::S2))
                .bg(T::color(T::border()))
        };
        let mut actions = div().flex().flex_wrap().items_center().gap(px(T::S6));
        if settled {
            // A settled todo has nothing to push forward: only undo and edit.
            actions = actions
                .child(plain(outcome_button("撤销结局", Outcome::Open)))
                .child(edit);
        } else {
            if status == TaskStatus::Proven {
                actions = actions.child(solid(outcome_button("标记完成", Outcome::Completed)));
                if running {
                    actions = actions.child(plain(back()));
                }
                actions = actions.child(plain(another()));
            } else if running {
                actions = actions
                    .child(solid(back()))
                    .child(plain(another()))
                    .child(divider())
                    .child(plain(outcome_button("标记完成", Outcome::Completed)));
            } else {
                let start = tid.clone();
                actions = actions.child(solid(button("task-start", "开始").on_click(
                    cx.listener(move |this, _, _, cx| this.ask_agent(start.clone(), false, cx)),
                )));
                if has_sessions {
                    actions = actions.child(plain(another()));
                }
                actions = actions
                    .child(divider())
                    .child(plain(outcome_button("标记完成", Outcome::Completed)));
            }
            if status == TaskStatus::Proven {
                actions = actions.child(divider());
            }
            if let Outcome::Deferred { .. } = outcome {
                actions = actions.child(outcome_button("撤销结局", Outcome::Open));
            } else {
                actions = actions.child(button("task-defer", "稍后再看…").on_click(cx.listener(
                    |this, _, window, cx| this.show_form(FormKind::TaskDefer, window, cx),
                )));
            }
            actions = actions
                .child(outcome_button("放弃", Outcome::Abandoned))
                .child(edit);
        }
        div()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(T::S16))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(T::S8))
                    .child(
                        div()
                            .text_size(px(T::TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(task.title.clone()),
                    )
                    .child(meta),
            )
            .child(actions)
    }
}
