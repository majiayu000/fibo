//! Evidence on a criterion: an attached record, the picker, and saving the change.
use super::*;

impl Workbench {
    /// One attached record: its kind, what it says, how it ended, when, and where from.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evidence_row(
        &self,
        task: &Task,
        criterion: usize,
        index: usize,
        e: &CriterionEvidence,
        last_try: bool,
        removable: bool,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let failed = !e.record.proves();
        let session = self
            .workspace
            .agent_sessions
            .iter()
            .find(|s| s.id == e.session)
            .cloned();
        let from = session
            .as_ref()
            .map_or_else(|| "已关闭的会话".to_owned(), |s| self.session_title(s));
        let path = e.record.source.path.clone();
        let key = format!("{criterion}-{index}");
        div()
            .flex()
            .flex_col()
            .px(px(T::S8))
            .py(px(T::S4))
            .rounded(px(T::CONTROL_RADIUS))
            .hover(|s| s.bg(T::color(T::raised())))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(T::S10))
                    .text_size(px(T::SMALL))
                    .child(
                        muted(kind_name(&e.record))
                            .w(px(T::EVIDENCE_KIND))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font(T::mono(cx))
                            .text_size(px(T::META))
                            .text_color(T::color(if failed { T::muted() } else { T::body() }))
                            .child(e.record.summary.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(if failed {
                                Tone::Danger.ink()
                            } else {
                                T::color(T::secondary())
                            })
                            .when(failed, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                            .child(result_name(&e.record)),
                    )
                    .child(muted(hm(e.record.at)).flex_shrink_0()),
            )
            .child(
                div()
                    .pl(px(T::EVIDENCE_KIND + T::S10))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(T::S4))
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("evidence-from-{key}")))
                            .cursor_pointer()
                            .hover(|s| s.text_color(T::color(T::link())))
                            .child(format!("来自「{from}」"))
                            .when_some(session, |d, s| {
                                d.on_click(cx.listener(move |this, _, window, cx| {
                                    this.nav.task_detail = None;
                                    this.enter_card(&s, window, cx);
                                }))
                            }),
                    )
                    .when(last_try, |d| d.child("· 最近一次尝试，不算证据"))
                    .child("·")
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("evidence-source-{key}")))
                            .cursor_pointer()
                            .text_color(T::color(T::link()))
                            .child("看原文 ↗")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                // The session log itself, opened as text where the record sits.
                                if let Err(e) = std::process::Command::new("open")
                                    .args(["-t", &path])
                                    .spawn()
                                {
                                    this.error = Some(format!("无法打开会话原文：{e}"));
                                    cx.notify();
                                }
                            })),
                    )
                    .when(removable || last_try, |d| {
                        let (tid, cid, gone) = (
                            task.id.clone(),
                            task.acceptance[criterion].id.clone(),
                            e.clone(),
                        );
                        d.child("·").child(
                            div()
                                .id(gpui::SharedString::from(format!("evidence-remove-{key}")))
                                .cursor_pointer()
                                .hover(|s| s.text_color(T::color(T::link())))
                                .child("移除")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.change_criterion(
                                        &tid,
                                        &cid,
                                        |c| c.evidence.retain(|x| x != &gone),
                                        cx,
                                    )
                                })),
                        )
                    }),
            )
    }
    /// The records the todo's sessions left that this criterion can hold; a click attaches one.
    pub(super) fn evidence_picker(
        &self,
        task: &Task,
        criterion: usize,
        available: &[CriterionEvidence],
        cx: &Context<Self>,
    ) -> gpui::Div {
        let mut list = div()
            .mt(px(T::S4))
            .flex()
            .flex_col()
            .p(px(T::S4))
            .rounded(px(T::CONTROL_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .bg(T::color(T::pane()));
        for (n, e) in available.iter().take(12).enumerate() {
            let failed = !e.record.proves();
            let from = self
                .workspace
                .agent_sessions
                .iter()
                .find(|s| s.id == e.session)
                .map(|s| self.session_title(s))
                .unwrap_or_default();
            let (tid, cid, attach) = (
                task.id.clone(),
                task.acceptance[criterion].id.clone(),
                e.clone(),
            );
            list = list.child(
                div()
                    .id(("evidence-pick", n))
                    .flex()
                    .items_baseline()
                    .gap(px(T::S10))
                    .px(px(T::S8))
                    .py(px(T::S4))
                    .rounded(px(T::CHIP_RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(T::color(T::raised())))
                    .text_size(px(T::SMALL))
                    .child(
                        muted(kind_name(&e.record))
                            .w(px(T::EVIDENCE_KIND))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font(T::mono(cx))
                            .text_size(px(T::META))
                            .child(e.record.summary.clone()),
                    )
                    .child(
                        muted(if failed {
                            format!("{} · 失败，只记作尝试", result_name(&e.record))
                        } else {
                            result_name(&e.record)
                        })
                        .flex_shrink_0()
                        .when(failed, |d| d.text_color(Tone::Danger.ink())),
                    )
                    .child(
                        muted(format!(
                            "{} · {}",
                            hm(e.record.at),
                            mooring_core::content::short(&from, 10)
                        ))
                        .flex_shrink_0(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tasks.attaching = None;
                        this.change_criterion(&tid, &cid, |c| c.evidence.push(attach.clone()), cx)
                    })),
            );
        }
        list
    }
    /// Changes one criterion of the todo as it is now and saves the whole todo.
    fn change_criterion(
        &mut self,
        task: &str,
        criterion: &str,
        change: impl FnOnce(&mut AcceptanceCriterion),
        cx: &mut Context<Self>,
    ) {
        let Some(mut t) = self.workspace.tasks.iter().find(|t| t.id == task).cloned() else {
            return;
        };
        if let Some(c) = t.acceptance.iter_mut().find(|c| c.id == criterion) {
            change(c);
            self.edit(Edit::UpdateTask(t), cx);
        }
    }
}
