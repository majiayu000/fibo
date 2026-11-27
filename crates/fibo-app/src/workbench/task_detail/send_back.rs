//! 退回: you say an Agent's 做完了 is not so. A note of yours takes the claim back, so the
//! todo is open again and the next conversation reads why.
use super::*;

impl Workbench {
    pub(in crate::workbench) fn send_back_button(
        &self,
        task: &str,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let task = task.to_owned();
        button("task-send-back", t!("退回")).on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.edit(
                Edit::AddTaskNote {
                    task: task.clone(),
                    session: None,
                    text: t!("用户退回：还没做完").into(),
                    done: false,
                },
                cx,
            )
        }))
    }
}
