//! A space's todos in the column.
use super::*;

impl Workbench {
    /// The folded column: a way back, and how many sessions in this space need you.
    /// The space's todos: what is still to be handed to an Agent, above what is running.
    /// In the dense presets the header lives in the space line (`待办 3`), so only the
    /// quick-add field and, once opened, the list come from here.
    pub(super) fn todos_view(
        &self,
        space_name: &str,
        dense: bool,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let mut sidebar = div().flex().flex_col();
        let todo_count = self
            .workspace
            .tasks
            .iter()
            .filter(|t| t.space == self.nav.space)
            .count();
        let open = self.tasks.todos_open;
        if dense {
            sidebar = sidebar
                .when(open || self.tasks.quick_task.is_some(), |d| d.pt(px(T::S6)))
                .children(self.quick_task_row(cx));
        } else {
            sidebar = sidebar
                .child(
                    div()
                        .id("todos-toggle")
                        .mt(px(T::S14))
                        .pt(px(T::S12))
                        .px(px(T::S8))
                        .mb(px(T::S4))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .child(
                            label(t!(
                                "{arrow} {space_name} 的待办 · {todo_count}",
                                arrow = if open { "▾" } else { "▸" },
                                space_name,
                                todo_count
                            ))
                            .flex_1(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tasks.todos_open = !this.tasks.todos_open;
                            cx.notify();
                        }))
                        .child(
                            button("sidebar-new-task", t!("＋"))
                                .aria_label(t!("新待办"))
                                .border_0()
                                .text_color(T::color(T::link()))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    // The header toggles on click; ＋ must only open it.
                                    cx.stop_propagation();
                                    this.show_quick_task(window, cx)
                                })),
                        ),
                )
                .children(self.quick_task_row(cx));
        }
        if !open {
            return sidebar;
        }
        // First open: say what the empty pane is for before asking for a task.
        if self.tasks.quick_task.is_none()
            && !self
                .workspace
                .tasks
                .iter()
                .any(|t| t.space == self.nav.space)
        {
            sidebar = sidebar.child(
                div()
                    .px(px(T::S8))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(T::S6))
                    .child(label(t!(
                        "还没有待办。先记下要做的事，再点「开始」派给 agent。"
                    )))
                    .child(
                        button("empty-new-task", t!("＋ 新建待办"))
                            .h(px(T::ICON_HEIGHT))
                            .px(px(T::S6))
                            .text_size(px(T::META))
                            .text_color(T::color(T::link()))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_quick_task(window, cx)),
                            ),
                    ),
            );
        }
        for task in self
            .workspace
            .tasks
            .iter()
            .filter(|t| t.space == self.nav.space)
        {
            let tid = task.id.clone();
            let start = tid.clone();
            let count = self
                .workspace
                .agent_sessions
                .iter()
                .filter(|s| s.task.as_ref() == Some(&tid))
                .count();
            // The detail's word for where it stands, with how many sessions it has.
            let status = super::task_detail::task_status(&self.workspace, task);
            let state = match status {
                fibo_core::status::TaskState::Working
                | fibo_core::status::TaskState::Settled
                | fibo_core::status::TaskState::Proven => {
                    t!("{label} · {count} 个会话", label = status.label(), count)
                }
                _ => status.label().to_owned(),
            };
            sidebar = sidebar.child(
                div()
                    .id(gpui::SharedString::from(format!("task-{tid}")))
                    .h(px(T::TASK_ROW))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .rounded(px(T::CONTROL_RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(T::color(T::raised())))
                    .child(
                        div()
                            .size(px(T::S8))
                            .flex_shrink_0()
                            .rounded_full()
                            .border(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::link()))
                            .when(count > 0, |d| d.bg(T::color(T::accent()))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(T::MONO_SIZE))
                                    .child(task.title.clone()),
                            )
                            .child(label(state).text_size(px(T::CHIP_SIZE))),
                    )
                    .child(
                        button(
                            gpui::SharedString::from(format!("start-task-{tid}")),
                            t!("开始"),
                        )
                        .h(px(T::ICON_HEIGHT))
                        .px(px(T::S6))
                        // Its text, not its padding, ends where the sessions' times do.
                        .mr(-px(T::S6))
                        .text_size(px(T::META))
                        .text_color(T::color(T::link()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.ask_agent(start.clone(), false, cx);
                        })),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.nav.task_detail = Some(tid.clone());
                        cx.notify();
                    })),
            );
            if let Some(picker) = self.agent_picker(&task.id, cx) {
                sidebar = sidebar.child(picker);
            }
        }
        sidebar
    }
}
