//! The todo detail: where it stands, acceptance with evidence per criterion, its sessions.
use super::*;
use mooring_core::content::Evidence;

mod criterion;
mod evidence;
mod head;

/// See `mooring_core::status::task_state`, the one place a todo's state is decided.
pub(super) use mooring_core::status::TaskState as TaskStatus;
pub(super) fn task_status(w: &Workspace, task: &Task) -> TaskStatus {
    mooring_core::status::task_state(w, task, now())
}
/// Records from the todo's sessions that a criterion can hold, newest first, each with the
/// session it came from; ones already on `criterion` are left out.
pub(super) fn attachable(
    w: &Workspace,
    task: &str,
    criterion: Option<&AcceptanceCriterion>,
) -> Vec<CriterionEvidence> {
    let mut records: Vec<CriterionEvidence> = w
        .agent_sessions
        .iter()
        .filter(|s| s.task.as_deref() == Some(task))
        .filter_map(|s| w.content.get(&s.id).map(|c| (s, c)))
        .flat_map(|(s, c)| {
            c.evidence
                .iter()
                .filter(|e| e.attachable())
                .map(|e| CriterionEvidence {
                    session: s.id.clone(),
                    record: e.clone(),
                })
        })
        .filter(|e| criterion.is_none_or(|c| !c.evidence.contains(e)))
        .collect();
    records.sort_by_key(|e| std::cmp::Reverse(e.record.at));
    records
}
fn kind_name(e: &Evidence) -> &'static str {
    match e.kind.as_str() {
        "test" => "测试",
        "commit" => "提交",
        _ => "文件",
    }
}
/// How the record ended: an exit code, or what a file change and a commit did.
fn result_name(e: &Evidence) -> String {
    match (e.kind.as_str(), e.exit_code) {
        ("file", Some(0)) => "已写入".into(),
        ("commit", Some(0)) => "已提交".into(),
        (_, Some(code)) => format!("exit {code}"),
        _ => "未知".into(),
    }
}
fn hm(at: u64) -> String {
    clock(at).chars().take(5).collect()
}
/// The ring before a status: empty, half or full as the todo moves along.
fn ring(status: TaskStatus) -> gpui::Div {
    let ink = T::color(T::secondary());
    let base = div()
        .size(px(T::DOT + 1.))
        .flex_shrink_0()
        .rounded_full()
        .border(px(T::CARET_WIDTH))
        .border_color(ink)
        .overflow_hidden();
    match status {
        TaskStatus::NotStarted | TaskStatus::Deferred | TaskStatus::Abandoned => {
            base.border_color(T::color(T::muted()))
        }
        TaskStatus::Working | TaskStatus::Settled => {
            base.flex().child(div().w_1_2().h_full().bg(ink))
        }
        TaskStatus::Proven | TaskStatus::Completed => base.bg(ink),
    }
}
fn panel() -> gpui::Div {
    div()
        .min_w_0()
        .flex()
        .flex_col()
        .bg(T::color(T::popover()))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::card_border()))
        .rounded(px(T::POPOVER_RADIUS))
        .overflow_hidden()
}
fn panel_head(title: &str, count: String) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(T::S8))
        .px(px(T::S16))
        .py(px(T::S12))
        .border_b(px(T::BORDER_WIDTH))
        .border_color(T::color(T::border()))
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_size(px(T::SMALL))
                .text_color(T::color(T::muted()))
                .child(count),
        )
}
fn link(
    id: impl Into<gpui::ElementId>,
    text: impl Into<gpui::SharedString>,
) -> gpui::Stateful<gpui::Div> {
    button(id, text)
        .h(px(T::ICON_WIDTH))
        .px(px(T::S6))
        .text_color(T::color(T::link()))
}
fn muted(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .text_size(px(T::META))
        .text_color(T::color(T::muted()))
        .child(text.into())
}

impl Workbench {
    pub(super) fn task_detail_view(
        &self,
        task: &Task,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let status = task_status(&self.workspace, task);
        let sessions: Vec<AgentSession> = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| s.task.as_ref() == Some(&task.id))
            .cloned()
            .collect();
        let mut left = div().flex_1().min_w_0().flex().flex_col().gap(px(T::S16));
        if !task.description.trim().is_empty() {
            left = left.child(
                panel()
                    .child(
                        panel_head("说明", String::new())
                            .child(div().flex_1())
                            .child(link("task-edit-description", "编辑").on_click({
                                let task = task.clone();
                                cx.listener(move |this, _, window, cx| {
                                    this.edit_task_form(&task, window, cx)
                                })
                            })),
                    )
                    .child(
                        div()
                            .px(px(T::S16))
                            .py(px(T::S12))
                            .text_color(T::color(T::body()))
                            .child(task.description.trim().to_owned()),
                    ),
            );
        }
        left = left.child(self.criteria_panel(task, cx));
        div()
            .id("task-detail")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(T::OVERVIEW_X))
            .py(px(T::S20))
            .child(
                div()
                    .w(px(T::TODO_DETAIL_WIDTH))
                    .max_w_full()
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(px(T::S16))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(T::S8))
                            .text_size(px(T::SMALL))
                            .text_color(T::color(T::secondary()))
                            .child(button("task-close", "← 返回画面").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.nav.task_detail = None;
                                    cx.notify();
                                },
                            )))
                            .child("·")
                            .child("待办"),
                    )
                    .child(self.detail_head(task, status, !sessions.is_empty(), cx))
                    .children(self.agent_picker(&task.id, cx))
                    .when(!task.acceptance.is_empty(), |d| {
                        d.child(self.progress(task, status, &sessions))
                    })
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(T::S16))
                            .child(left)
                            .child(
                                div()
                                    .w(px(T::TODO_DETAIL_SIDE))
                                    .flex_shrink_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(T::S16))
                                    .child(self.sessions_panel(task, &sessions, cx))
                                    .when(!sessions.is_empty(), |d| {
                                        d.child(muted(
                                        "标记完成不会停掉关联会话；它们各自的结局仍在鸟瞰里处理。",
                                    ))
                                    }),
                            ),
                    ),
            )
    }
    /// The 「n / m 有证据」 strip: one cell per criterion, filled once evidence proves it.
    fn progress(&self, task: &Task, status: TaskStatus, sessions: &[AgentSession]) -> gpui::Div {
        let proven = task.proven();
        let total = task.acceptance.len();
        let note = if status == TaskStatus::Proven {
            let last = task
                .acceptance
                .iter()
                .flat_map(|c| c.evidence.iter())
                .filter(|e| e.record.proves())
                .map(|e| e.record.at)
                .max()
                .unwrap_or_default();
            let done = sessions
                .iter()
                .filter(|s| status_of(&self.workspace, s) == Status::Review)
                .count();
            [
                format!(
                    "最后一条证据 {}{}",
                    hm(last),
                    if !sessions.is_empty() && done == sessions.len() {
                        format!(" · {done} 个会话都在等你验收")
                    } else {
                        String::new()
                    }
                ),
                "核对后点「标记完成」".to_owned(),
            ]
        } else {
            [
                "证据只认成功的测试、提交和文件改动".to_owned(),
                "Agent 自己说「做完了」不算".to_owned(),
            ]
        };
        div()
            .flex()
            .items_center()
            .gap(px(T::S16))
            .px(px(T::S16))
            .py(px(T::S14))
            .bg(T::color(T::popover()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .rounded(px(T::POPOVER_RADIUS))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(T::S4))
                    .font_features(T::tabular())
                    .child(
                        div()
                            .text_size(px(T::PROGRESS_NUMBER))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(proven.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(T::FORM_TITLE))
                            .text_color(T::color(T::muted()))
                            .child(format!("/ {total}")),
                    )
                    .child(
                        div()
                            .ml(px(T::S6))
                            .text_size(px(T::SMALL))
                            .text_color(T::color(T::secondary()))
                            .child("有证据"),
                    ),
            )
            .child(div().flex_1().min_w_0().flex().gap(px(T::S4)).children(
                task.acceptance.iter().enumerate().map(|(i, c)| {
                    let ok = c.proven();
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(T::S4))
                        .child(
                            div()
                                .h(px(T::PROGRESS_CELL))
                                .rounded(px(T::PROGRESS_CELL_RADIUS))
                                .when(ok, |d| d.bg(T::color(T::secondary())))
                                .when(!ok, |d| {
                                    d.bg(T::color(T::app()))
                                        .border(px(T::BORDER_WIDTH))
                                        .border_color(T::color(T::control()))
                                }),
                        )
                        .child(
                            muted(format!(
                                "{} · {}",
                                i + 1,
                                mooring_core::content::short(&c.description, 16)
                            ))
                            .truncate()
                            .when(ok, |d| d.text_color(T::color(T::secondary()))),
                        )
                }),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::secondary()))
                    .children(note),
            )
    }
    fn criteria_panel(&self, task: &Task, cx: &Context<Self>) -> gpui::Div {
        let total = task.acceptance.len();
        let mut p = panel().child(panel_head(
            "验收标准",
            if total == 0 {
                "0".into()
            } else {
                format!("{} / {total} 有证据", task.proven())
            },
        ));
        if total == 0 {
            let edit = task.clone();
            return p.child(
                div()
                    .px(px(T::S16))
                    .py(px(T::S14))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .text_color(T::color(T::muted()))
                    .child("没有写验收标准，这条待办靠你自己判断完成")
                    .child(div().flex_1())
                    .child(link("task-criteria-add", "补写").on_click(cx.listener(
                        move |this, _, window, cx| this.edit_task_form(&edit, window, cx),
                    ))),
            );
        }
        for (i, criterion) in task.acceptance.iter().enumerate() {
            p = p.child(self.criterion_row(task, i, criterion, i + 1 == total, cx));
        }
        p
    }
    fn sessions_panel(
        &self,
        task: &Task,
        sessions: &[AgentSession],
        cx: &Context<Self>,
    ) -> gpui::Div {
        let mut p = panel().child(panel_head("关联会话", sessions.len().to_string()));
        if sessions.is_empty() {
            let tid = task.id.clone();
            return p.child(
                div()
                    .px(px(T::S16))
                    .py(px(T::S14))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .text_color(T::color(T::muted()))
                    .child("没有会话做过这条待办")
                    .child(div().flex_1())
                    .child(link("task-session-start", "再开一个会话 ▾").on_click(
                        cx.listener(move |this, _, _, cx| this.ask_agent(tid.clone(), false, cx)),
                    )),
            );
        }
        let at = now();
        let count = sessions.len();
        for (n, session) in sessions.iter().enumerate() {
            let group = status_of(&self.workspace, session);
            let updated = self.workspace.session_updated(session);
            let open = session.clone();
            p = p.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "task-session-{}",
                        session.id
                    )))
                    .flex()
                    .items_center()
                    .gap(px(T::S10))
                    .px(px(T::S16))
                    .py(px(T::S10))
                    .cursor_pointer()
                    .hover(|s| s.bg(T::color(T::raised())))
                    .when(n + 1 < count, |d| {
                        d.border_b(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::border()))
                    })
                    .child(dot(group))
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
                                    .text_color(T::color(T::primary()))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(self.session_title(session)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(T::S6))
                                    .text_size(px(T::META))
                                    .text_color(T::color(T::muted()))
                                    .child(
                                        div()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(group.colors().1)
                                            .child(group.label()),
                                    )
                                    .child(session.runtime.name())
                                    .child(ago(updated, at))
                                    .when(
                                        self.workspace
                                            .content
                                            .get(&session.id)
                                            .is_some_and(|c| c.waking()),
                                        |d| d.child("等待唤醒"),
                                    ),
                            ),
                    )
                    .child(muted("打开 →").text_color(T::color(T::link())))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.nav.task_detail = None;
                        this.enter_card(&open, window, cx);
                    })),
            );
        }
        p
    }
}

#[cfg(test)]
mod tests;
