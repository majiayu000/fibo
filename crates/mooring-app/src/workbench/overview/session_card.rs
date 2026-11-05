//! One session's card on 鸟瞰.
use super::*;

impl Workbench {
    pub(super) fn session_card(
        &self,
        group: Status,
        session: &AgentSession,
        width: f32,
        now: u64,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let closed_only = self.overview.closed_only;
        let session = session.clone();
        let enter = session.clone();
        let primary = session.clone();
        let menu_id = session.id.clone();
        let meta = format!(
            "{} · {} · {}{}{}",
            if session.terminal.is_none() {
                "外部·只读"
            } else {
                "托管"
            },
            session.runtime.executable(),
            (self.workspace.session_space(&session))
                .and_then(|id| self.workspace.spaces.iter().find(|s| s.id == id))
                .map_or("外部", |s| s.name.as_str()),
            if self
                .workspace
                .outcome_history
                .iter()
                .rev()
                .find(|h| h.session == session.id)
                .is_some_and(|h| h.reason == "暂存到期")
            {
                " · 到期"
            } else {
                ""
            },
            if changed_since(&self.workspace, &session, self.overview.since) {
                " · 新"
            } else {
                ""
            }
        );
        // The last thing said, in body text; a muted lead says what it is.
        let line = if self.workspace.content.contains_key(&session.id) {
            const LEAD: &str = "最后一句  ";
            gpui::StyledText::new(format!("{LEAD}{}", self.session_line(&session))).with_highlights(
                [(
                    0..LEAD.len(),
                    gpui::HighlightStyle {
                        color: Some(T::color(T::muted())),
                        ..Default::default()
                    },
                )],
            )
        } else {
            gpui::StyledText::new(self.session_line(&session))
        };
        let primary_continue =
            closed_only || matches!(group, Status::Waiting | Status::Error | Status::Paused);
        div()
            .id(gpui::SharedString::from(format!("card-{}", session.id)))
            .role(gpui::accesskit::Role::Button)
            .aria_label(gpui::SharedString::from(format!(
                "会话：{}",
                self.session_title(&session)
            )))
            .relative()
            .w(px(width))
            .min_w(px(T::CARD_MIN))
            .min_h(px(T::CARD_MIN_HEIGHT))
            .px(px(T::S14))
            .pt(px(T::S12))
            .pb(px(T::S10))
            .rounded(px(T::CARD_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .bg(T::color(T::popover()))
            .flex()
            .flex_col()
            .gap(px(T::S6))
            .cursor_pointer()
            .hover(|s| s.border_color(T::color(T::control())))
            .on_click(cx.listener(move |this, _, window, cx| this.enter_card(&enter, window, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(T::CARD_TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::primary()))
                            .truncate()
                            .child(self.session_title(&session)),
                    )
                    .child(
                        button(
                            gpui::SharedString::from(format!("more-{}", session.id)),
                            "⋯",
                        )
                        .h(px(T::ICON_HEIGHT))
                        .px(px(T::S6))
                        .text_color(T::color(T::muted()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.overview.here = None;
                            this.overview.card_menu =
                                if this.overview.card_menu.as_ref() == Some(&menu_id) {
                                    None
                                } else {
                                    Some(menu_id.clone())
                                };
                            cx.notify();
                        })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .child(div().flex_1().min_w_0().truncate().child(meta))
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(ago(self.session_updated(&session), now)),
                    ),
            )
            .child(
                div()
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::body()))
                    .line_clamp(3)
                    .child(line),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .child(chip(group).font_weight(gpui::FontWeight::SEMIBOLD))
                    .child(div().flex_1())
                    .child(
                        button(
                            gpui::SharedString::from(format!("primary-{}", session.id)),
                            if closed_only {
                                "接着聊 →"
                            } else if primary_continue {
                                "继续 →"
                            } else {
                                "看进度"
                            },
                        )
                        .h(px(T::SEGMENT_HEIGHT))
                        .px(px(T::S8))
                        .text_color(T::color(T::link()))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.overview.card_menu = None;
                                if primary_continue {
                                    this.open_session(&primary, window, cx);
                                } else {
                                    this.overview.here = Some(primary.id.clone());
                                    cx.notify();
                                }
                            },
                        )),
                    ),
            )
            .when(self.overview.card_menu.as_ref() == Some(&session.id), |d| {
                d.child(
                    gpui::deferred(
                        self.session_actions(&session, cx)
                            .absolute()
                            .top(px(T::ROW + T::S12))
                            .right(px(T::S12))
                            .occlude(),
                    )
                    .priority(1),
                )
            })
            .when(self.overview.here.as_ref() == Some(&session.id), |d| {
                d.child(self.here_card(&session, false, cx))
            })
    }
}
