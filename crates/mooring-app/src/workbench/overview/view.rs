//! 鸟瞰's page: filters, the distribution and every group.
use super::*;

impl Workbench {
    pub(in crate::workbench) fn overview_view(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        // The board is 鸟瞰; the list stays for closed terminals and the ended or hidden.
        if !self.overview.closed_only && !self.overview.show_closed && !self.overview.show_hidden {
            return self.board_view(window, cx);
        }
        let open: Vec<_> = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| {
                s.outcome.needs_review(now())
                    && self.session_hidden(s).is_none()
                    && self.matches_filter(s)
            })
            .collect();
        let closed_only = self.overview.closed_only;
        // Search sits in the top bar and the way back is the same 「鸟瞰全部」 toggle, the
        // tab strip or Esc, so the header keeps only what this page is.
        let header = div()
            .flex()
            .flex_wrap()
            .items_end()
            .gap(px(T::S16))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(T::S4))
                    .child(
                        div()
                            .text_size(px(T::TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::primary()))
                            .child(if closed_only {
                                "已关闭的会话"
                            } else if self.overview.welcome {
                                "欢迎回来"
                            } else {
                                "鸟瞰全部"
                            }),
                    )
                    .child(label(if closed_only {
                        let quiet = open
                            .iter()
                            .filter(|s| !closed_recently(&self.workspace, s))
                            .count();
                        format!(
                            "{} 个 · 终端已关，对话还在，点卡片在新画面里接着聊{} · Esc 回到工作区",
                            open.len(),
                            if quiet > 0 {
                                format!(" · 其中 {quiet} 个很久没动，左栏不再计数")
                            } else {
                                String::new()
                            }
                        )
                    } else {
                        format!(
                            "{} 个会话还没收尾 · 按要你做什么分好了组 · Esc 回到工作区",
                            open.len()
                        )
                    })),
            )
            .child(div().flex_1())
            .child(self.navigation_filters(cx));
        let mut before: Vec<gpui::AnyElement> = vec![header.into_any_element()];
        // Closed ones pile up: walk them one at a time instead of card menus.
        if closed_only && !open.is_empty() {
            let queue: Vec<_> = open.iter().map(|s| s.id.clone()).collect();
            before.push(
                div()
                    .flex()
                    .child(solid(button("cleanup-closed", "逐个清理 →")).on_click(
                        cx.listener(move |this, _, _, cx| this.start_cleanup(queue.clone(), cx)),
                    ))
                    .into_any_element(),
            );
        }
        if let Some(error) = self.error.clone() {
            before.push(
                div()
                    .text_color(Tone::Danger.ink())
                    .child(error)
                    .into_any_element(),
            );
        }
        // The sidebar stays beside the overview.
        let available = f32::from(window.viewport_size().width) - T::SIDEBAR - T::OVERVIEW_X * 2.;
        let card_width = |min: f32| {
            let columns = ((available + T::S10) / (min + T::S10)).floor().max(1.);
            ((available - T::S10 * (columns - 1.)) / columns).floor()
        };
        let mut middle: Vec<gpui::AnyElement> = vec![];
        if !closed_only {
            middle.push(
                self.todo_section(card_width(T::TODO_CARD_MIN), cx)
                    .into_any_element(),
            );
        }
        let at_now = now();
        let grouped: Vec<(Status, Vec<&AgentSession>)> = SECTIONS
            .into_iter()
            .map(|g| {
                (
                    g,
                    open.iter()
                        .copied()
                        .filter(|s| section(status_of(&self.workspace, s)) == g)
                        .collect(),
                )
            })
            .collect();
        // Empty groups get no section: their pill above already reads 0.
        let shown: Vec<_> = grouped.iter().filter(|(_, s)| !s.is_empty()).collect();
        let first_group = before.len() + 1 + middle.len();
        let place: Vec<(Status, usize)> = shown
            .iter()
            .enumerate()
            .map(|(i, (g, _))| (*g, first_group + i))
            .collect();
        let width = card_width(T::CARD_MIN);
        let sections: Vec<gpui::AnyElement> = shown
            .iter()
            .map(|(group, sessions)| {
                self.group_section(*group, sessions, width, at_now, cx)
                    .into_any_element()
            })
            .collect();
        let dist = self.distribution(&grouped, &place, cx);
        let mut view = div()
            .id("overview-scroll")
            .track_scroll(&self.overview.scroll)
            .size_full()
            .overflow_y_scroll()
            .font_features(super::diagnostics::tabular())
            .flex()
            .flex_col()
            .gap(px(T::S20))
            .px(px(T::OVERVIEW_X))
            .py(px(T::S20))
            .children(before)
            .child(dist)
            .children(middle)
            .children(sections);
        if closed_only {
            return view.when(open.is_empty(), |d| {
                d.child(label(
                    "这里没有关掉的会话。关掉画面后，它的对话会留在这里，随时能接着聊。",
                ))
            });
        }
        let completed = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| s.outcome == Outcome::Completed && self.session_hidden(s).is_none())
            .count();
        let abandoned = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| s.outcome == Outcome::Abandoned && self.session_hidden(s).is_none())
            .count();
        let deferred = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| {
                matches!(s.outcome, Outcome::Deferred { .. }) && self.session_hidden(s).is_none()
            })
            .count();
        view = view.child(
            div()
                .border_t(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .pt(px(T::S16))
                .child(
                    button(
                        "closed-toggle",
                        format!(
                            "稍后再看 {deferred} · 已完成 {completed} · 已放弃 {abandoned} · {}",
                            if self.overview.show_closed {
                                "收起"
                            } else {
                                "展开"
                            }
                        ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.overview.show_closed = !this.overview.show_closed;
                        cx.notify();
                    })),
                ),
        );
        if self.overview.show_closed {
            for s in self
                .workspace
                .agent_sessions
                .iter()
                .filter(|s| !s.outcome.needs_review(now()) && self.session_hidden(s).is_none())
            {
                let sid = s.id.clone();
                view = view.child(
                    div()
                        .p(px(T::S8))
                        .child(
                            button(
                                gpui::SharedString::from(format!("closed-{sid}")),
                                format!(
                                    "{} · {}",
                                    self.session_title(s),
                                    outcome_label(&s.outcome)
                                ),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.overview.here = Some(sid.clone());
                                    cx.notify();
                                },
                            )),
                        )
                        .when(self.overview.here.as_ref() == Some(&s.id), |d| {
                            d.child(self.here_card(s, false, cx))
                        }),
                );
            }
        }
        let hidden: Vec<_> = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| self.session_hidden(s).is_some())
            .collect();
        view = view.child(
            button(
                "toggle-hidden",
                format!(
                    "已隐藏 {} 个自动化会话 · {}",
                    hidden.len(),
                    if self.overview.show_hidden {
                        "收起"
                    } else {
                        "展开查看"
                    }
                ),
            )
            .border_0()
            .justify_start()
            .text_color(T::color(T::secondary()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.overview.show_hidden = !this.overview.show_hidden;
                cx.notify();
            })),
        );
        if self.overview.show_hidden {
            for s in hidden {
                let sid = s.id.clone();
                view = view.child(
                    div()
                        .p(px(T::S8))
                        .border_b(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::border()))
                        .child(
                            button(
                                gpui::SharedString::from(format!("hidden-{}", s.id)),
                                self.session_title(s),
                            )
                            .border_0()
                            .justify_start()
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.overview.here = Some(sid.clone());
                                    cx.notify();
                                },
                            )),
                        )
                        .child(label(format!(
                            "{} · {} · {} · {}",
                            self.session_hidden(s).unwrap_or(""),
                            s.runtime.executable(),
                            s.cli_session_id,
                            s.cwd
                        )))
                        .when(self.overview.here.as_ref() == Some(&s.id), |d| {
                            d.child(self.here_card(s, false, cx))
                        }),
                );
            }
        }
        view
    }
}
