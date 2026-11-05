//! The preview beside the todo form.
use super::*;

impl Workbench {
    /// What 「保存并开始」 then does, and the exact text placed in the Agent's input box.
    pub(super) fn form_preview(&self, form: &Form, draft: &Task, cx: &Context<Self>) -> gpui::Div {
        let space = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == form.space)
            .map_or_else(String::new, |s| s.name.clone());
        let agent = form.agent;
        // A missing title still shows where it will go, rather than a blank first line.
        let text = if draft.title.is_empty() {
            Task {
                title: "（先写要做什么）".into(),
                ..draft.clone()
            }
            .prompt()
        } else {
            draft.prompt()
        };
        let step = |n: &str, you: bool, body: gpui::AnyElement| {
            div()
                .flex()
                .items_start()
                .gap(px(T::S8))
                .child(
                    div()
                        .size(px(T::STEP_DOT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::control()))
                        .text_size(px(T::CHIP_SIZE))
                        .text_color(T::color(T::secondary()))
                        .when(you, |d| {
                            d.bg(T::color(T::accent()))
                                .border_color(T::color(T::accent()))
                                .text_color(T::color(T::on_accent()))
                        })
                        .child(n.to_owned()),
                )
                .child(div().flex_1().min_w_0().child(body))
        };
        let code = |s: String| {
            div()
                .px(px(T::S4))
                .rounded(px(T::CHIP_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .bg(T::color(T::app()))
                .font(T::mono(cx))
                .text_size(px(T::SMALL))
                .text_color(T::color(T::primary()))
                .child(s)
        };
        let copy = text.clone();
        div()
            .w(px(T::TODO_PREVIEW_WIDTH))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(T::color(T::popover()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .rounded(px(T::POPOVER_RADIUS))
            .overflow_hidden()
            .child(
                div()
                    .px(px(T::S16))
                    .pt(px(T::S14))
                    .pb(px(T::S12))
                    .flex()
                    .flex_col()
                    .gap(px(T::S10))
                    .border_b(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("保存并开始之后"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(T::S8))
                            .text_size(px(T::SMALL))
                            .child(step(
                                "1",
                                false,
                                format!("在「{space}」新开一个画面，标签就叫待办标题")
                                    .into_any_element(),
                            ))
                            .child(step(
                                "2",
                                false,
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(T::S4))
                                    .child("运行")
                                    .child(code(agent.command()))
                                    .child("，等它的输入框出现")
                                    .into_any_element(),
                            ))
                            .child(step(
                                "3",
                                false,
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .child(format!("把下面这段放进 {} 的输入框，", agent.name()))
                                    .child(
                                        div().font_weight(gpui::FontWeight::BOLD).child("不发送"),
                                    )
                                    .into_any_element(),
                            ))
                            .child(step(
                                "4",
                                true,
                                "你检查、可以改，按回车才发给 Agent".into_any_element(),
                            )),
                    ),
            )
            .child(
                div()
                    .px(px(T::S16))
                    .pt(px(T::S12))
                    .pb(px(T::S14))
                    .flex()
                    .flex_col()
                    .gap(px(T::S8))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .text_size(px(T::SMALL))
                            .text_color(T::color(T::secondary()))
                            .child("放进输入框的内容")
                            .child(div().flex_1())
                            .child(
                                button("form-copy", "复制")
                                    .h(px(T::ICON_WIDTH))
                                    .px(px(T::S6))
                                    .text_color(T::color(T::link()))
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            copy.clone(),
                                        ))
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .p(px(T::S10))
                            .rounded(px(T::CONTROL_RADIUS))
                            .border(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::border()))
                            .bg(T::color(T::app()))
                            .font(T::mono(cx))
                            .text_size(px(T::SMALL))
                            .text_color(T::color(T::body()))
                            .flex()
                            .gap(px(T::S6))
                            .child(div().text_color(T::color(T::muted())).child("›"))
                            .child(div().flex_1().min_w_0().child(text)),
                    ),
            )
            .child(
                div()
                    .px(px(T::S16))
                    .py(px(T::S10))
                    .flex()
                    .flex_wrap()
                    .gap(px(T::S12))
                    .border_t(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .child(hint("不是 shell 参数，引号和换行原样保留"))
                    .child(hint("同一待办再点「开始」会切回这个画面")),
            )
    }
}
