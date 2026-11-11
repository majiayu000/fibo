//! The 「你在这里」 card.
use super::super::overview::{quiet, solid};
use super::*;

impl Workbench {
    /// `in_pane`: the card sits on the session's own pane, not in the overview. Head
    /// (title, status, this turn), the facts that scroll, and the two decisions at the foot
    /// with the rest behind 更多 (Keepline repo, specs/terminal-workbench/design/palette-here.html).
    pub(in crate::workbench) fn here_card(
        &self,
        session: &AgentSession,
        in_pane: bool,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let card = self
            .workspace
            .content
            .get(&session.id)
            .cloned()
            .unwrap_or_default();
        let h = card.here();
        let save = session.clone();
        let (group, _) = session_status(&self.workspace, &self.attention.unseen, session);
        let mut meta = vec![session.runtime.executable().to_owned()];
        if card.turn_started > 0 && card.updated > card.turn_started {
            meta.push(t!(
                "本轮 {v1}",
                v1 = span_label(card.updated - card.turn_started)
            ));
        }
        meta.push(ago(self.session_updated(session), now()));
        let head = div()
            .flex_none()
            .px(px(T::S16))
            .pt(px(T::S14))
            .flex()
            .flex_col()
            .gap(px(T::S8))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(T::META))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::muted()))
                            .child(t!("你在这里")),
                    )
                    .child(div().flex_1())
                    .child(
                        button("dismiss-here", "×")
                            .h(px(T::PANE_ACTION))
                            .px(px(T::S6))
                            .text_color(T::color(T::muted()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.overview.here = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(T::CARD_TITLE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(T::primary()))
                    .line_clamp(3)
                    .text_ellipsis()
                    .child(self.session_title(session)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(T::S6))
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .font_features(T::tabular())
                    .child(chip(group))
                    .child(meta.join(" · ")),
            );
        // Only the body scrolls: the head and the decisions stay put.
        let mut panel = div()
            .id(gpui::SharedString::from(format!(
                "here-body-{}",
                session.id
            )))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(T::S16))
            .py(px(T::S12))
            .flex()
            .flex_col()
            .gap(px(T::S12));
        let fact = |name: &'static str, value: gpui::Div| {
            div()
                .flex()
                .gap(px(T::S10))
                .child(
                    div()
                        .w(px(T::HERE_FACT))
                        .flex_none()
                        .text_color(T::color(T::muted()))
                        .child(name),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(T::color(T::body()))
                        .child(value),
                )
        };
        // Each later ask on its own line, not run together with 「追加：」.
        let sep = mooring_core::i18n::say(not_ui!("\n追加："));
        let parts: Vec<&str> = if h.ask.contains(sep) {
            h.ask.split(sep).collect()
        } else {
            h.ask.split(not_ui!("\n追加：")).collect()
        };
        let mut asks = parts.into_iter();
        let mut ask = div()
            .flex()
            .flex_col()
            .gap(px(T::S2))
            .child(asks.next().unwrap_or_default().to_owned());
        for more in asks {
            ask = ask.child(
                div()
                    .text_color(T::color(T::secondary()))
                    .child(t!("追加 {more}", more)),
            );
        }
        // Only fields that say something: no evidence yet is not news, and the guessed
        // next step was the same few template lines for every session.
        let done = done_items(&card.evidence, &session.cwd);
        let mut facts = div()
            .flex()
            .flex_col()
            .gap(px(T::S8))
            .text_size(px(T::SMALL))
            .child(fact(t!("你要它"), ask));
        if !done.is_empty() {
            let mut list = div().flex().flex_col().gap(px(T::S2));
            for (text, times, file) in done {
                list = list.child(
                    div()
                        .when(file, |d| {
                            d.font_family(T::mono(cx).family)
                                .text_size(px(T::META))
                                .text_color(T::color(T::primary()))
                        })
                        .child(text)
                        .when(times > 1, |d| {
                            d.child(
                                div()
                                    .text_size(px(T::CHIP_SIZE))
                                    .text_color(T::color(T::muted()))
                                    .child(t!("改了 {times} 次", times)),
                            )
                        }),
                );
            }
            facts = facts.child(fact(t!("做成了"), list));
        }
        facts = facts.child(fact(
            t!("停在"),
            div().line_clamp(4).text_ellipsis().child(h.stopped),
        ));
        panel = panel.child(facts);
        let (ok, files, failed) = evidence_tally(&card.evidence);
        if ok + files + failed > 0 {
            let total = (ok + files + failed) as f32;
            let failure = Tone::Danger.dot();
            let mut bar = div()
                .flex()
                .gap(px(T::S2))
                .h(px(T::EVIDENCE_BAR))
                .overflow_hidden();
            for (n, color) in [
                (ok, T::color(T::secondary())),
                (files, T::color(T::primary())),
                (failed, failure),
            ] {
                if n > 0 {
                    bar = bar.child(
                        div()
                            .h_full()
                            .w(gpui::relative(n as f32 / total))
                            .flex_shrink(1.)
                            .rounded(px(T::EVIDENCE_CELL_RADIUS))
                            .bg(color),
                    );
                }
            }
            let key = |name: &'static str, n: usize, color: gpui::Hsla| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S4))
                    .child(
                        div()
                            .w(px(T::EVIDENCE_CELL_WIDTH))
                            .h(px(T::EVIDENCE_CELL_HEIGHT))
                            .rounded(px(T::EVIDENCE_CELL_RADIUS))
                            .bg(color),
                    )
                    .child(name)
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::primary()))
                            .child(n.to_string()),
                    )
            };
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(T::S6))
                    .text_size(px(T::META))
                    .font_features(T::tabular())
                    .child(
                        div()
                            .flex()
                            .text_color(T::color(T::muted()))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(T::color(T::secondary()))
                                    .child(t!("留下的证据")),
                            )
                            .child(div().flex_1())
                            .child(t!("最近 {len} 条", len = card.evidence.len())),
                    )
                    .child(bar)
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(T::S12))
                            .text_color(T::color(T::secondary()))
                            .child(key(t!("命令成功"), ok, T::color(T::secondary())))
                            .child(key(t!("改文件"), files, T::color(T::primary())))
                            .child(key(t!("失败"), failed, failure)),
                    ),
            );
        }
        let note = |text: String| {
            div()
                .text_size(px(T::META))
                .text_color(T::color(T::muted()))
                .child(text)
        };
        if card.partial_history {
            panel = panel.child(note(
                t!("仅覆盖已索引的历史尾段与之后的增量；没有证据不等于没做。").into(),
            ));
        }
        if let Some(binding) = session.binding.filter(|_| session.current) {
            panel = panel.child(note(t!(
                "和这个终端对上靠的是：{how}",
                how = binding.label()
            )));
        }
        if let Some(h) = self
            .workspace
            .outcome_history
            .iter()
            .rev()
            .find(|h| h.session == session.id)
        {
            panel = panel.child(note(t!(
                "{reason} · {before} → {after} · 最近一次结局变化",
                reason = &h.reason,
                before = outcome_label(&h.before),
                after = outcome_label(&h.after)
            )));
        }
        if session.current && session.outcome != Outcome::Open {
            panel = panel.child(note(
                t!("结局已记录，Agent 仍在运行；若要停止，请在画面菜单选择停止 Agent。").into(),
            ));
        }
        let resume = session.clone();
        let command = self.workspace.resume_command(session);
        // Two decisions stay in sight: go back to it, and settle it. The rest waits in 更多.
        // On its own live pane there is nothing to go back to: you type in the terminal,
        // and settling it is the card's one solid button.
        let typing_here = in_pane && live_terminal(&self.workspace, session).is_some();
        let (settle_id, settle_name, settle) = if session.outcome == Outcome::Open {
            ("here-complete", t!("标记完成"), Outcome::Completed)
        } else {
            ("here-reopen", t!("撤销结局"), Outcome::Open)
        };
        let settle_session = session.clone();
        let more = self.overview.here_more.as_ref() == Some(&session.id);
        let more_id = session.id.clone();
        let settle_target = settle.clone();
        let settle_button =
            button(settle_id, settle_name).on_click(cx.listener(move |this, _, _, cx| {
                this.settle_session(&settle_session, settle_target.clone(), in_pane, cx)
            }));
        let mut foot = div()
            .flex_none()
            .relative()
            .px(px(T::S16))
            .pt(px(T::S10))
            .pb(px(T::S12))
            .flex()
            .items_center()
            .gap(px(T::S6))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .when(!typing_here, |d| {
                d.child(
                    solid(button("here-continue", t!("继续#workbench"))).on_click(cx.listener(
                        move |this, _, window, cx| this.open_session(&resume, window, cx),
                    )),
                )
            })
            .child(if typing_here {
                solid(settle_button)
            } else {
                quiet(settle_button)
            })
            .child(div().flex_1())
            .child(
                // The menu folds on 「复制恢复命令」, so the button it hangs from says so.
                button("here-more", self.copy_label("here-copy", t!("更多 ⋯")))
                    .text_color(T::color(T::link()))
                    .when(more, |d| d.bg(T::color(T::button_hover())))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.overview.here_more = (!more).then(|| more_id.clone());
                        cx.notify();
                    })),
            );
        if more {
            let item = |id: &'static str, name: &'static str| {
                button(id, "")
                    .aria_label(name)
                    .w_full()
                    .px(px(T::S8))
                    .justify_start()
                    .gap(px(T::S8))
                    .child(name)
            };
            let fold = |this: &mut Self| this.overview.here_more = None;
            let hint: String = command.chars().take(T::HERE_COMMAND_CHARS).collect();
            let mut menu = div()
                .id("here-menu")
                .absolute()
                .right(px(T::S12))
                .bottom(gpui::relative(1.))
                .w(px(T::MENU_WIDTH))
                .p(px(T::S4))
                .flex()
                .flex_col()
                .gap(px(T::BORDER_WIDTH))
                .rounded(px(T::CARD_RADIUS))
                .bg(T::color(T::raised()))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(T::control()))
                .shadow_lg()
                .child(
                    item("here-copy", t!("复制命令"))
                        .child(div().flex_1())
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(T::CHIP_SIZE))
                                .text_color(T::color(T::muted()))
                                .font_family(T::mono(cx).family)
                                .child(if hint.len() < command.len() {
                                    format!("{hint}…")
                                } else {
                                    hint
                                }),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            fold(this);
                            this.copy_text("here-copy", command.clone(), cx);
                        })),
                )
                .child(item("save-here-task", t!("存为待办")).on_click(cx.listener(
                    move |this, _, _, cx| {
                        fold(this);
                        this.save_session_task(&save, cx)
                    },
                )));
            let sid = session.id.clone();
            menu = menu.child(item("here-defer", t!("稍后再看…")).on_click(cx.listener(
                move |this, _, window, cx| {
                    fold(this);
                    this.overview.here = Some(sid.clone());
                    this.nav.overview = false;
                    this.show_form(FormKind::Defer, window, cx);
                },
            )));
            // The other endings; 放弃 sits apart below a rule.
            for (id, name, outcome) in [
                ("here-menu-complete", t!("标记完成"), Outcome::Completed),
                ("here-menu-reopen", t!("撤销结局"), Outcome::Open),
                ("here-menu-abandon", t!("放弃"), Outcome::Abandoned),
            ]
            .into_iter()
            .filter(|(_, _, o)| *o != session.outcome && *o != settle)
            {
                let settled = session.clone();
                let abandon = outcome == Outcome::Abandoned;
                if abandon {
                    menu = menu.child(
                        div()
                            .mx(px(T::S2))
                            .my(px(T::S4))
                            .h(px(T::BORDER_WIDTH))
                            .bg(T::color(T::control())),
                    );
                }
                menu = menu.child(
                    item(id, name)
                        .when(abandon, |d| d.text_color(Tone::Danger.ink()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            fold(this);
                            this.settle_session(&settled, outcome.clone(), in_pane, cx)
                        })),
                );
            }
            foot = foot.child(menu);
        }
        div()
            .id(gpui::SharedString::from(format!(
                "here-card-{}",
                session.id
            )))
            .w(px(T::HERE_WIDTH))
            .max_w_full()
            .max_h(px(T::HERE_HEIGHT))
            .rounded(px(T::POPOVER_RADIUS))
            .bg(T::color(T::popover()))
            .text_color(T::color(T::primary()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            .shadow_lg()
            .flex()
            .flex_col()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            // A click anywhere else puts it away; the defer form it opened still needs it.
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.overview.here.is_some() && this.tasks.form.is_none() {
                    this.overview.here = None;
                    cx.notify();
                }
            }))
            .child(head)
            .child(panel)
            .child(foot)
    }
}
