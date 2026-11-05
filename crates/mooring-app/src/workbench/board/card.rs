//! The cards of the board: a todo, and this machine's sessions.
use super::*;

/// How long it has waited, as four rising bars that fill and warm up with the wait.
fn wait_bars(waited: u64, hue: Hsla) -> gpui::Div {
    let filled = match waited {
        0..300 => 1,
        300..1800 => 2,
        1800..3600 => 3,
        _ => 4,
    };
    let lit = if filled >= 3 {
        hue
    } else {
        T::color(T::secondary())
    };
    div()
        .size(px(PILL))
        .flex_shrink_0()
        .flex()
        .items_end()
        .justify_center()
        .gap(px(T::BOARD_BAR_GAP))
        .pb(px(T::BOARD_BAR_INSET))
        .rounded(px(T::BOARD_BAR_FRAME_RADIUS))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::card_border()))
        .children((0..4).map(|i| {
            div()
                .w(px(T::BOARD_BAR_WIDTH))
                .h(px(T::BOARD_BAR_BASE + T::BOARD_BAR_STEP * i as f32))
                .rounded(px(T::BOARD_BAR_RADIUS))
                .bg(if i < filled {
                    lit
                } else {
                    T::color(T::border())
                })
        }))
}

impl Workbench {
    pub(in crate::workbench) fn todo_card(
        &self,
        task: &Task,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let id = task.id.clone();
        let space = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == task.space)
            .map_or("外部", |s| s.name.as_str());
        card_frame(gpui::SharedString::from(format!("board-todo-{id}")))
            .on_drag(
                super::board_drawer::DraggedCard {
                    id: id.clone(),
                    title: task.title.clone(),
                    from: Column::Todo,
                },
                super::board_drawer::lift_card(cx.entity().downgrade()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .child(agent_mark(task.agent, T::color(T::muted())))
                    .child("还没开始")
                    .child(div().flex_1())
                    .child(
                        mesh(self.space_mesh(&task.space), AVATAR)
                            .id(gpui::SharedString::from(format!("todo-space-{id}")))
                            .tooltip(super::diagnostics::tip(vec![space.to_owned()])),
                    ),
            )
            .child(card_title(task.title.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .child(pill(
                        task.agent.name(),
                        T::color(T::card_border()),
                        None,
                        T::color(T::body()),
                    ))
                    .child(
                        div()
                            .text_size(px(T::META))
                            .text_color(T::color(T::muted()))
                            .child(format!("{} 条验收", task.acceptance.len())),
                    )
                    .child(div().flex_1())
                    .child(
                        button(gpui::SharedString::from(format!("start-{id}")), "开始 →")
                            .h(px(PILL + 4.))
                            .px(px(T::S10))
                            .bg(T::color(T::button()))
                            .text_color(T::color(T::primary()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ask_agent(id.clone(), false, cx)
                            })),
                    ),
            )
            .children(self.agent_picker(&task.id, cx))
    }
    pub(in crate::workbench) fn board_card(
        &self,
        session: &AgentSession,
        column: Column,
        now: u64,
        animate: bool,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let enter = session.clone();
        let menu_id = session.id.clone();
        // 要你处理's cards say what they wait on: the status, its age and the question.
        let asks_you = (column == Column::Needs).then(|| status_of(&self.workspace, session));
        let content = self.workspace.content.get(&session.id);
        let space_id = self.workspace.session_space(session).unwrap_or("外部");
        let space_name = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == space_id)
            .map_or("外部", |s| s.name.as_str());
        // The id's tail: ids share prefixes (dates, `visual-`), their ends differ.
        let alnum: Vec<char> = session
            .cli_session_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        let short: String = alnum[alnum.len().saturating_sub(4)..]
            .iter()
            .collect::<String>()
            .to_uppercase();
        let running_since = (column == Column::Running && asks_you.is_none())
            .then(|| self.turn_started(session))
            .flatten();
        let when = match running_since {
            Some(at) => super::layout::turn_clock(now.saturating_sub(at)),
            None => short_ago(self.session_updated(session), now),
        };
        let branch = self.git_branch(&session.cwd, cx);
        let asks = content.map_or(0, |c| c.asks.len());
        let hover_group = gpui::SharedString::from(format!("board-card-{}", session.id));

        let status_pill = |g: Status| {
            let (dot, ink, _) = g.colors();
            pill(g.label(), dot.opacity(0.42), Some(dot), ink)
        };
        let branch_pill = branch.as_ref().map(|b| {
            let hue = branch_hue(b).map_or(T::color(T::muted()), T::color);
            name_pill(
                b.rsplit('/').next().unwrap_or(b).to_owned(),
                hue.opacity(0.4),
                Some(hue),
                T::color(T::body()),
            )
        });
        let mut left: Vec<gpui::AnyElement> = vec![];
        match (column, asks_you) {
            (_, Some(group)) => {
                let (dot, _, _) = group.colors();
                left.push(
                    wait_bars(now.saturating_sub(self.session_updated(session)), dot)
                        .into_any_element(),
                );
                left.push(status_pill(group).into_any_element());
            }
            (Column::Stale, None) => left.push(status_pill(Status::Stale).into_any_element()),
            (Column::Review, None) if status_of(&self.workspace, session) == Status::Paused => {
                left.push(status_pill(Status::Paused).into_any_element());
                left.extend(branch_pill.map(|p| p.into_any_element()));
            }
            _ => left.extend(branch_pill.map(|p| p.into_any_element())),
        }
        let title = self.session_title(session);
        // What it asks, unless that only repeats the title (an interrupted turn says nothing).
        let quote = asks_you
            .map(|_| self.session_line(session))
            .filter(|q| !q.trim().is_empty() && q.trim() != title.trim());

        card_frame(gpui::SharedString::from(format!("card-{}", session.id)))
            .group(hover_group.clone())
            .aria_label(gpui::SharedString::from(format!(
                "会话：{}",
                self.session_title(session)
            )))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.overview.card_menu = None;
                this.overview.drawer = Some(enter.id.clone());
                cx.notify();
            }))
            .on_drag(
                super::board_drawer::DraggedCard {
                    id: session.id.clone(),
                    title: title.clone(),
                    from: column,
                },
                super::board_drawer::lift_card(cx.entity().downgrade()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .child(agent_mark(session.runtime, T::color(T::muted())))
                    .child(div().text_color(T::color(T::secondary())).child(short))
                    .child(
                        svg()
                            .path("icons/clock.svg")
                            .size(px(T::BOARD_CLOCK))
                            .text_color(T::color(T::muted())),
                    )
                    .child(when)
                    .child(div().flex_1())
                    .child(
                        button(
                            gpui::SharedString::from(format!("more-{}", session.id)),
                            "⋯",
                        )
                        .h(px(AVATAR))
                        .px(px(T::S4))
                        .text_color(T::color(T::muted()))
                        .invisible()
                        .group_hover(hover_group.clone(), |s| s.visible())
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
                    )
                    .child(
                        mesh(self.space_mesh(space_id), AVATAR)
                            .id(gpui::SharedString::from(format!("space-{}", session.id)))
                            .tooltip(super::diagnostics::tip(vec![space_name.to_owned()])),
                    ),
            )
            .child(card_title(title.clone()))
            .when_some(quote, |d, q| {
                d.child(
                    div()
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::secondary()))
                        .line_clamp(1)
                        .child(q),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .children(left)
                    .child(div().flex_1())
                    .when(column == Column::Done, |d| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(T::META))
                                .text_color(Status::Closed.colors().1)
                                .child(format!("✓ {}", Status::Closed.label())),
                        )
                    })
                    .when(asks > 0, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap(px(T::S4))
                                .text_size(px(T::META))
                                .text_color(T::color(T::muted()))
                                .child(
                                    svg()
                                        .path("icons/turn.svg")
                                        .size(px(MARK))
                                        .text_color(T::color(T::muted())),
                                )
                                .child(asks.to_string()),
                        )
                    }),
            )
            .when(column == Column::Running && animate, |d| {
                super::pace::wake(&MOTION, MOTION_STEP, cx);
                d.child(sweep())
            })
            .when(self.overview.card_menu.as_ref() == Some(&session.id), |d| {
                d.child(
                    gpui::deferred(
                        self.session_actions(session, cx)
                            .absolute()
                            .top(px(T::ROW + T::S6))
                            .right(px(T::S8))
                            .occlude(),
                    )
                    .priority(1),
                )
            })
            .when(self.overview.here.as_ref() == Some(&session.id), |d| {
                d.child(self.here_card(session, false, cx))
            })
    }
}

pub(super) fn card_frame(id: gpui::SharedString) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::accesskit::Role::Button)
        .relative()
        .flex_shrink_0()
        .px(px(T::S12))
        .py(px(T::S10))
        .rounded(px(T::CARD_RADIUS))
        .bg(T::color(T::popover()))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::card_border()))
        .flex()
        .flex_col()
        .gap(px(T::BOARD_CARD_GAP))
        .cursor_pointer()
        .hover(|s| s.border_color(T::color(T::control())))
}
pub(super) fn card_title(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .text_size(px(T::CARD_TITLE))
        .line_height(relative(1.4))
        .text_color(T::color(T::primary()))
        .line_clamp(2)
        .child(text.into())
}
/// A comet of the running hue that runs along a working card's bottom edge.
fn sweep() -> gpui::Div {
    let hue = Status::Running.colors().0;
    div()
        .absolute()
        .bottom_0()
        .left(px(T::S10))
        .right(px(T::S10))
        .h(px(T::BORDER_WIDTH))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(0.4))
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(hue.opacity(0.), 0.),
                    linear_color_stop(hue, 1.),
                ))
                .left(relative(-0.4 + 1.4 * super::pace::phase(SWEEP))),
        )
}
