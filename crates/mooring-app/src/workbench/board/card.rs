//! One card on the board: a todo not yet started, or a session.
use super::*;

impl Workbench {
    pub(in crate::workbench) fn todo_card(
        &self,
        task: &Task,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let id = task.id.clone();
        let (open, done, hover) = (id.clone(), id.clone(), id.clone());
        let sessions = self.task_sessions(&task.id);
        let needs = (sessions.iter())
            .filter(|s| self.workspace.needs_you(s, now()))
            .count();
        let lit = self.overview.hover_task.as_ref() == Some(&task.id);
        let space = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == task.space)
            .map_or("外部", |s| s.name.as_str());
        card_frame(gpui::SharedString::from(format!("board-todo-{id}")))
            .when(lit, |d| d.border_color(T::color(T::link())))
            .on_hover(cx.listener(move |this, on: &bool, _, cx| this.hover_task(&hover, *on, cx)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.overview.drawer = None;
                this.nav.task_detail = Some(open.clone());
                cx.notify();
            }))
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
                    // Who runs it, as on a session's card: the latest session's Agent once it
                    // has one, since that may not be the one it was written for.
                    .child(agent_mark(
                        sessions.last().map_or(task.agent, |s| s.runtime),
                        T::color(T::muted()),
                    ))
                    .when(sessions.is_empty(), |d| d.child("还没开始"))
                    // One thing on this row: what needs you if anything does, else how many run.
                    .when(!sessions.is_empty(), |d| {
                        d.child(div().min_w_0().truncate().when_else(
                            needs > 0,
                            |d| {
                                d.text_color(Status::Waiting.colors().1)
                                    .child(format!("{needs} 个{}", mooring_core::status::NEEDS_YOU))
                            },
                            |d| {
                                d.text_color(T::color(T::secondary()))
                                    .child(format!("{} 个会话在做", sessions.len()))
                            },
                        ))
                    })
                    // Here, not beside 「开始」: a 200 pt card can't hold both on one row.
                    .when(sessions.is_empty() && !task.acceptance.is_empty(), |d| {
                        d.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(format!("· {} 条验收", task.acceptance.len())),
                        )
                    })
                    .child(div().flex_1())
                    .when_some(
                        (self.workspace.task_logs.get(&task.id))
                            .and_then(|l| l.created)
                            // A started todo's first row has its sessions to say instead.
                            .filter(|_| sessions.is_empty()),
                        |d, at| d.child(short_ago(at, now())),
                    )
                    .child(
                        mesh(self.space_mesh(&task.space), AVATAR)
                            .flex_shrink_0()
                            .id(gpui::SharedString::from(format!("todo-space-{id}")))
                            .tooltip(super::diagnostics::tip(vec![space.to_owned()])),
                    ),
            )
            .child(card_title(task.title.clone()))
            .when_some(
                (self.workspace.task_logs.get(&task.id)).and_then(|l| l.notes.last()),
                |d, note| d.child(card_quote(note.text.clone())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .child(div().flex_1())
                    .child(
                        button(gpui::SharedString::from(format!("done-{id}")), "完成")
                            .flex_shrink_0()
                            .h(px(PILL + 4.))
                            .px(px(T::S8))
                            .border_0()
                            .text_color(T::color(T::secondary()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.complete_todo(&done, cx)
                            })),
                    )
                    .child({
                        let started = !sessions.is_empty();
                        button(
                            gpui::SharedString::from(format!("start-{id}")),
                            if started {
                                "再开一个"
                            } else {
                                "开始 →"
                            },
                        )
                        .flex_shrink_0()
                        .h(px(PILL + 4.))
                        .px(px(T::S10))
                        .bg(T::color(T::button()))
                        .text_color(T::color(T::primary()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.ask_agent(id.clone(), started, cx)
                        }))
                    }),
            )
            .children(self.agent_picker(&task.id, cx).map(|picker| {
                // A click elsewhere is an answer too: not now.
                picker
                    .id("board-agent-picker")
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .on_mouse_up_out(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.tasks.picking = None;
                            cx.notify();
                        }),
                    )
            }))
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

        let lit = session.task.is_some() && self.overview.hover_task == session.task;
        card_frame(gpui::SharedString::from(format!("card-{}", session.id)))
            .when(lit, |d| d.border_color(T::color(T::link())))
            .when_some(session.task.clone(), |d, t| {
                d.on_hover(cx.listener(move |this, on: &bool, _, cx| this.hover_task(&t, *on, cx)))
            })
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
            .when_some(quote, |d, q| d.child(card_quote(q)))
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
