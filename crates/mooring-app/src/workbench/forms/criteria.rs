//! The criteria checklist of the todo form.
use super::*;

impl Workbench {
    /// The criteria checklist: drag a line by its grip, Enter opens the next line, × drops it.
    pub(super) fn form_criteria(
        &self,
        form: &Form,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let warn = Tone::Warn.ink();
        let texts: Vec<String> = form
            .criteria
            .iter()
            .map(|r| r.field.read(cx).text.trim().to_owned())
            .collect();
        let written = texts.iter().filter(|t| !t.is_empty()).count();
        let mut list = div()
            .flex()
            .flex_col()
            .rounded(px(T::CONTROL_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::field_edge()))
            .bg(T::color(T::panel()))
            .overflow_hidden();
        for (i, row) in form.criteria.iter().enumerate() {
            let same = (!texts[i].is_empty())
                .then(|| texts[..i].iter().position(|t| t == &texts[i]))
                .flatten();
            let focused = row.field.focus_handle(cx).is_focused(window);
            let dragged = DraggedCriterion {
                index: i,
                text: texts[i].clone(),
            };
            list = list.child(
                div()
                    .id(("criterion", i))
                    .h(px(T::CRITERION_ROW))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .pl(px(T::S6))
                    .pr(px(T::S8))
                    .border_b(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .when(focused, |d| d.bg(T::color(T::popover())))
                    .when(same.is_some(), |d| {
                        d.border_l(px(T::FOCUS_RING)).border_color(warn)
                    })
                    .drag_over::<DraggedCriterion>(|s, _, _, _| {
                        s.border_t(px(T::FOCUS_RING))
                            .border_color(T::color(T::accent()))
                    })
                    .on_drop(cx.listener(move |this, d: &DraggedCriterion, _, cx| {
                        this.move_criterion(d.index, i, cx)
                    }))
                    .child(
                        div()
                            .id(("criterion-grip", i))
                            .w(px(T::CRITERION_GRIP))
                            .flex_shrink_0()
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::muted()))
                            .cursor_grab()
                            .child("⋮⋮")
                            .on_drag(dragged, |d, _, _, cx| cx.new(|_| d.clone())),
                    )
                    .child(
                        div()
                            .w(px(T::CRITERION_NUMBER))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .text_size(px(T::SMALL))
                            .text_color(T::color(T::muted()))
                            .font_features(T::tabular())
                            .child((i + 1).to_string()),
                    )
                    .child(div().flex_1().min_w_0().h_full().child(row.field.clone()))
                    .when_some(same, |d, j| {
                        d.child(
                            hint(t!("和第 {n} 条相同，保存时合并", n = j + 1))
                                .flex_shrink_0()
                                .text_color(warn),
                        )
                    })
                    .when(same.is_none() && focused, |d| {
                        d.child(hint(t!("回车加下一条")).flex_shrink_0())
                    })
                    .child(
                        button(("criterion-delete", i), "×")
                            .aria_label(t!("删除第 {n} 条", n = i + 1))
                            .w(px(T::ICON_HEIGHT))
                            .h(px(T::ICON_HEIGHT))
                            .px_0()
                            .flex_shrink_0()
                            .text_color(T::color(T::muted()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(f) = &mut this.tasks.form
                                    && i < f.criteria.len()
                                {
                                    f.criteria.remove(i);
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        let count = form.criteria.len();
        list = list.child(
            div()
                .id("criterion-add")
                .h(px(T::CRITERION_ROW))
                .flex()
                .items_center()
                .gap(px(T::S8))
                .pl(px(T::S6))
                .pr(px(T::S8))
                .cursor_text()
                .drag_over::<DraggedCriterion>(|s, _, _, _| {
                    s.border_t(px(T::FOCUS_RING))
                        .border_color(T::color(T::accent()))
                })
                .on_drop(cx.listener(move |this, d: &DraggedCriterion, _, cx| {
                    this.move_criterion(d.index, count, cx)
                }))
                .child(div().w(px(T::CRITERION_GRIP)).flex_shrink_0())
                .child(
                    div()
                        .w(px(T::CRITERION_NUMBER))
                        .flex_shrink_0()
                        .flex()
                        .justify_end()
                        .text_color(T::color(T::muted()))
                        .child(t!("＋")),
                )
                .child(div().text_color(T::color(T::muted())).child(if count == 0 {
                    t!("写一条完成时怎么核对…")
                } else {
                    t!("再加一条…")
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.add_criterion(count, None, true, window, cx)
                })),
        );
        div()
            .flex()
            .flex_col()
            .gap(px(T::S6))
            .child(form_label(
                t!("验收标准"),
                Some(t!("可选 · 也会交给 Agent，完成时逐条核对")),
                (written > 0).then(|| t!("{written} 条", written)),
            ))
            .child(list)
            .child(if written == 0 && !form.fields[0].read(cx).text.trim().is_empty() {
                hint(t!("没有验收标准，完成时没有东西可核对；开始后待办信息条会一直提示「补写」"))
                    .text_color(warn)
            } else {
                hint(t!("每条最好能被一条命令、一次测试或一处文件改动证明；Fibo 会从关联会话里找这类记录当证据。"))
            })
    }
}
