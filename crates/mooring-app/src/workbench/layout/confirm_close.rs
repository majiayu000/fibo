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
        let what = match closing {
            Closing::Tab(_) => "关闭这个标签",
            Closing::Pane(_) => "关闭这个画面",
            Closing::Host => "重启后台",
            Closing::Agent(_) => "停止这个 Agent",
        };
        let verb = if host {
            "重启"
        } else if agent {
            "停止"
        } else {
            "关闭"
        };
        let shells = if host {
            self.running_terminals().len().saturating_sub(agents.len())
        } else {
            0
        };
        let splits = splits_closed(&self.workspace, closing);
        let lines = agents
            .iter()
            .map(|(title, phase)| match phase {
                Some(Phase::Working) => format!("「{title}」还在工作，{verb}会打断这一轮。"),
                Some(Phase::Waiting) => format!("「{title}」在等你回答，{verb}后它会退出。"),
                _ => format!("「{title}」这一轮已经结束，{verb}后它会退出。"),
            })
            .chain((shells > 0).then(|| format!("另有 {shells} 个普通终端会停止。")));
        let lines = (splits > 0)
            .then(|| format!("这个标签有 {splits} 个分屏，会一起关闭。"))
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
                            .child(if host {
                                "重启后台，换上新版本？".to_owned()
                            } else if agent {
                                format!("{what}？")
                            } else {
                                format!("{}{what}？", if working { "停止并" } else { "" })
                            }),
                    )
                    .when(host, |d| {
                        d.child(label(
                            "后台还在跑旧版本，新改动要重启后台才生效。重启会停止所有终端，Mooring 会自动重新打开。",
                        ))
                    })
                    .child(label(lines))
                    .child(label(if host {
                        "对话记录会保留，重启后可以原地重开终端，或从左栏「已关闭」里接着聊。"
                    } else if agent {
                        "终端会留着，回到命令行。对话记录会保留，可以从左栏底部「已关闭」里接着聊。"
                    } else {
                        "对话记录会保留，可以从左栏底部「已关闭」里接着聊。"
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(T::S8))
                            .child(button("confirm-close-cancel", "取消").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.layout.confirm_close = None;
                                    cx.notify();
                                },
                            )))
                            .child(
                                button(
                                    "confirm-close-stop",
                                    if host {
                                        "重启后台"
                                    } else if agent {
                                        "停止"
                                    } else if working {
                                        "停止并关闭"
                                    } else {
                                        "关闭"
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
