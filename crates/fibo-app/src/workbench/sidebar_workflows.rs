//! Workflows are already grouped by the host, beside ordinary child agents.
use super::*;
use crate::ui::{theme::Flat as F, tone::Tone};
use fibo_core::content::Workflow;

impl Workbench {
    pub(super) fn workflow_steps(
        &self,
        workflow: &Workflow,
        live: bool,
        run: gpui::Hsla,
    ) -> gpui::AnyElement {
        div()
            .ml(px(F::SUB_BRANCH + T::S6))
            .flex()
            .flex_col()
            .children(workflow.phases.iter().filter_map(|phase| {
                let steps: Vec<_> = workflow
                    .steps
                    .iter()
                    .filter(|s| &s.phase == phase)
                    .collect();
                if steps.is_empty() {
                    return None;
                }
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .h(px(F::SUB_LEAF))
                                .text_size(px(F::SUB))
                                .text_color(T::color(F::third()))
                                .child(format!("{phase} · {}", steps.len())),
                        )
                        .children(steps.into_iter().map(|s| {
                            let on = live && s.agent.running();
                            let (mark, color) = if on {
                                ("·", run)
                            } else if s.agent.stopped || workflow.journal && s.agent.running() {
                                ("✕", Tone::Danger.dot())
                            } else {
                                ("✓", T::color(F::done()))
                            };
                            div()
                                .h(px(F::SUB_LEAF))
                                .flex()
                                .items_center()
                                .gap(px(T::S6))
                                .text_size(px(F::SUB))
                                .child(div().w(px(F::STATE)).text_color(color).child(mark))
                                .child(
                                    div()
                                        .flex_1()
                                        .truncate()
                                        .text_color(T::color(F::second()))
                                        .child(s.agent.name.clone()),
                                )
                        })),
                )
            }))
            .into_any_element()
    }
}
