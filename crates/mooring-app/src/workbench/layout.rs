//! Panes, their split layout, resizing and pane menus.
use super::*;

/// The most a terminal's text opened in an editor holds: the host's own read limit.
const SCROLLBACK_ROWS: usize = 10_000;
mod drag;
use drag::DropState;
use mooring_core::git_head::read_branch;

/// The visible panes and what is open over them: menus, drags and pending closes.
pub(super) struct LayoutState {
    pub(super) panes: HashMap<String, Entity<Pane>>,
    pub(super) menu: Option<PaneMenu>,
    pub(super) split_target: Option<(String, Axis)>,
    /// A plugin action waiting for its new pane: the command and the directory to run it in.
    pub(super) pending_command: Option<(String, String)>,
    pub(super) drag: Option<Vec<bool>>,
    pub(super) split_bounds: HashMap<Vec<bool>, gpui::Bounds<gpui::Pixels>>,
    /// Where a pane or tab being dragged would land.
    pub(super) drop: DropState,
    /// The tab strip's sideways scroll, read to tell which tabs are out of sight.
    pub(super) tab_scroll: gpui::ScrollHandle,
    /// The tab the strip last scrolled into view.
    pub(super) revealed_tab: Option<String>,
    /// A tab being renamed in place after a double-click.
    pub(super) renaming_tab: Option<(String, Entity<Field>, [gpui::Subscription; 2])>,
    /// A sidebar conversation being renamed in place after a double-click.
    pub(super) renaming_session: Option<(String, Entity<Field>, [gpui::Subscription; 2])>,
    /// The pane whose turn bar has 看改动 open: this turn's files listed under it.
    pub(super) changes_open: Option<String>,
    /// A space renamed in place in the sidebar; it carries the space so a just-added
    /// one can be named before the host's reply lists it.
    pub(super) renaming_space: Option<(Space, Entity<Field>, [gpui::Subscription; 2])>,
    /// A space's right-click menu: the space and where it was opened.
    pub(super) space_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    /// The space menu was opened from the dense presets' space line: it lists the spaces
    /// to switch to and a new one above the shown space's own entries.
    pub(super) space_switch: bool,
    /// 删除空间 was pressed once in the open space menu; the second press removes it.
    pub(super) space_removing: bool,
    /// A sidebar session row's right-click menu: the session and where it was opened.
    pub(super) session_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    /// The ✓ row unfolded in place: a session you marked complete opens nothing on a
    /// click, it shows what it ended with and offers 继续对话 and 撤销完成.
    pub(super) done_open: Option<String>,
    /// Session rows whose subagent list you opened (true) or closed (false); the rest
    /// follow the default, open on the session you are in.
    pub(super) subagents_toggled: HashMap<String, bool>,
    /// Session rows whose subagent list shows every subagent, not the first few.
    pub(super) subagents_all: HashSet<String>,
    /// A tab's right-click menu: the tab and where it was opened.
    pub(super) tab_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    /// The title-bar logo's menu for choosing the app icon, where it was opened.
    pub(super) icon_menu: Option<gpui::Point<gpui::Pixels>>,
    /// A close that would stop a running Agent, waiting for the user to confirm it.
    pub(super) confirm_close: Option<Closing>,
    /// Interrupted terminals already reopened in place once; a failed try keeps the button.
    pub(super) reopened: HashSet<String>,
    /// Panes playing their 标记完成 run, each with the run's number so a late timer
    /// from an earlier, kept run does not close the pane.
    pub(super) completing: HashMap<String, super::complete::Run>,
    pub(super) completing_seq: u64,
    /// Where panes and sidebar rows were drawn, for the run's overlay to fly between.
    pub(super) spots: super::complete::SharedSpots,
}
impl Workbench {
    pub(super) fn pane_view(
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
        let who = match session {
            Some(s) => s.runtime.name().to_owned(),
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
            .child(self.grip_mark(id, session))
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
    pub(super) fn layout_view(
        &self,
        tree: &Layout,
        path: Vec<bool>,
        borders: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        match tree {
            Layout::Leaf(id) => self.pane_view(id, borders, window, cx),
            Layout::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let down = *axis == Axis::Down;
                let mut left = path.clone();
                left.push(false);
                let mut right = path.clone();
                right.push(true);
                let drag_path = path.clone();
                let entity = cx.entity();
                let bounds_path = path.clone();
                div()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .relative()
                    .flex()
                    .when(down, |d| d.flex_col())
                    .child(
                        gpui::canvas(
                            |_, _, _| (),
                            move |bounds, _, _, cx| {
                                entity.update(cx, |this, _| {
                                    this.layout.split_bounds.insert(bounds_path.clone(), bounds);
                                });
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .min_h_0()
                            .flex_basis(px(0.))
                            .flex_grow(*ratio as f32)
                            .when(down, |d| d.w_full())
                            .when(!down, |d| d.h_full())
                            .child(self.layout_view(first, left, true, window, cx)),
                    )
                    .child(
                        // The gutter is the gap between the panes' cards, with a handle
                        // on hover or while dragging.
                        div()
                            .id(gpui::SharedString::from(format!("divider-{path:?}")))
                            .group("divider")
                            .relative()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(down, |d| {
                                d.h(px(T::S8))
                                    .w_full()
                                    .cursor(gpui::CursorStyle::ResizeUpDown)
                            })
                            .when(!down, |d| {
                                d.w(px(T::S8))
                                    .h_full()
                                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                            })
                            .child(
                                div()
                                    .rounded_full()
                                    .when(down, |d| d.h(px(T::S2)).w(px(T::S20 * 2.)))
                                    .when(!down, |d| d.w(px(T::S2)).h(px(T::S20 * 2.)))
                                    .when(self.layout.drag.as_ref() == Some(&path), |d| {
                                        d.bg(T::color(T::focus()))
                                    })
                                    .group_hover("divider", |d| d.bg(T::color(T::focus()))),
                            )
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    // Its resize is a request, which waits behind a busy one.
                                    this.layout.drag = Some(drag_path.clone());
                                    this.layout.menu = None;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .min_h_0()
                            .flex_basis(px(0.))
                            .flex_grow((10000 - *ratio) as f32)
                            .when(down, |d| d.w_full())
                            .when(!down, |d| d.h_full())
                            .child(self.layout_view(second, right, true, window, cx)),
                    )
                    .into_any_element()
            }
        }
    }
    pub(super) fn resize_drag(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = &self.layout.drag else {
            return;
        };
        let Some(bounds) = self.layout.split_bounds.get(path) else {
            return;
        };
        let Some(Layout::Split { axis, ratio, .. }) = self
            .workspace
            .tabs
            .iter_mut()
            .find(|t| Some(&t.id) == self.nav.tab.as_ref())
            .and_then(|t| t.layout.as_mut())
            .and_then(|l| l.at_mut(path))
        else {
            return;
        };
        let fraction = if *axis == Axis::Right {
            (event.position.x - bounds.left()) / bounds.size.width
        } else {
            (event.position.y - bounds.top()) / bounds.size.height
        };
        *ratio = (fraction.clamp(0.05, 0.95) * 10000.).round() as u16;
        cx.notify();
    }
    pub(super) fn finish_drag(&mut self, cx: &mut Context<Self>) {
        self.layout.drop = DropState::default();
        let (Some(path), Some(tab)) = (self.layout.drag.take(), self.nav.tab.clone()) else {
            return;
        };
        let at = (self.workspace.tabs.iter_mut().find(|t| t.id == tab))
            .and_then(|t| t.layout.as_mut()?.at_mut(&path).cloned());
        if let Some(Layout::Split { axis, ratio, .. }) = at {
            self.edit(
                Edit::ResizeSplit {
                    tab,
                    path,
                    axis,
                    ratio,
                },
                cx,
            );
        }
    }
    /// Moves a pane, process and all, beside the panes of another tab or into a tab of
    /// its own, and follows it there. Its old tab closes with its last pane.
    pub(super) fn move_pane(&mut self, terminal: &str, to: Option<&str>, cx: &mut Context<Self>) {
        let (id, edit) = match to {
            Some(to) => {
                let Some(target) = self.workspace.tab_terminals(to).last().cloned() else {
                    return;
                };
                let side = Some((Axis::Right, false));
                let terminal = terminal.to_owned();
                (
                    to.to_owned(),
                    Edit::MovePane {
                        terminal,
                        target,
                        side,
                    },
                )
            }
            None => {
                let tab = Tab {
                    id: super::id(),
                    space: self.nav.space.clone(),
                    default_task: None,
                    title: String::new(),
                    layout: Some(Layout::Leaf(terminal.to_owned())),
                };
                (tab.id.clone(), Edit::CreateTab { tab })
            }
        };
        self.nav.zoomed = None;
        self.nav.tab = Some(id);
        self.nav.focused = Some(terminal.to_owned());
        self.nav.pending_focus = true;
        self.edit(edit, cx);
    }
    /// The terminal's text, scrollback included, saved under the host's private runtime
    /// directory and opened with `$EDITOR` in a new tab. The host reads it,
    /// so a pane's own screen is never locked for it; one file per terminal, replaced.
    fn scrollback_in_editor(&mut self, id: &str, cx: &mut Context<Self>) {
        let socket = self.store.socket().to_owned();
        let terminal = id.to_owned();
        let cwd = self
            .workspace
            .terminal_tab(id)
            .and_then(|t| self.workspace.spaces.iter().find(|s| s.id == t.space))
            .map(|s| s.cwd.clone())
            .unwrap_or_default();
        let task = cx.background_spawn(async move {
            let Reply::Text(text) = rpc(
                &socket,
                &Request::ReadScreen {
                    id: terminal.clone(),
                    rows: SCROLLBACK_ROWS,
                },
            )?
            else {
                return Err(std::io::Error::other("后台没有返回文字"));
            };
            let dir = socket
                .parent()
                .ok_or_else(|| std::io::Error::other("运行目录未知"))?
                .join("scrollback");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(format!("{terminal}.txt"));
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)?;
            std::io::Write::write_all(&mut file, text.trim_end().as_bytes())?;
            Ok(path)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(path) => {
                    // Through sh so the user's $EDITOR applies whatever their shell is.
                    let command = format!(
                        "sh -c '${{EDITOR:-vi}} \"$1\"' sh {}",
                        shell_quote(&path.to_string_lossy())
                    );
                    this.layout.pending_command = Some((command, cwd));
                    this.launch(false, None, cx);
                }
                Err(e) => {
                    this.error = Some(format!("在编辑器里看滚动内容失败：{e}"));
                    cx.notify();
                }
            })
        })
        .detach();
    }
    pub(super) fn menu_action(
        &mut self,
        id: &str,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let menu = self.layout.menu.take();
        match action {
            "向右分屏" | "向下分屏" => {
                self.nav.zoomed = None;
                self.layout.split_target = Some((
                    id.into(),
                    if action == "向右分屏" {
                        Axis::Right
                    } else {
                        Axis::Down
                    },
                ));
                self.launch(true, None, cx);
            }
            "放大" | "还原" => {
                self.nav.zoomed = if self.nav.zoomed.as_deref() == Some(id) {
                    None
                } else {
                    Some(id.into())
                };
                self.sync_panes(cx);
                if let Some(pane) = self.layout.panes.get(id) {
                    window.focus(&pane.focus_handle(cx), cx);
                }
            }
            "关闭画面" => self.close_pane(id, cx),
            "复制" => {
                if let Some(pane) = self.layout.panes.get(id) {
                    pane.update(cx, |p, cx| p.copy(cx));
                }
            }
            "粘贴" => {
                if let Some(pane) = self.layout.panes.get(id) {
                    pane.update(cx, |p, cx| p.paste(cx));
                    window.focus(&pane.focus_handle(cx), cx);
                }
            }
            "在编辑器里看滚动内容" => self.scrollback_in_editor(id, cx),
            "搜索滚动内容" => {
                if let Some(pane) = self.layout.panes.get(id) {
                    pane.update(cx, |p, cx| p.open_search(window, cx));
                }
            }
            "中断（Ctrl-C）" => {
                if let Some(pane) = self.layout.panes.get(id) {
                    pane.update(cx, |p, cx| p.interrupt(cx));
                }
            }
            "停止 Agent" => self.request_close(Closing::Agent(id.into()), cx),
            "取消关联" => {
                if let Some(session) = self.current_agent(id) {
                    self.edit(
                        Edit::AssignTask {
                            id: session.id.clone(),
                            task: None,
                        },
                        cx,
                    );
                }
            }
            "挂到待办…" | "为这个待办开一个会话" | "移到标签…" | "为什么是这个状态" => {
                if let Some(mut menu) = menu {
                    menu.page = match action {
                        "挂到待办…" => MenuPage::Assign,
                        "移到标签…" => MenuPage::Move,
                        "为什么是这个状态" => MenuPage::Why,
                        _ => MenuPage::Sessions,
                    };
                    self.layout.menu = Some(menu);
                }
            }
            _ => (),
        }
        cx.notify();
    }
    /// The right-clicked path's entries: open it, show it, copy it whole or relative.
    fn path_entries(
        &self,
        terminal: &str,
        target: &crate::terminal::pane::Hovered,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        use crate::terminal::{open_path, pane::Pane};
        let relative = open_path::relative(&target.path, target.cwd.as_deref());
        let name = target
            .path
            .file_name()
            .map_or_else(|| target.path.to_string_lossy(), |n| n.to_string_lossy())
            .into_owned();
        let mut entries = vec![
            label(name)
                .h(px(T::ROW))
                .px(px(T::S10))
                .flex()
                .items_center()
                .truncate()
                .into_any_element(),
        ];
        let items: [(&str, String, bool); 4] = [
            ("open", Pane::open_title(target), true),
            ("reveal", "在访达中显示".into(), true),
            ("copy", "复制路径".into(), true),
            ("copy-relative", "复制相对路径".into(), relative.is_some()),
        ];
        for (key, title, enabled) in items {
            let terminal = terminal.to_owned();
            let target = target.clone();
            let relative = relative.clone();
            entries.push(
                button(gpui::SharedString::from(format!("menu-path-{key}")), title)
                    .w_full()
                    .justify_start()
                    .border_0()
                    .when(!enabled, |d| d.opacity(0.4))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !enabled {
                            return;
                        }
                        this.layout.menu = None;
                        if let Some(pane) = this.layout.panes.get(&terminal) {
                            pane.update(cx, |p, cx| match key {
                                "open" => p.open_target(&target, false, cx),
                                "reveal" => p.open_target(&target, true, cx),
                                "copy" => p.copy_note(
                                    target.path.to_string_lossy().into_owned(),
                                    "已复制路径",
                                    cx,
                                ),
                                _ => p.copy_note(
                                    relative.clone().unwrap_or_default(),
                                    "已复制相对路径",
                                    cx,
                                ),
                            });
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        entries.push(
            div()
                .h(px(T::S4))
                .border_b(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .mb(px(T::S4))
                .into_any_element(),
        );
        entries
    }
    /// The steps a pane's title once showed as buttons, now at the top of its menu:
    /// 标记完成 when it is due, 看进度, 放入待办 while the tab has a todo to hand over,
    /// and 待办详情.
    fn pane_entries(&self, terminal: &str, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        let session = self.pane_session(terminal);
        let status = self.pane_status(terminal).map(|(g, _)| g);
        let mut out = Vec::new();
        let entry = |key: &str, title: &'static str| {
            button(gpui::SharedString::from(format!("menu-{key}")), title)
                .w_full()
                .justify_start()
                .border_0()
        };
        if let Some(s) = session.filter(|s| {
            s.outcome == Outcome::Open
                && self.completing(terminal).is_none()
                && matches!(
                    status,
                    Some(Status::Review | Status::Background | Status::Paused)
                )
        }) {
            let (t, sid) = (terminal.to_owned(), s.id.clone());
            out.push(
                entry("accept", "标记完成")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.layout.menu = None;
                        this.complete_pane(t.clone(), sid.clone(), cx)
                    }))
                    .into_any_element(),
            );
        }
        if let Some(s) = session {
            let sid = s.id.clone();
            out.push(
                entry("here", "看进度")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.layout.menu = None;
                        this.overview.here = Some(sid.clone());
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        let pane = self.layout.panes.get(terminal);
        let tab_task = self
            .workspace
            .terminal_tab(terminal)
            .and_then(|t| t.default_task.as_ref())
            .and_then(|tid| self.workspace.tasks.iter().find(|t| &t.id == tid));
        if let Some(text) = tab_task
            .filter(|_| session.is_some())
            .filter(|_| pane.is_some_and(|p| p.read(cx).handoff_state().is_none()))
            .map(|t| t.prompt())
        {
            let target = terminal.to_owned();
            out.push(
                entry("place-task", "放入待办")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.layout.menu = None;
                        if let Some(p) = this.layout.panes.get(&target) {
                            p.update(cx, |p, cx| p.place_text(text.clone(), cx));
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        if let Some(task) = self.pane_task(terminal, session) {
            let tid = task.id.clone();
            out.push(
                entry("task-detail", "待办详情")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.layout.menu = None;
                        this.nav.task_detail = Some(tid.clone());
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        out
    }
    pub(super) fn menu_view(
        &self,
        menu: &PaneMenu,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let session = self.current_agent(&menu.terminal);
        let task = self
            .workspace
            .terminal_tab(&menu.terminal)
            .and_then(|t| t.default_task.clone());
        let mut entries = Vec::new();
        match menu.page {
            MenuPage::Main => {
                if let Some(target) = &menu.path {
                    entries.extend(self.path_entries(&menu.terminal, target, cx));
                }
                let own = self.pane_entries(&menu.terminal, cx);
                if !own.is_empty() {
                    entries.extend(own);
                    entries.push(menu_rule(usize::MAX));
                }
                let keys = crate::keymap::current(cx);
                let zoomed = self.nav.zoomed.as_deref() == Some(&menu.terminal);
                for (i, row) in menu_rows(zoomed).into_iter().enumerate() {
                    let Some(title) = row else {
                        entries.push(menu_rule(i));
                        continue;
                    };
                    // A greyed item says why on hover, so it doesn't read as broken.
                    let blocked = match title {
                        "为这个待办开一个会话" => {
                            task.is_none().then_some("这个标签没有设默认待办")
                        }
                        "挂到待办…" | "停止 Agent" => {
                            session.is_none().then_some("这个终端里没有识别到 Agent")
                        }
                        "取消关联" => match session {
                            None => Some("这个终端里没有识别到 Agent"),
                            Some(s) if s.task.is_none() => Some("这个会话没有挂到待办"),
                            Some(_) => None,
                        },
                        _ => None,
                    };
                    let shortcut = menu_shortcut(title, &keys);
                    let terminal = menu.terminal.clone();
                    let action = title.to_owned();
                    entries.push(
                        button(gpui::SharedString::from(format!("menu-{title}")), "")
                            .aria_label(title)
                            .w_full()
                            .justify_between()
                            .border_0()
                            // No row gap: `button` puts its (empty) text first, and a gap
                            // would indent these titles past the ones above the rule.
                            .child(
                                div()
                                    .flex_1()
                                    .when(blocked.is_some(), |d| d.opacity(0.4))
                                    .child(title),
                            )
                            .when_some(shortcut, |d, keys| {
                                d.child(
                                    div()
                                        .ml(px(T::S16))
                                        .text_size(px(T::META))
                                        .text_color(T::color(T::muted()))
                                        .child(keys),
                                )
                            })
                            .when_some(blocked, |d, why| {
                                d.cursor_default().tooltip(move |_, cx| {
                                    cx.new(|_| super::render::TextTip(why.into())).into()
                                })
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if blocked.is_none() {
                                    this.menu_action(&terminal, &action, window, cx);
                                }
                            }))
                            .into_any_element(),
                    );
                }
                // Plugin actions this pane offers, after the built-ins.
                let offered = self.actions_for(&menu.terminal);
                if !offered.is_empty() {
                    entries.push(
                        label("插件")
                            .h(px(T::ROW))
                            .px(px(T::S10))
                            .flex()
                            .items_center()
                            .border_t(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::border()))
                            .into_any_element(),
                    );
                }
                for action in offered {
                    let terminal = menu.terminal.clone();
                    entries.push(
                        button(
                            gpui::SharedString::from(format!("menu-action-{}", action.id)),
                            action.title.clone(),
                        )
                        .w_full()
                        .justify_start()
                        .border_0()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.run_action(&terminal, action, cx);
                        }))
                        .into_any_element(),
                    );
                }
                // A program that took the mouse gets plain drags; say how to select instead.
                if self
                    .layout
                    .panes
                    .get(&menu.terminal)
                    .is_some_and(|p| p.read(cx).takes_mouse())
                {
                    entries.push(
                        label("选中文字：按住 Shift 再拖动")
                            .h(px(T::ROW))
                            .px(px(T::S10))
                            .flex()
                            .items_center()
                            .into_any_element(),
                    );
                }
            }
            MenuPage::Sessions => {
                for agent in Agent::all() {
                    let terminal = menu.terminal.clone();
                    let task = task.clone();
                    entries.push(
                        button(
                            gpui::SharedString::from(format!("new-{}", agent.executable())),
                            agent.name(),
                        )
                        .w_full()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.layout.menu = None;
                            this.tasks.chosen_agent = Some(agent);
                            this.layout.split_target = Some((terminal.clone(), Axis::Right));
                            this.launch(true, task.clone(), cx);
                        }))
                        .into_any_element(),
                    );
                }
            }
            MenuPage::Why => {
                let hints = &self.store.recognition().terminal_hints;
                for (i, line) in state_reasons(
                    &self.workspace,
                    &menu.terminal,
                    hints.get(&menu.terminal).map(String::as_str),
                    now(),
                )
                .into_iter()
                .enumerate()
                {
                    entries.push(
                        div()
                            .id(gpui::SharedString::from(format!("why-{i}")))
                            .w_full()
                            .px(px(T::S10))
                            .py(px(T::S4))
                            .text_size(px(if i == 0 { T::META } else { T::SMALL }))
                            .text_color(T::color(if i == 0 {
                                T::secondary()
                            } else {
                                T::primary()
                            }))
                            .child(line)
                            .into_any_element(),
                    );
                }
            }
            MenuPage::Move => {
                let own = self
                    .workspace
                    .terminal_tab(&menu.terminal)
                    .map(|t| t.id.clone());
                let targets: Vec<_> = self
                    .workspace
                    .tabs
                    .iter()
                    .filter(|t| t.space == self.nav.space && Some(&t.id) != own.as_ref())
                    .map(|t| (Some(t.id.clone()), self.tab_name(t, cx)))
                    .chain([(None, "新标签".to_owned())])
                    .collect();
                for (tab, name) in targets {
                    let terminal = menu.terminal.clone();
                    entries.push(
                        button(
                            gpui::SharedString::from(format!(
                                "move-to-{}",
                                tab.as_deref().unwrap_or("new")
                            )),
                            name,
                        )
                        .w_full()
                        .justify_start()
                        .border_0()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.layout.menu = None;
                            this.move_pane(&terminal, tab.as_deref(), cx);
                        }))
                        .into_any_element(),
                    );
                }
            }
            MenuPage::Assign => {
                if let Some(session) = session {
                    for task in &self.workspace.tasks {
                        let sid = session.id.clone();
                        let tid = task.id.clone();
                        entries.push(
                            button(
                                gpui::SharedString::from(format!("assign-{tid}")),
                                task.title.clone(),
                            )
                            .w_full()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.layout.menu = None;
                                this.edit(
                                    Edit::AssignTask {
                                        id: sid.clone(),
                                        task: Some(tid.clone()),
                                    },
                                    cx,
                                );
                            }))
                            .into_any_element(),
                        );
                    }
                }
            }
        }
        let position = menu.position;
        self.popup(position, entries, window, cx, |this| {
            this.layout.menu = None
        })
    }
    /// A tab's right-click menu.
    pub(super) fn tab_menu_view(
        &self,
        id: &str,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let rename = id.to_owned();
        let close = id.to_owned();
        let tabs = self.sibling_tabs(id);
        let at = tabs.iter().position(|t| t == id).unwrap_or(0);
        let mut entries: Vec<gpui::AnyElement> =
            [("向左移", -1, at > 0), ("向右移", 1, at + 1 < tabs.len())]
                .into_iter()
                .map(|(name, step, enabled)| {
                    let id = id.to_owned();
                    button(gpui::SharedString::from(format!("tab-menu-{name}")), name)
                        .w_full()
                        .justify_start()
                        .border_0()
                        .when(!enabled, |d| d.opacity(0.4))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if enabled {
                                this.layout.tab_menu = None;
                                this.step_tab(&id, step, cx);
                            }
                        }))
                        .into_any_element()
                })
                .collect();
        entries.extend([
            button("tab-menu-rename", "重命名")
                .w_full()
                .justify_start()
                .border_0()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.layout.tab_menu = None;
                    this.rename_tab(&rename, window, cx);
                }))
                .into_any_element(),
            button("tab-menu-close", "关闭标签")
                .w_full()
                .justify_start()
                .border_0()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.close_tab(&close, cx);
                }))
                .into_any_element(),
        ]);
        self.popup(position, entries, window, cx, |this| {
            this.layout.tab_menu = None
        })
    }
    /// Ink ring or mascot: the title bar and the Dock tile change at once, and the
    /// choice is remembered for the next launch.
    pub(super) fn icon_menu_view(
        &self,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let current = cx.global::<Chosen>().icon;
        let entries = AppIcon::ALL
            .into_iter()
            .map(|icon| {
                div()
                    .id(gpui::SharedString::from(format!(
                        "app-icon-{}",
                        icon.label()
                    )))
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(icon.label())
                    .h(px(T::ROW))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .rounded(px(T::CONTROL_RADIUS))
                    .text_size(px(T::SMALL))
                    .cursor_pointer()
                    .hover(|s| s.bg(T::color(T::button_hover())))
                    .child(gpui::img(icon.title_image()).size(px(T::LOGO)))
                    .child(div().flex_1().child(icon.label()))
                    .when(icon == current, |d| {
                        d.child(div().text_color(T::color(T::link())).child("✓"))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.layout.icon_menu = None;
                        choose_icon(icon, cx);
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        self.popup(position, entries, window, cx, |this| {
            this.layout.icon_menu = None
        })
    }
    /// Asks before a close stops running Agents or takes several splits with it; a click
    /// outside cancels.
    pub(super) fn confirm_close_view(
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
    /// A floating menu at `position`; a click outside it runs `dismiss`.
    pub(super) fn popup(
        &self,
        position: gpui::Point<gpui::Pixels>,
        entries: Vec<gpui::AnyElement>,
        window: &Window,
        cx: &Context<Self>,
        dismiss: fn(&mut Self),
    ) -> gpui::AnyElement {
        let height = px(T::ROW * entries.len() as f32 + T::S12);
        let viewport = window.viewport_size();
        div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    dismiss(this);
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("pane-menu")
                    .absolute()
                    .left(
                        position
                            .x
                            .min(viewport.width - px(T::MENU_WIDTH))
                            .max(px(0.)),
                    )
                    .top(position.y.min(viewport.height - height).max(px(0.)))
                    .w(px(T::MENU_WIDTH))
                    .p(px(T::S6))
                    .bg(T::color(T::raised()))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::control()))
                    .rounded(px(T::CONTROL_RADIUS))
                    .shadow_lg()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(entries),
            )
            .into_any_element()
    }
}
/// Applies `icon` to the title bar and the Dock, and remembers it.
/// One look for the pane header's text actions: link-colored, filled only on hover.
pub(super) trait PaneAction {
    fn pane_action(self) -> Self;
}
impl PaneAction for gpui::Stateful<gpui::Div> {
    fn pane_action(self) -> Self {
        self.h(px(T::PANE_ACTION))
            .px(px(T::S6))
            .rounded(px(T::PANE_ACTION_RADIUS))
            .text_size(px(T::SMALL))
            .text_color(T::color(T::link()))
    }
}
/// The pane header's status: the group's wash, its ink and its dot.
/// The state word, and after it why the session needs you when a reasons rule knows:
/// 「等你回答 · 批准 WebSearch」. The phrase is already cut to the rules' width.
fn state_chip(group: Status, reason: Option<String>) -> gpui::Div {
    let (_, ink, wash) = group.colors();
    div()
        .flex_shrink_0()
        .h(px(T::STATE_CHIP))
        .px(px(T::S6))
        .flex()
        .items_center()
        .gap(px(T::PULSE_GAP))
        .rounded(px(T::STATE_CHIP_RADIUS))
        .bg(wash)
        .text_color(ink)
        .text_size(px(T::META))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(crate::ui::widgets::status_mark(group))
        .child(group.label())
        .when_some(reason, |d, r| {
            d.child(div().opacity(0.6).child("·"))
                .child(div().font_weight(gpui::FontWeight::NORMAL).child(r))
        })
}
/// Lower is more urgent: what a tab's dot shows when its panes disagree.
pub(super) fn urgency(group: Status) -> u8 {
    match group {
        Status::Waiting => 0,
        Status::Error => 1,
        Status::Review => 2,
        Status::Running => 3,
        Status::Background => 4,
        Status::Paused => 5,
        Status::Deferred => 6,
        Status::Stale => 7,
        Status::Fresh => 8,
        Status::Closed => 9,
    }
}
/// `3:12`, or `1:03:12` past an hour.
/// A question as the host words it, `【范围】问题（a / b）`, down to `问题`.
pub(super) fn ask_prompt(line: &str) -> &str {
    let line = line.split_once('】').map_or(line, |(_, rest)| rest);
    line.rsplit_once('（').map_or(line, |(head, _)| head).trim()
}
/// `+128 −14`: lines a turn added in green and removed in red; a side that is zero is left
/// out.
pub(super) fn line_counts((added, removed): (u32, u32)) -> gpui::Div {
    div()
        .flex_shrink_0()
        .flex()
        .gap(px(T::S4))
        .font_features(T::tabular())
        .when(added > 0, |d| {
            d.child(div().text_color(Tone::Ok.ink()).child(format!("+{added}")))
        })
        .when(removed > 0, |d| {
            d.child(
                div()
                    .text_color(Tone::Danger.ink())
                    .child(format!("−{removed}")),
            )
        })
}
pub(super) fn turn_clock(seconds: u64) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
/// A running turn's clock is redrawn once a second while one is on screen, and not at all
/// otherwise; one pending redraw serves every pane.
static TURN_TICK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const TURN_TICK_EVERY: Duration = Duration::from_secs(1);
/// Each directory's branch with when it was read.
type Branches = HashMap<String, (std::time::Instant, Option<String>)>;
/// Branches by directory, read off the render path and reread at most this often.
static BRANCHES: std::sync::LazyLock<std::sync::Mutex<Branches>> =
    std::sync::LazyLock::new(Default::default);
const BRANCH_TTL: Duration = Duration::from_secs(10);
impl Workbench {
    /// A pane's status as the sidebar shows it, and whether it asks for attention; a
    /// conversation already closed out and no longer live shows none.
    pub(super) fn pane_status(&self, id: &str) -> Option<(Status, bool)> {
        let s = self.pane_session(id)?;
        if live_terminal(&self.workspace, s).is_none() && !s.outcome.needs_review(now()) {
            return None;
        }
        Some(session_status(&self.workspace, &self.attention.unseen, s))
    }
    /// `claude · 5 分钟`, or `claude · 本轮 3:12` while a turn runs in a live terminal.
    fn pane_meta(&self, session: Option<&AgentSession>, cx: &Context<Self>) -> gpui::Div {
        let meta = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(T::S6))
            .text_size(px(T::META))
            .text_color(T::color(T::muted()))
            .font_features(T::tabular());
        let Some(s) = session else {
            return meta.child("Shell");
        };
        let content = self.workspace.content.get(&s.id);
        let started = content
            .filter(|_| live_terminal(&self.workspace, s).is_some())
            .and_then(mooring_core::content::Content::running_since);
        let when = match started {
            Some(at) => {
                self.tick_turn(cx);
                format!("本轮 {}", turn_clock(now().saturating_sub(at)))
            }
            None => crate::model::time::short_ago(content.map_or(s.updated, |c| c.updated), now()),
        };
        meta.child(super::board::agent_mark(s.runtime, T::color(T::muted())))
            .child(s.runtime.executable().to_string())
            .child(div().opacity(T::ZERO_OPACITY).child("·"))
            .child(when)
    }
    pub(super) fn tick_turn(&self, cx: &Context<Self>) {
        super::pace::wake(&TURN_TICK, TURN_TICK_EVERY, cx);
    }
    /// The cached branch for `dir`; a missing or old entry is read in the background and
    /// redraws the window only if it changed.
    pub(super) fn git_branch(&self, dir: &str, cx: &Context<Self>) -> Option<String> {
        if dir.is_empty() {
            return None;
        }
        let mut map = BRANCHES.lock().ok()?;
        let old = match map.get(dir) {
            Some((at, branch)) if at.elapsed() < BRANCH_TTL => return branch.clone(),
            Some((_, branch)) => branch.clone(),
            None => None,
        };
        // Marked as read now, so the redraws before the read lands do not read again.
        map.insert(dir.to_owned(), (std::time::Instant::now(), old.clone()));
        drop(map);
        let dir = dir.to_owned();
        cx.spawn(async move |this, cx| {
            let at = PathBuf::from(&dir);
            let branch = cx.background_spawn(async move { read_branch(&at) }).await;
            let changed = BRANCHES.lock().is_ok_and(|mut map| {
                let before = map.insert(dir, (std::time::Instant::now(), branch.clone()));
                before.and_then(|(_, b)| b) != branch
            });
            if changed {
                let _ = this.update(cx, |_, cx| cx.notify());
            }
        })
        .detach();
        old
    }
    /// The directory the pane's program is in: as the terminal reports it, else where its
    /// conversation or its terminal started.
    fn pane_cwd(
        &self,
        id: &str,
        session: Option<&AgentSession>,
        cx: &Context<Self>,
    ) -> Option<String> {
        self.layout
            .panes
            .get(id)
            .and_then(|p| p.read(cx).cwd().map(|c| c.to_string_lossy().into_owned()))
            .or_else(|| session.map(|s| s.cwd.clone()).filter(|c| !c.is_empty()))
            .or_else(|| {
                self.workspace
                    .terminals
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.cwd.clone())
            })
            .filter(|c| !c.is_empty())
    }
    /// How long the pane's state has held, for its title: `本轮 3:12` while a turn runs in
    /// a live terminal, else the time since its last change (`34 分钟`).
    fn pane_when(&self, session: Option<&AgentSession>, cx: &Context<Self>) -> Option<String> {
        let s = session?;
        let content = self.workspace.content.get(&s.id);
        let started = content
            .filter(|_| live_terminal(&self.workspace, s).is_some())
            .and_then(mooring_core::content::Content::running_since);
        Some(match started {
            Some(at) => {
                self.tick_turn(cx);
                format!("本轮 {}", turn_clock(now().saturating_sub(at)))
            }
            None => crate::model::time::short_ago(content.map_or(s.updated, |c| c.updated), now()),
        })
    }
    /// The files this turn changed, from the session's own record of its edits.
    pub(super) fn turn_files(&self, s: &AgentSession) -> usize {
        let Some(c) = self.workspace.content.get(&s.id) else {
            return 0;
        };
        c.files.iter().filter(|t| t.at >= c.turn_started).count()
    }
    /// The todo a pane's work counts toward: its conversation's own, else its tab's.
    pub(super) fn pane_task(&self, id: &str, s: Option<&AgentSession>) -> Option<&Task> {
        s.and_then(|s| s.task.as_ref())
            .or_else(|| {
                self.workspace
                    .terminal_tab(id)
                    .and_then(|t| t.default_task.as_ref())
            })
            .and_then(|tid| self.workspace.tasks.iter().find(|t| &t.id == tid))
    }
    /// Under a pane's title while its turn waits for you to check it: `这一轮改了 3 个文件 ·
    /// 验收标准 1/3`, then 标记完成, the only solid button in the window. To say what is
    /// wrong, type in the terminal below. A part with nothing to say is left out.
    fn turn_bar(&self, id: &str, s: &AgentSession, g: Status, cx: &Context<Self>) -> gpui::Div {
        // The session's own state ink, as its title's chip has it.
        let ink = g.colors().1;
        let files = self.turn_files(s);
        let task = self.pane_task(id, Some(s));
        let mut facts = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(T::S6))
            .overflow_hidden()
            .font_features(T::tabular());
        facts = facts.child(
            div()
                .flex_shrink_0()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(ink)
                .child(if files > 0 {
                    format!("这一轮改了 {files} 个文件")
                } else {
                    "这一轮做完了".to_owned()
                }),
        );
        let content = self.workspace.content.get(&s.id);
        if let Some(lines) = content.map(|c| c.turn_lines()).filter(|(a, r)| a + r > 0) {
            facts = facts.child(line_counts(lines));
        }
        if let Some(task) = task.filter(|t| !t.acceptance.is_empty()) {
            let tid = task.id.clone();
            facts = facts
                .child(div().text_color(T::color(T::muted())).child("·"))
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("turn-criteria-{id}")))
                        .flex_shrink_0()
                        .cursor_pointer()
                        .text_color(T::color(T::secondary()))
                        .hover(|d| d.text_color(T::color(T::primary())))
                        .child(format!(
                            "验收标准 {}/{}",
                            task.proven(),
                            task.acceptance.len()
                        ))
                        .tooltip(|_, cx| {
                            cx.new(|_| super::render::TextTip("打开待办详情".into()))
                                .into()
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.nav.task_detail = Some(tid.clone());
                            cx.notify();
                        })),
                );
        }
        let terminal = id.to_owned();
        let accept = s.id.clone();
        let open = self.layout.changes_open.as_deref() == Some(id);
        let toggle = id.to_owned();
        // 看改动: this turn's files, each with its own line counts, listed under the bar.
        let changes = (files > 0).then(|| {
            button(
                gpui::SharedString::from(format!("turn-changes-{id}")),
                "看改动",
            )
            .h(px(T::PANE_ACTION))
            .px(px(T::S10))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            .text_color(T::color(T::primary()))
            .when(open, |d| d.bg(T::color(T::selection())))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.layout.changes_open =
                    if this.layout.changes_open.as_deref() == Some(toggle.as_str()) {
                        None
                    } else {
                        Some(toggle.clone())
                    };
                cx.notify();
            }))
        });
        let list = open.then(|| {
            let started = content.map_or(0, |c| c.turn_started);
            let mut rows: Vec<_> = content
                .map(|c| c.files.iter().filter(|t| t.at >= started).collect())
                .unwrap_or_default();
            rows.reverse();
            div()
                .flex_shrink_0()
                .max_h(px(T::TURN_BAR * 6.))
                .overflow_hidden()
                .px(px(T::S14))
                .py(px(T::S6))
                .flex()
                .flex_col()
                .gap(px(T::S2))
                .border_b(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .text_size(px(T::SMALL))
                .font_features(T::tabular())
                .children(rows.into_iter().map(|t| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(T::S8))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(T::color(T::secondary()))
                                .child(t.path.clone()),
                        )
                        .child(line_counts((t.added, t.removed)))
                }))
        });
        let bar = div()
            .h(px(T::TURN_BAR))
            .flex_shrink_0()
            .pl(px(T::S14))
            .pr(px(T::S10))
            .flex()
            .items_center()
            .gap(px(T::S8))
            .border_b(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            // A faint wash of the state's hue, as in the v3 design: the bar reads as the
            // turn's verdict, not as one more toolbar.
            .bg(g.colors().2.opacity(0.35))
            .text_size(px(T::SMALL))
            .child(facts)
            .children(changes)
            .child(super::forms::solid(
                button(
                    gpui::SharedString::from(format!("accept-pane-{id}")),
                    "标记完成",
                )
                .h(px(T::PANE_ACTION))
                .px(px(T::S10))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.complete_pane(terminal.clone(), accept.clone(), cx)
                })),
            ));
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(bar)
            .children(list)
    }
    /// `~/code/app · main`: where the pane's program is and on which branch.
    fn pane_place(
        &self,
        id: &str,
        session: Option<&AgentSession>,
        cx: &Context<Self>,
    ) -> Option<String> {
        let cwd = self.pane_cwd(id, session, cx)?;
        Some(match self.git_branch(&cwd, cx) {
            Some(b) => format!("{} · {b}", home_relative(&cwd)),
            None => home_relative(&cwd),
        })
    }
    /// The title's hover card lines about the conversation: what it was asked, and where
    /// it stopped in full (the question, the error, else the latest reply's first line).
    fn pane_story(&self, s: &AgentSession, g: Status) -> Vec<String> {
        let ask = self
            .workspace
            .content
            .get(&s.id)
            .and_then(|c| c.asks.first())
            .map(|q| {
                format!(
                    "你要它 {}",
                    mooring_core::content::short(q.text.trim(), 240)
                )
            });
        let at = self
            .attention_text(s, g)
            .map(|t| mooring_core::content::short(&mooring_core::content::plain_line(&t), 240))
            .or_else(|| self.row_detail(s, g))
            .map(|d| {
                if d.starts_with("停在") || d.starts_with("正在") {
                    d
                } else {
                    format!("停在 {d}")
                }
            });
        ask.into_iter().chain(at).collect()
    }
    /// Shown only while another window types into this pane: the way to take it back.
    fn pane_footer(&self, id: &str, takeover: String, cx: &Context<Self>) -> gpui::Div {
        div()
            .h(px(T::FOOTER))
            .flex_shrink_0()
            .px(px(T::S10))
            .flex()
            .items_center()
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .text_size(px(T::PANE_FOOTER_SIZE))
            .text_color(T::color(T::muted()))
            .child(div().flex_1().child("另一个窗口在输入"))
            .child(
                button(
                    gpui::SharedString::from(format!("takeover-{id}")),
                    "点击接管输入",
                )
                .flex_shrink_0()
                .h(px(T::ICON_HEIGHT))
                .px(px(T::S6))
                .text_size(px(T::PANE_FOOTER_SIZE))
                .text_color(T::color(T::link()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    let pane = cx.new(|cx| Pane::new(this.store.socket(), &takeover, true, cx));
                    window.focus(&pane.focus_handle(cx), cx);
                    Self::adopt_pane(&pane, cx);
                    this.layout.panes.insert(takeover.clone(), pane);
                    cx.notify();
                })),
            )
    }
}
pub(super) fn choose_icon(icon: AppIcon, cx: &mut gpui::App) {
    crate::desktop::set_dock_icon(icon.dock_png());
    let file = cx.global::<Chosen>().file.clone();
    if let Err(e) = icon.save(&file) {
        eprintln!("记住图标选择失败: {e}");
    }
    cx.set_global(Chosen { icon, file });
}
/// The pane menu's items in four groups (None draws a rule between them): layout,
/// text, the session and its todo, then what stops or closes something.
pub(super) fn menu_rows(zoomed: bool) -> Vec<Option<&'static str>> {
    vec![
        Some("向右分屏"),
        Some("向下分屏"),
        Some(if zoomed { "还原" } else { "放大" }),
        Some("移到标签…"),
        None,
        Some("复制"),
        Some("粘贴"),
        Some("搜索滚动内容"),
        Some("在编辑器里看滚动内容"),
        None,
        Some("为什么是这个状态"),
        Some("为这个待办开一个会话"),
        Some("挂到待办…"),
        Some("取消关联"),
        None,
        Some("中断（Ctrl-C）"),
        Some("停止 Agent"),
        Some("关闭画面"),
    ]
}
/// The rule between two groups of the pane menu.
pub(super) const MENU_RULE: f32 = T::S8 + T::BORDER_WIDTH;
fn menu_rule(i: usize) -> gpui::AnyElement {
    div()
        .id(gpui::SharedString::from(format!("menu-rule-{i}")))
        .w_full()
        .h(px(MENU_RULE))
        .flex()
        .items_center()
        .child(
            div()
                .w_full()
                .h(px(T::BORDER_WIDTH))
                .bg(T::color(T::border())),
        )
        .into_any_element()
}
/// The chord that runs a menu item without the menu, as the user has it bound; copy
/// and paste are the terminal's own fixed ⌘C and ⌘V.
fn menu_shortcut(title: &str, keys: &crate::keymap::Keymap) -> Option<String> {
    use crate::keymap::Action;
    let action = match title {
        "复制" => return Some("⌘C".into()),
        "粘贴" => return Some("⌘V".into()),
        "向右分屏" => Action::SplitRight,
        "向下分屏" => Action::SplitDown,
        "搜索滚动内容" => Action::Find,
        "关闭画面" => Action::Close,
        _ => return None,
    };
    keys.chords(action).first().map(|c| c.display())
}

#[cfg(test)]
mod tests {
    #[test]
    fn turn_clock_reads_minutes_and_hours() {
        assert_eq!(super::turn_clock(0), "0:00");
        assert_eq!(super::turn_clock(192), "3:12");
        assert_eq!(super::turn_clock(3792), "1:03:12");
    }
}
