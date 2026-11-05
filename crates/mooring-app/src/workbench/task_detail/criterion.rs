//! One acceptance criterion of the todo detail, with its evidence.
use super::*;

impl Workbench {
    pub(super) fn criterion_row(
        &self,
        task: &Task,
        i: usize,
        criterion: &AcceptanceCriterion,
        last: bool,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let ok = criterion.proven();
        // Newest first: what proves it, and the attempts that did not.
        let mut proofs: Vec<(usize, &CriterionEvidence)> = criterion
            .evidence
            .iter()
            .enumerate()
            .filter(|(_, e)| e.record.proves())
            .collect();
        let mut fails: Vec<(usize, &CriterionEvidence)> = criterion
            .evidence
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.record.proves())
            .collect();
        proofs.sort_by_key(|e| std::cmp::Reverse(e.1.record.at));
        fails.sort_by_key(|e| std::cmp::Reverse(e.1.record.at));
        let count = match (proofs.len(), fails.len()) {
            (0, 0) => "0 条".to_owned(),
            (0, f) => format!("0 条 · {f} 次失败"),
            (n, 0) => format!("{n} 条证据"),
            (n, f) => format!("{n} 条证据 · {f} 次失败"),
        };
        let expanded = self.tasks.expanded.contains(&criterion.id);
        let mut body = div().flex().flex_col().gap(px(T::S2)).mt(px(T::S4));
        let available = attachable(&self.workspace, &task.id, Some(criterion));
        if expanded {
            for (at, e) in proofs.iter().chain(fails.iter()) {
                body = body.child(self.evidence_row(task, i, *at, e, false, true, cx));
            }
        } else if let Some((at, e)) = proofs.first() {
            body = body.child(self.evidence_row(task, i, *at, e, false, false, cx));
            let mut more = Vec::new();
            if let Some((_, next)) = proofs.get(1) {
                more.push(format!(
                    "另 {} 条：{} · {}",
                    proofs.len() - 1,
                    mooring_core::content::short(&next.record.summary, 28),
                    hm(next.record.at)
                ));
            }
            if let Some((_, f)) = fails.first() {
                more.push(format!(
                    "{} {} 次失败：{} · {}",
                    if f.record.at < e.record.at {
                        "更早"
                    } else {
                        "之后又有"
                    },
                    fails.len(),
                    result_name(&f.record),
                    hm(f.record.at)
                ));
            }
            if !more.is_empty() {
                let cid = criterion.id.clone();
                body = body.child(
                    button(("evidence-more", i), more.join("；"))
                        .h(px(T::ICON_HEIGHT))
                        .px(px(T::S8))
                        .justify_start()
                        .text_size(px(T::META))
                        .text_color(T::color(T::link()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.tasks.expanded.insert(cid.clone());
                            cx.notify();
                        })),
                );
            }
        } else if let Some((at, e)) = fails.first() {
            body = body.child(self.evidence_row(task, i, *at, e, true, false, cx));
        } else {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .px(px(T::S8))
                    .py(px(T::S4))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::muted()))
                    .child(if available.is_empty() {
                        "还没有证据 · 关联会话里还没有测试、提交或文件改动".to_owned()
                    } else {
                        format!("还没有证据 · 关联会话里有 {} 条记录可以挂", available.len())
                    }),
            );
        }
        let cid = criterion.id.clone();
        let picking = self.tasks.attaching.as_ref() == Some(&criterion.id);
        body = body.child(
            div()
                .flex()
                .gap(px(T::S4))
                .when(!available.is_empty() || picking, |d| {
                    let cid = cid.clone();
                    d.child(
                        link(
                            ("evidence-attach", i),
                            if picking {
                                "收起"
                            } else {
                                "挂一条证据 ▾"
                            },
                        )
                        .text_size(px(T::META))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.tasks.attaching = if this.tasks.attaching.as_ref() == Some(&cid) {
                                None
                            } else {
                                Some(cid.clone())
                            };
                            cx.notify();
                        })),
                    )
                })
                .when(expanded, |d| {
                    let cid = cid.clone();
                    d.child(
                        link(("evidence-fold", i), "收起较早的证据")
                            .text_size(px(T::META))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.tasks.expanded.remove(&cid);
                                cx.notify();
                            })),
                    )
                }),
        );
        if picking {
            body = body.child(self.evidence_picker(task, i, &available, cx));
        }
        div()
            .flex()
            .gap(px(T::S10))
            .pl(px(T::S14))
            .pr(px(T::S16))
            .py(px(T::S12))
            .when(!last, |d| {
                d.border_b(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
            })
            .child(
                div()
                    .size(px(T::CRITERION_BOX))
                    .mt(px(T::S2))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(T::CHIP_RADIUS))
                    .border(px(T::CARET_WIDTH))
                    .border_color(T::color(T::control()))
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::on_accent()))
                    .when(ok, |d| {
                        d.bg(T::color(T::secondary()))
                            .border_color(T::color(T::secondary()))
                            .child("✓")
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(T::S8))
                            .child(muted((i + 1).to_string()).text_size(px(T::SMALL)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_color(T::color(T::primary()))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(criterion.description.clone()),
                            )
                            .child(muted(count).flex_shrink_0()),
                    )
                    .child(body),
            )
    }
}
