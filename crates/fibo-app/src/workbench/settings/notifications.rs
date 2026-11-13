//! 设置 › 通知: desktop notifications, the sound, and when either speaks up.
use super::*;

impl SettingsView {
    pub(super) fn notifications_page(&self, cx: &Context<Self>) -> gpui::Div {
        let notify = self.shows(Page::Notifications, t!("桌面通知"), cx);
        let sound = self.shows(Page::Notifications, t!("提示音"), cx);
        if !notify && !sound {
            return div();
        }
        let on = self.values.notifications;
        let mut switches = Vec::new();
        if notify {
            let toggle =
                switch("notifications-toggle", on).on_click(cx.listener(|this, _, _, cx| {
                    this.change(cx, |v| v.notifications = !v.notifications)
                }));
            switches.push(
                self.row(
                    true,
                    t!("桌面通知"),
                    t!("Agent 停下来等你或做完一轮时，发一条系统通知"),
                    toggle,
                )
                .into_any_element(),
            );
        }
        // Off until chosen, so an upgrade stays quiet; it sounds without notifications too.
        if sound {
            let toggle = switch("sound-toggle", self.values.sound)
                .on_click(cx.listener(|this, _, _, cx| this.change(cx, |v| v.sound = !v.sound)));
            switches.push(
                self.row(
                    switches.is_empty(),
                    t!("提示音"),
                    t!("Agent 等你或出错时响一声；不开桌面通知也会响"),
                    toggle,
                )
                .into_any_element(),
            );
        }
        let alerting = on || self.values.sound;
        let when = |group: Status, title: &'static str, note: &'static str, first: bool| {
            div()
                .px(px(T::S16))
                .py(px(T::S12))
                .flex()
                .items_center()
                .gap(px(T::S12))
                .when(!first, |d| {
                    d.border_t(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::border()))
                })
                .when(!alerting, |d| d.opacity(0.5))
                .child(dot(group))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(T::S2))
                        .child(
                            div()
                                .text_size(px(T::BODY_SIZE))
                                .text_color(T::color(T::primary()))
                                .child(title),
                        )
                        .child(label(note)),
                )
                .child(chip(group))
                .into_any_element()
        };
        self.cards(vec![
            (t!("开关"), switches),
            (
                t!("什么时候提醒"),
                vec![
                    when(
                        Status::Waiting,
                        t!("等你回复"),
                        t!("Agent 提问或请求批准；同一次等待只提醒一次"),
                        true,
                    ),
                    when(
                        Status::Error,
                        t!("出错停下"),
                        t!("Agent 报错中断；同一条错误只提醒一次"),
                        false,
                    ),
                    when(
                        Status::Review,
                        t!("做完一轮"),
                        t!("Agent 做完一轮请你过目；每轮只提醒一次，不响提示音"),
                        false,
                    ),
                ],
            ),
        ])
        .child(
            div()
                .pt(px(T::S12))
                .px(px(T::S4))
                .flex()
                .flex_col()
                .gap(px(T::S4))
                .child(label(t!(
                    "只提醒由 Fibo 打开的会话；你正看着的那个终端不会打扰你。"
                )))
                .child(label(t!(
                    "首次开启时 macOS 会询问权限；若已拒绝，到 系统设置 → 通知 → Fibo 重新允许。"
                ))),
        )
    }
}
