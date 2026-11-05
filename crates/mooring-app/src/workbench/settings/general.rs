//! 设置 › 通用: the default Agent, tidying old sessions and the sidebar.
use super::*;

impl SettingsView {
    pub(super) fn general_page(&self, cx: &Context<Self>) -> gpui::Div {
        let mut start = Vec::new();
        if self.shows(Page::General, "默认 Agent", cx) {
            let mut choices = segments();
            for agent in Agent::all() {
                choices = choices.child(
                    segment(
                        format!("agent-{}", agent.name()),
                        agent.name(),
                        self.values.default_agent == agent,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change(cx, |v| v.default_agent = agent)
                    })),
                );
            }
            start.push(
                self.row(
                    start.is_empty(),
                    "默认 Agent",
                    "新建待办和未指定 Agent 时用它",
                    choices,
                )
                .into_any_element(),
            );
        }
        let mut tidy = Vec::new();
        for (away, title, note, options, selected) in [
            (
                true,
                "回来时显示总览",
                "离开窗口超过这段时间，回来先看各会话进展",
                &[5, 15, 30, 60][..],
                self.values.away_minutes,
            ),
            (
                false,
                "旧会话收起时间",
                "已关闭的会话过了这段时间从侧栏收起，历史仍保留",
                &[1, 3, 7, 14][..],
                self.values.stale_days,
            ),
        ] {
            if !self.shows(Page::General, title, cx) {
                continue;
            }
            // Show an existing custom value even when it is not one of the presets.
            let mut numbers = options.to_vec();
            if !numbers.contains(&selected) {
                numbers.push(selected);
                numbers.sort_unstable();
            }
            let mut choices = segments();
            for value in numbers {
                let text = format!("{value} {}", if away { "分钟" } else { "天" });
                choices = choices.child(
                    segment(format!("{away}-{value}"), text, value == selected).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.change(cx, |v| {
                                if away {
                                    v.away_minutes = value;
                                } else {
                                    v.stale_days = value;
                                }
                            })
                        }),
                    ),
                );
            }
            tidy.push(
                self.row(tidy.is_empty(), title, note, choices)
                    .into_any_element(),
            );
        }
        let mut sidebar = Vec::new();
        if self.shows(Page::General, "偷看浮层", cx) {
            let on = crate::ui::peek::enabled(cx);
            let toggle = switch("peek-toggle", on).on_click(cx.listener(move |this, _, _, cx| {
                this.text_error = crate::ui::peek::choose(!on, cx).err();
                cx.notify();
            }));
            sidebar.push(
                self.row(
                    true,
                    "偷看浮层",
                    "左栏点开不在当前标签的会话时，先浮出它的实时画面；关掉则直接切过去",
                    toggle,
                )
                .into_any_element(),
            );
        }
        self.cards(vec![("启动", start), ("会话整理", tidy), ("侧栏", sidebar)])
    }
}
