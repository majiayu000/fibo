//! Asking before a close stops running Agents.
use super::*;

impl Workbench {
    /// Asks before a close stops running Agents or takes several splits with it; a click
    /// outside cancels.
    pub(in crate::workbench) fn confirm_close_view(
        &self,
        closing: &Closing,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let agents = self.running_agents(closing);
        let working = agents.iter().any(|(_, p)| *p == Some(Phase::Working));
        let host = matches!(closing, Closing::Host);
        let agent = matches!(closing, Closing::Agent(_));
        let unmatched = matches!(closing, Closing::Unmatched(_));
        // Whole questions: English cannot glue 停止并 in front of the action.
        let question = match (closing, working) {
            (Closing::Host, _) => t!("重启后台，换上新版本？"),
            (Closing::Agent(_), _) => t!("停止这个 Agent？"),
            (Closing::Tab(_), false) => t!("关闭这个标签？"),
            (Closing::Tab(_), true) => t!("停止并关闭这个标签？"),
            (Closing::Pane(_), false) => t!("关闭这个画面？"),
            (Closing::Pane(_), true) => t!("停止并关闭这个画面？"),
            (Closing::Unmatched(_), false) => t!("只关掉这个画面？"),
            (Closing::Unmatched(_), true) => t!("停止并只关掉这个画面？"),
        };
        // One whole sentence per action and phase: the verb's form differs in English.
        let say = |title: &str, phase: &Option<Phase>| {
            let working = *phase == Some(Phase::Working);
            let waiting = *phase == Some(Phase::Waiting);
            match (host, agent, working, waiting) {
                (true, _, true, _) => t!("「{title}」还在工作，重启会打断这一轮。", title),
                (true, _, _, true) => t!(
                    "「{title}」在{state}，重启后它会退出。",
                    title,
                    state = Status::Waiting.label()
                ),
                (true, ..) => t!("「{title}」这一轮已经结束，重启后它会退出。", title),
                (_, true, true, _) => t!("「{title}」还在工作，停止会打断这一轮。", title),
                (_, true, _, true) => t!(
                    "「{title}」在{state}，停止后它会退出。",
                    title,
                    state = Status::Waiting.label()
                ),
                (_, true, ..) => t!("「{title}」这一轮已经结束，停止后它会退出。", title),
                (.., true, _) => t!("「{title}」还在工作，关闭会打断这一轮。", title),
                (.., true) => t!(
                    "「{title}」在{state}，关闭后它会退出。",
                    title,
                    state = Status::Waiting.label()
                ),
                _ => t!("「{title}」这一轮已经结束，关闭后它会退出。", title),
            }
        };
        let shells = if host {
            self.running_terminals().len().saturating_sub(agents.len())
        } else {
            0
        };
        let splits = splits_closed(&self.workspace, closing);
        let lines = agents
            .iter()
            .map(|(title, phase)| say(title, phase))
            .chain((shells > 0).then(|| t!("另有 {shells} 个普通终端会停止。", shells)));
        let lines = (splits > 0)
            .then(|| t!("这个标签有 {splits} 个分屏，会一起关闭。", splits))
            .into_iter()
            .chain(lines)
            .collect::<Vec<_>>()
            .join("\n");
        let confirmed = closing.clone();
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.layout.confirm_close = None;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("confirm-close")
                    .w(px(T::HERE_WIDTH))
                    .p(px(T::S16))
                    .flex()
                    .flex_col()
                    .gap(px(T::S12))
                    .bg(T::color(T::raised()))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::control()))
                    .rounded(px(T::CARD_RADIUS))
                    .shadow_lg()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(T::CARD_TITLE))
                            .child(question),
                    )
                    .when(host, |d| {
                        d.child(label(
                            t!("后台还在跑旧版本，新改动要重启后台才生效。重启会停止所有终端，Fibo 会自动重新打开。"),
                        ))
                    })
                    .when(unmatched, |d| {
                        d.child(label(
                            t!("没认出这个窗格是哪条对话，所以不会记成完成。"),
                        ))
                    })
                    .when(!lines.is_empty(), |d| d.child(label(lines)))
                    .child(label(if unmatched {
                        t!("里面的 Agent 会退出，正在做的事也会停下。对话记录会保留，可以从左栏底部「已关闭」里接着聊。")
                    } else if host {
                        t!("对话记录会保留，重启后可以原地重开终端，或从左栏「已关闭」里接着聊。")
                    } else if agent {
                        t!("终端会留着，回到命令行。对话记录会保留，可以从左栏底部「已关闭」里接着聊。")
                    } else {
                        t!("对话记录会保留，可以从左栏底部「已关闭」里接着聊。")
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(T::S8))
                            .child(button("confirm-close-cancel", t!("取消")).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.layout.confirm_close = None;
                                    cx.notify();
                                },
                            )))
                            .child(
                                button(
                                    "confirm-close-stop",
                                    if unmatched {
                                        t!("只关掉画面")
                                    } else if host {
                                        t!("重启后台")
                                    } else if agent {
                                        t!("停止")
                                    } else if working {
                                        t!("停止并关闭")
                                    } else {
                                        t!("关闭")
                                    },
                                )
                                .when(working || host || agent, |b| {
                                    b.text_color(Tone::Danger.ink())
                                })
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.close_now(confirmed.clone(), cx);
                                    },
                                )),
                            ),
                    ),
            )
            .into_any_element()
    }
}
