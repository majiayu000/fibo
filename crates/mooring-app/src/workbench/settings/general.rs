//! 设置 › 通用: the default Agent, the Agents' hooks (`hooks.rs`), tidying old sessions and
//! the sidebar.
use super::*;

impl SettingsView {
    pub(super) fn general_page(&self, cx: &Context<Self>) -> gpui::Div {
        let mut start = Vec::new();
        if self.shows(Page::General, t!("默认 Agent"), cx) {
            start.push(
                self.row(
                    true,
                    t!("默认 Agent"),
                    t!("新建待办和未指定 Agent 时用它"),
                    self.default_agent_picker(cx),
                )
                .into_any_element(),
            );
        }
        if self.shows(Page::General, t!("语言"), cx) {
            start.push(self.language_row(start.is_empty(), cx).into_any_element());
        }
        let mut tidy = Vec::new();
        for (away, title, note, options, selected) in [
            (
                true,
                t!("回来时显示总览"),
                t!("离开窗口超过这段时间，回来先看各会话进展"),
                &[5, 15, 30, 60][..],
                self.values.away_minutes,
            ),
            (
                false,
                t!("旧会话收起时间"),
                t!("已关闭的会话过了这段时间从侧栏收起，历史仍保留"),
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
                let text = if away {
                    t!("{value} 分钟", value)
                } else {
                    t!("{value} 天", value)
                };
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
        if self.shows(Page::General, t!("偷看浮层"), cx) {
            let on = crate::ui::peek::enabled(cx);
            let toggle = switch("peek-toggle", on).on_click(cx.listener(move |this, _, _, cx| {
                this.text_error = crate::ui::peek::choose(!on, cx).err();
                cx.notify();
            }));
            sidebar.push(
                self.row(
                    true,
                    t!("偷看浮层"),
                    t!("左栏点开不在当前标签的会话时，先浮出它的实时画面；关掉则直接切过去"),
                    toggle,
                )
                .into_any_element(),
            );
        }
        self.cards(vec![
            (t!("启动"), start),
            ("Agent Hook", self.hook_card(cx)),
            (t!("会话整理"), tidy),
            (t!("侧栏"), sidebar),
        ])
    }
    fn default_agent_picker(&self, cx: &Context<Self>) -> gpui::Div {
        div()
            .relative()
            .child(
                secondary("default-agent", self.values.default_agent.name())
                    .w(px(T::SETTINGS_AGENT_WIDTH))
                    .justify_between()
                    .child("▾")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.agent_menu = if this.agent_menu.is_some() {
                            None
                        } else {
                            Some(this.values.default_agent)
                        };
                        if let Some(agent) = this.agent_menu
                            && let Some(ix) = Agent::all().iter().position(|a| *a == agent)
                        {
                            this.agent_scroll.scroll_to_item(ix);
                        }
                        window.focus(&this.focus, cx);
                        cx.notify();
                    })),
            )
            .when_some(self.agent_menu, |d, highlighted| {
                d.child(
                    gpui::deferred(
                        gpui::anchored()
                            .position_mode(gpui::AnchoredPositionMode::Local)
                            .position(gpui::point(px(0.), px(T::S4)))
                            .snap_to_window_with_margin(px(T::S8))
                            .child(
                                div()
                                    .when(self.agent_scroll.bounds_for_item(0).is_none(), |d| {
                                        let scroll = self.agent_scroll.clone();
                                        let view = cx.entity_id();
                                        let ix = Agent::all()
                                            .iter()
                                            .position(|a| *a == highlighted)
                                            .unwrap_or(0);
                                        // A new ScrollHandle has no viewport until prepaint.
                                        // Reveal the saved choice once that first layout exists.
                                        d.on_children_prepainted(move |_, window, _| {
                                            scroll.scroll_to_item(ix);
                                            window.on_next_frame(move |_, cx| cx.notify(view));
                                        })
                                    })
                                    .id("default-agent-menu")
                                    .track_scroll(&self.agent_scroll)
                                    .w(px(T::SETTINGS_AGENT_WIDTH))
                                    .max_h(px(T::SETTINGS_AGENT_MENU_HEIGHT))
                                    .overflow_y_scroll()
                                    .p(px(T::S4))
                                    .bg(T::color(T::popover()))
                                    .border(px(T::BORDER_WIDTH))
                                    .border_color(T::color(T::control()))
                                    .rounded(px(T::CONTROL_RADIUS))
                                    .shadow_lg()
                                    .occlude()
                                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                        this.agent_menu = None;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }))
                                    .children(Agent::all().into_iter().map(|agent| {
                                        button(format!("agent-{}", agent.name()), agent.name())
                                            .w_full()
                                            .justify_between()
                                            .when(agent == highlighted, |d| {
                                                d.bg(T::color(T::button_hover()))
                                            })
                                            .child(if agent == self.values.default_agent {
                                                "✓"
                                            } else {
                                                ""
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.agent_menu = None;
                                                this.change(cx, |v| v.default_agent = agent);
                                                cx.notify();
                                            }))
                                    })),
                            ),
                    )
                    .with_priority(3),
                )
            })
    }

    /// Keep menu keys inside settings; Esc dismisses only the open picker.
    pub(super) fn agent_menu_key(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(highlighted) = self.agent_menu else {
            return;
        };
        match key {
            "escape" => self.agent_menu = None,
            "enter" | "space" => {
                self.agent_menu = None;
                self.change(cx, |v| v.default_agent = highlighted);
            }
            "up" | "down" => {
                let agents = Agent::all();
                let i = agents.iter().position(|a| *a == highlighted).unwrap_or(0);
                let next = if key == "down" {
                    (i + 1) % agents.len()
                } else {
                    (i + agents.len() - 1) % agents.len()
                };
                self.agent_menu = Some(agents[next]);
                self.agent_scroll.scroll_to_item(next);
            }
            _ => {}
        }
        cx.notify();
    }

    /// 语言: follow the system, or Chinese or English whatever it says. Each language is named
    /// in itself, so a reader who cannot read the other one still finds theirs.
    fn language_row(&self, first: bool, cx: &Context<Self>) -> gpui::Div {
        use mooring_core::i18n::Choice;
        let chosen = crate::ui::language::saved();
        let mut choices = segments();
        for choice in Choice::ALL {
            let text = match choice {
                Choice::Auto => t!("跟随系统"),
                Choice::Zh => not_ui!("中文"),
                Choice::En => "English",
            };
            choices = choices.child(
                segment(format!("lang-{}", choice.key()), text, choice == chosen).on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.text_error = crate::ui::language::choose(choice).err();
                        window.refresh();
                        cx.refresh_windows();
                        cx.notify();
                    }),
                ),
            );
        }
        self.row(
            first,
            t!("语言"),
            t!("跟随系统时，中文系统用中文，其他语言用英文"),
            choices,
        )
    }
}
