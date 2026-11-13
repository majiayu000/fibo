//! The status bar's sessions popover: what waits for you, then what runs.
use super::*;

impl Workbench {
    pub(super) fn sessions_popover(
        &self,
        mut panel: gpui::Stateful<gpui::Div>,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let (working, needs) = self.status_sessions();
        if working.is_empty() && needs.is_empty() {
            return panel.child(label(t!(
                "没有{label}或{needs_you_word}的会话",
                label = Status::Running.label(),
                needs_you_word = fibo_core::status::needs_you_word()
            )));
        }
        let now = now();
        for (title, group, list) in [
            (t!("等你 {len}", len = needs.len()), Status::Waiting, &needs),
            (
                format!("{} {}", Status::Running.label(), working.len()),
                Status::Running,
                &working,
            ),
        ] {
            if list.is_empty() {
                continue;
            }
            panel = panel.child(
                div()
                    .text_size(px(T::CHIP_SIZE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(group.colors().1)
                    .child(title),
            );
            for s in list.iter() {
                let g = status_of(&self.workspace, s);
                let session = (*s).clone();
                panel = panel.child(
                    div()
                        .id(gpui::SharedString::from(format!("status-session-{}", s.id)))
                        .h(px(T::ROW))
                        .px(px(T::S6))
                        .mx(px(-T::S6))
                        .flex()
                        .items_center()
                        .gap(px(T::S8))
                        .rounded(px(T::CHIP_RADIUS))
                        .cursor_pointer()
                        .hover(|d| d.bg(T::color(T::button_hover())))
                        .child(
                            div()
                                .size(px(T::DOT))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(g.colors().0),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(self.session_title(s)),
                        )
                        .child(label(format!(
                            "{} · {}",
                            s.runtime.name(),
                            ago(self.workspace.session_updated(s), now)
                        )))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.statusbar = None;
                            this.enter_card(&session, window, cx);
                        })),
                );
            }
        }
        panel
    }
}
