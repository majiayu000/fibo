//! A space's todos in the column.
use super::*;

impl Workbench {
    /// The space's todos as the column lists them: in front by rank, and the folded
    /// ones (搁着, closed) apart. What the header counts is the front.
    pub(in crate::workbench) fn space_todos(&self) -> (Vec<&Task>, Vec<&Task>) {
        let mut rows: Vec<(&Task, fibo_core::status::TaskState)> = (self.workspace.tasks.iter())
            .filter(|t| t.space == self.nav.space)
            .map(|t| (t, super::task_detail::task_status(&self.workspace, t)))
            .collect();
        rows.sort_by_key(|(_, s)| s.rank());
        let (folded, front): (Vec<_>, Vec<_>) = rows.into_iter().partition(|(_, s)| s.folded());
        (
            front.into_iter().map(|(t, _)| t).collect(),
            folded.into_iter().map(|(t, _)| t).collect(),
        )
    }
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
        let (front, folded) = self.space_todos();
        let todo_count = front.len();
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
        let folded_open = self.tasks.todos_folded_open;
        let shown = front.len() + if folded_open { folded.len() } else { 0 };
        for (i, task) in front.iter().chain(folded.iter()).take(shown).enumerate() {
            if i == front.len() {
                sidebar = sidebar.child(self.folded_todos_toggle(&folded, cx));
            }
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
            let claimed = status == fibo_core::status::TaskState::Claimed;
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
                            if claimed { t!("确认") } else { t!("开始") },
                        )
                        .h(px(T::ICON_HEIGHT))
                        .px(px(T::S6))
                        // Its text, not its padding, ends where the sessions' times do.
                        .mr(-px(T::S6))
                        .text_size(px(T::META))
                        .text_color(T::color(T::link()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            // An Agent said it is done: one click completes it.
                            if claimed {
                                this.complete_todo(&start, cx);
                            } else {
                                this.ask_agent(start.clone(), false, cx);
                            }
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
        if !folded.is_empty() && !folded_open {
            sidebar = sidebar.child(self.folded_todos_toggle(&folded, cx));
        }
        sidebar
    }

    /// 搁着 3 · 已收尾 5: the folded part of the list, opened and closed in place.
    fn folded_todos_toggle(
        &self,
        folded: &[&Task],
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let count = |pick: fn(fibo_core::status::TaskState) -> bool| {
            (folded.iter())
                .filter(|t| pick(super::task_detail::task_status(&self.workspace, t)))
                .count()
        };
        let idle = count(|s| s == fibo_core::status::TaskState::Idle);
        let closed = count(fibo_core::status::TaskState::closed);
        let mut words = Vec::new();
        if idle > 0 {
            words.push(t!("搁着 {idle}", idle).to_owned());
        }
        if closed > 0 {
            words.push(t!("已收尾 {closed}", closed).to_owned());
        }
        let open = self.tasks.todos_folded_open;
        div()
            .id("todos-folded")
            .h(px(T::TASK_ROW))
            .px(px(T::S8))
            .flex()
            .items_center()
            .cursor_pointer()
            .child(label(format!(
                "{} {}",
                if open { "▾" } else { "▸" },
                words.join(" · ")
            )))
            .on_click(cx.listener(|this, _, _, cx| {
                this.tasks.todos_folded_open = !this.tasks.todos_folded_open;
                cx.notify();
            }))
    }
}
