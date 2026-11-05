//! A pane: its title bar, its terminal and what is drawn over them.
use super::*;

impl Workbench {
    pub(in crate::workbench) fn pane_view(
        &self,
        id: &str,
        borders: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        if interrupted(&self.workspace, id) {
            let reopen = id.to_owned();
            let close = id.to_owned();
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(T::S12))
                .p(px(T::S20))
                .rounded(px(T::PANE_RADIUS))
                .border(px(if borders { T::BORDER_WIDTH } else { 0. }))
                .border_color(T::color(T::border()))
                .bg(T::color(T::pane()))
                .child(label("已中断（后台重启，原进程已结束）"))
                .child(
                    button(
                        gpui::SharedString::from(format!("reopen-{id}")),
                        "在这里重新开一个终端",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.reopen(reopen.clone(), cx))),
                )
                .child(
                    button(
                        gpui::SharedString::from(format!("close-interrupted-{id}")),
                        "关闭",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.close_pane(&close, cx))),
                )
                .into_any_element();
        }
        // Panes attach in sync_panes; a leaf drawn before that (a tab switch whose
        // reply has not arrived) stays blank for a frame instead of taking the app down.
        let Some(pane) = self.layout.panes.get(id) else {
            return div().size_full().into_any_element();
        };
        let focused = pane.focus_handle(cx).is_focused(window);
        let session = self.pane_session(id);
        let status = self.pane_status(id).map(|(g, _)| g);
        // A question or an error you have not looked at yet: the pane rings for it.
        let asking = self
            .pane_status(id)
            .filter(|(g, unseen)| *unseen && matches!(g, Status::Waiting | Status::Error))
            .map(|(g, _)| g);
        // The conversation's title names the pane, like its tab; its latest reply is
        // already on screen below, so that line only shows on hover. Without an Agent,
        // the title the program set.
        let title = session
            .map(|s| self.session_title(s))
            .or_else(|| pane.read(cx).program_title())
            .unwrap_or_else(|| "空终端".into());
        let rang = pane.read(cx).bell;
        // What the Agent's adapter says about one it sees here but cannot read.
        let recognition = self.store.recognition();
        let unread = recognition.terminal_notes.get(id).cloned().or_else(|| {
            // A program that reports its own status, with no conversation read here.
            session
                .is_none()
                .then(|| recognition.terminal_reports.get(id))
                .flatten()
                .map(|r| crate::model::session_state::report_view(r).1)
        });
        let takeover = id.to_owned();
        let focus = pane.focus_handle(cx);
        let focus_id = id.to_owned();
        let menu_id = id.to_owned();
        let other_owner = pane.read(cx).has_other_owner();
        let card_id = session.map(|s| s.id.clone());
        let terminal_style = crate::ui::style::current(cx) == crate::ui::style::Style::Terminal;
        let decorated = self.pane_decorations(id, cx);
        // Why it needs you, short, rides on the state chip; the whole text and what it was
        // asked are in the title's hover card, so the pane carries no second line.
        let reason = session
            .zip(status)
            .and_then(|(s, g)| self.attention_reason(s, g));
        // Where the program runs left the footer for the title's hover card.
        let tip: Vec<String> = session
            .map(|s| self.session_line(s))
            .into_iter()
            .chain(
                session
                    .zip(status)
                    .map(|(s, g)| self.pane_story(s, g))
                    .unwrap_or_default(),
            )
            .chain(self.pane_place(id, session, cx))
            .chain(decorated.tip)
            .filter(|l| !l.is_empty())
            .collect();
        // 标记完成 whenever the Agent is not mid-turn or asking: done is yours to say.
        let review = session.filter(|s| {
            s.outcome == Outcome::Open
                && matches!(
                    status,
                    Some(Status::Review | Status::Background | Status::Paused)
                )
        });
        let completing = self.completing(id);
        let line = T::color(if focused { T::focus() } else { T::border() });
        // Who and where, never the conversation's title again: that is the tab's and the
        // sidebar's. `Codex ~/…/tools/mooring`; a plain shell names its program.
        let agent = self.pane_agent(id);
        let who = match agent {
            Some(agent) => agent.name().to_owned(),
            None => pane
                .read(cx)
                .program_title()
                .unwrap_or_else(|| "终端".into()),
        };
        let place = self.pane_cwd(id, session, cx).map(|c| short_place(&c));
        let left = div()
            .id(gpui::SharedString::from(format!("pane-title-{id}")))
            .map(|d| self.grip_title(id, terminal_style, d))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(T::S8))
            .when(terminal_style, |d| {
                d.when_some(status, |d, g| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .gap(px(T::S4))
                            .text_size(px(T::MONO_SIZE))
                            .text_color(g.colors().1)
                            .child(glyph(g))
                            .child(g.label()),
                    )
                })
            })
            // Whose pane: its CLI's mark always, or a terminal's; the move arrows on hover.
            .child(self.grip_mark(id, agent))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(T::SMALL))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(T::primary()))
                    .child(who),
            )
            .when_some(place, |d, place| {
                d.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(T::mono(cx).family)
                        .text_size(px(T::META))
                        .text_color(T::color(T::muted()))
                        .child(place),
                )
            })
            .when(rang, |d| d.child(label("🔔")))
            .when(!tip.is_empty(), |d| {
                let line = gpui::SharedString::from(tip.join("\n"));
                d.tooltip(move |_, cx| cx.new(|_| super::render::TextTip(line.clone())).into())
            });
        let menu_at = id.to_owned();
        let right = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(T::S8))
            // The turn's ask is out of sight above: hover reads it, a click goes back.
            .when_some(
                pane.read(cx).pinned_latest().filter(|(_, back)| {
                    *back && crate::ui::pinned_ask::current(cx).header_button()
                }),
                |d, (text, _)| {
                    let target = pane.clone();
                    let tip = gpui::SharedString::from(text);
                    d.child(
                        button(gpui::SharedString::from(format!("ask-back-{id}")), "↑ 你问")
                            .pane_action()
                            .tooltip(move |_, cx| {
                                cx.new(|_| super::render::TextTip(tip.clone())).into()
                            })
                            .on_click(cx.listener(move |_, _, _, cx| {
                                target.update(cx, |p, cx| p.back_to_latest_ask(cx));
                            })),
                    )
                },
            )
            .when_some(unread.clone().map(|n| (n.note, n.why)), |d, (note, why)| {
                d.child(
                    div()
                        .id(gpui::SharedString::from(format!("unread-{id}")))
                        .child(label(note))
                        .tooltip(move |_, cx| {
                            cx.new(|_| super::render::TextTip(why.clone().into()))
                                .into()
                        }),
                )
            })
            // What the plugins put here, before the state.
            .children(decorated.title.into_iter().map(|(line, tone)| {
                use mooring_core::decoration::Tone;
                let color = match tone {
                    Tone::Muted => T::color(T::muted()),
                    Tone::Ok => crate::ui::tone::Tone::Ok.ink(),
                    Tone::Warn => crate::ui::tone::Tone::Warn.ink(),
                    Tone::Error => crate::ui::tone::Tone::Danger.ink(),
                };
                div()
                    .flex_shrink_0()
                    .text_size(px(T::META))
                    .text_color(color)
                    .child(line)
            }))
            // A window used up shows where that CLI is working, with when it comes back.
            .when_some(session.and_then(|s| self.quota_alert(s)), |d, alert| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .px(px(T::S6))
                        .rounded(px(T::CHIP_RADIUS))
                        .bg(Tone::Danger.wash())
                        .text_size(px(T::META))
                        .text_color(Tone::Danger.ink())
                        .child(alert),
                )
            })
            .when_some(completing, |d, (style, t)| {
                d.child(self.completing_button(id, style, t))
            })
            // One state on the right: its word and how long it has been so. Why it needs
            // you is in the chip's hover card.
            .when_some(status.filter(|_| !terminal_style), |d, g| {
                let when = self.pane_when(session, cx);
                d.child(
                    div()
                        .id(gpui::SharedString::from(format!("pane-state-{id}")))
                        .child(state_chip(g, when))
                        .when_some(reason.clone(), |d, r| {
                            d.tooltip(move |_, cx| {
                                cx.new(|_| super::render::TextTip(r.clone().into())).into()
                            })
                        }),
                )
            })
            .when(terminal_style && unread.is_none(), |d| {
                d.child(self.pane_meta(session, cx))
            })
            // Everything else the pane offers is one click away in its menu: 看进度,
            // 放入待办, 标记完成, 待办详情 and 关闭画面 among the rest.
            .child(
                button(gpui::SharedString::from(format!("pane-more-{id}")), "⋯")
                    .aria_label("更多操作")
                    .pane_action()
                    .text_color(T::color(T::muted()))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        let path = this
                            .layout
                            .panes
                            .get(&menu_at)
                            .and_then(|p| p.read(cx).path_target());
                        this.layout.menu = Some(PaneMenu {
                            terminal: menu_at.clone(),
                            position: event.position(),
                            page: MenuPage::Main,
                            path,
                        });
                        cx.notify();
                    })),
            );
        // The turn bar: what this turn changed and how far its todo's criteria are, with
        // the window's one solid button, 标记完成. Only while it waits for you to check it.
        let turn = self
            .shown_turn_bar(id, session, review, status)
            .map(|(s, g)| self.turn_bar(id, s, g, cx));
        let body =
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .px(px(T::S14))
                .py(px(T::S10))
                .when_some(
                    pane.read(cx)
                        .handoff_state()
                        .filter(|_| self.workspace.pending_tasks.get(id) != Some(&None))
                        .zip(
                            self.workspace
                                .terminal_tab(id)
                                .and_then(|t| t.default_task.as_ref())
                                .and_then(|tid| self.workspace.tasks.iter().find(|t| &t.id == tid)),
                        ),
                    |d, (state, task)| d.child(self.handoff_bar(id, task, state, cx)),
                )
                .child(div().flex_1().min_h_0().child(pane.clone().cached(
                    gpui::StyleRefinement {
                        size: gpui::SizeRefinement {
                            width: Some(gpui::relative(1.).into()),
                            height: Some(gpui::relative(1.).into()),
                        },
                        ..Default::default()
                    },
                )));
        let footer = other_owner.then(|| self.pane_footer(id, takeover, cx));
        let mut pane_box = div()
            .id(gpui::SharedString::from(format!("pane-{id}")))
            .relative()
            // Typing into the terminal is how you carry on; its card steps out of the way.
            // Captured on the way down, so the keystroke still reaches the terminal.
            .capture_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                if !event.keystroke.modifiers.platform
                    && card_id.is_some()
                    && this.overview.here == card_id
                {
                    this.overview.here = None;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.nav.focused = Some(focus_id.clone());
                    window.focus(&focus, cx);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    let path = this
                        .layout
                        .panes
                        .get(&menu_id)
                        .and_then(|p| p.read(cx).path_target());
                    this.layout.menu = Some(PaneMenu {
                        terminal: menu_id.clone(),
                        position: event.position,
                        page: MenuPage::Main,
                        path,
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden();
        if terminal_style {
            // The title sits on the frame's top line, tmux / Zellij style: the header's
            // upper half is the gap around the pane, its lower half the pane, and each
            // group covers the line with the same two tones.
            let half = T::PANE_HEADER / 2.;
            let split = gpui::linear_gradient(
                180.,
                gpui::linear_color_stop(T::surface(T::app()), 0.5),
                gpui::linear_color_stop(T::surface(T::pane()), 0.5),
            );
            let frame = |d: gpui::Div| d.bg(line);
            pane_box = pane_box
                .child(
                    self.pane_grip(id, &title, div())
                        .h(px(T::PANE_HEADER))
                        .flex_shrink_0()
                        .relative()
                        .flex()
                        .items_center()
                        .px(px(T::S8))
                        .bg(split)
                        .when(borders, |d| {
                            d.child(frame(
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .top(px(half))
                                    .h(px(T::BORDER_WIDTH)),
                            ))
                            .child(frame(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top(px(half))
                                    .bottom_0()
                                    .w(px(T::BORDER_WIDTH)),
                            ))
                            .child(frame(
                                div()
                                    .absolute()
                                    .right_0()
                                    .top(px(half))
                                    .bottom_0()
                                    .w(px(T::BORDER_WIDTH)),
                            ))
                        })
                        .child(left.flex_shrink_1().px(px(T::S6)).bg(split))
                        .child(div().flex_1())
                        .child(right.px(px(T::S6)).bg(split)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .bg(T::surface(T::pane()))
                        .when(borders, |d| {
                            d.border_l(px(T::BORDER_WIDTH))
                                .border_r(px(T::BORDER_WIDTH))
                                .border_b(px(T::BORDER_WIDTH))
                                .border_color(line)
                        })
                        .children(turn)
                        .child(body)
                        .children(footer),
                );
        } else {
            // A rounded card: its edge drawn over everything inside it, so a program's
            // own background never covers the line, and the gutter shares the pane's
            // ground so nothing shows past the corners.
            pane_box = pane_box
                .bg(T::surface(T::pane()))
                .rounded(px(T::PANE_RADIUS))
                .child(
                    self.pane_grip(id, &title, div())
                        .h(px(T::PANE_HEADER))
                        .flex_shrink_0()
                        .rounded_t(px(T::PANE_RADIUS))
                        .pl(px(T::S14))
                        .pr(px(T::S10))
                        .flex()
                        .items_center()
                        .gap(px(T::S8))
                        // A pane that asks you something says so by its ring and its
                        // state chip; the title row keeps the card's own ground.
                        .bg(T::surface(T::header()))
                        // The title keeps its own width, so its hover pill hugs it.
                        .child(left)
                        .child(div().flex_1())
                        .child(right),
                )
                .children(turn)
                .child(body)
                .children(footer)
                // The card's edge; among several panes the focused one's is lighter.
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(T::PANE_RADIUS))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(if borders && focused {
                            T::control()
                        } else {
                            T::border()
                        })),
                )
                // Among several panes one that asks you something rings in its state's
                // color with a faint glow inside, so a glance across the split finds it.
                .when_some(asking.filter(|_| borders), |d, g| {
                    let ink = g.colors().0;
                    d.child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded(px(T::PANE_RADIUS))
                            .border(px(T::FOCUS_RING))
                            .border_color(ink)
                            .shadow(vec![gpui::BoxShadow {
                                color: ink.opacity(0.14),
                                offset: gpui::point(px(0.), px(0.)),
                                blur_radius: px(T::RING_GLOW),
                                spread_radius: px(0.),
                                inset: true,
                            }]),
                    )
                });
        }
        let pane_box = self
            .pane_landing(id, pane_box, cx)
            .when_some(
                session.filter(|s| self.overview.here.as_ref() == Some(&s.id)),
                |d, s| {
                    d.child(
                        div()
                            .absolute()
                            .top(px(T::PANE_HEADER + T::S10))
                            .right(px(T::S10))
                            .left(px(T::S10))
                            .bottom(px(T::S10))
                            .flex()
                            .justify_end()
                            .items_start()
                            .overflow_hidden()
                            .child(self.here_card(s, true, cx).max_h_full()),
                    )
                },
            )
            .when_some(completing, |d, (style, t)| {
                d.child(super::complete::spot(
                    &self.layout.spots,
                    id.to_owned(),
                    (0., 0.),
                    |s| &mut s.panes,
                ))
                .child(self.completing_veil(id, style, t, &title, cx))
            });
        pane_box.into_any_element()
    }
}
