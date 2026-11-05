//! The board's title row: what is open, the per-Agent filter chips and the links.
use super::*;

impl Workbench {
    pub(super) fn board_header(
        &self,
        open: usize,
        stale: usize,
        empty: bool,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let mut agents: Vec<(Agent, usize)> = vec![];
        for s in self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| self.session_hidden(s).is_none() && s.outcome.needs_review(now()))
        {
            match agents.iter_mut().find(|(a, _)| *a == s.runtime) {
                Some((_, n)) => *n += 1,
                None => agents.push((s.runtime, 1)),
            }
        }
        agents.sort_by_key(|(a, _)| a.name());
        let chips = agents.into_iter().map(|(agent, n)| {
            let on = self.overview.agent == Some(agent);
            let dim = self.overview.agent.is_some() && !on;
            div()
                .id(gpui::SharedString::from(format!(
                    "agent-filter-{}",
                    agent.key()
                )))
                .role(gpui::accesskit::Role::Button)
                .aria_label(gpui::SharedString::from(format!("只看 {}", agent.name())))
                .h(px(T::SEGMENT_HEIGHT))
                .pl(px(T::S6))
                .pr(px(T::S8))
                .flex()
                .items_center()
                .gap(px(T::S6))
                .rounded(px(T::SEGMENT_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(if on { T::control() } else { T::card_border() }))
                .when(on, |d| d.bg(T::color(T::raised())))
                .when(dim, |d| d.opacity(0.5))
                .text_size(px(T::SMALL))
                .text_color(T::color(if on { T::primary() } else { T::body() }))
                .cursor_pointer()
                .hover(|s| s.border_color(T::color(T::control())))
                .child(agent_mark(agent, T::color(T::secondary())))
                .child(agent.name())
                .child(div().text_color(T::color(T::muted())).child(n.to_string()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.overview.agent = if this.overview.agent == Some(agent) {
                        None
                    } else {
                        Some(agent)
                    };
                    cx.notify();
                }))
        });
        let link = |id: &'static str, text: String| {
            button(id, text)
                .h(px(T::SEGMENT_HEIGHT))
                .px(px(T::S8))
                .text_color(T::color(T::secondary()))
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(T::S10))
            .px(px(T::OVERVIEW_X))
            .pt(px(T::S20))
            .pb(px(T::S14))
            .child(
                div()
                    .text_size(px(T::TITLE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(T::primary()))
                    .child(if self.overview.welcome {
                        "欢迎回来"
                    } else {
                        "鸟瞰全部"
                    }),
            )
            .child(
                // An empty board says what lands in these columns, not just 「0 个」.
                label(if empty {
                    "还没有会话：在终端里运行 claude 或 codex，它们会按进度出现在下面几列 · Esc 回到工作区".to_owned()
                } else {
                    format!("{open} 个没收尾 · Esc 回到工作区")
                })
                .text_color(T::color(T::muted())),
            )
            .child(div().w(px(T::S8)))
            .children(chips)
            .child(div().flex_1())
            .when(stale > 0 || self.overview.show_stale, |d| {
                d.child(
                    link(
                        "board-stale",
                        if self.overview.show_stale {
                            format!("收起很久没动 {stale}")
                        } else {
                            format!("很久没动 {stale}")
                        },
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.overview.show_stale = !this.overview.show_stale;
                        cx.notify();
                    })),
                )
            })
            .child(
                link("board-closed", "稍后再看与已结束".into()).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.overview.show_closed = true;
                        cx.notify();
                    },
                )),
            )
            .child(self.navigation_filters(cx))
    }
}
