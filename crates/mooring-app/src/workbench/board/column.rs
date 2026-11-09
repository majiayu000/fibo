//! A board column: its header and the list of cards it scrolls.
use super::*;

impl Workbench {
    pub(super) fn column(
        &self,
        column: Column,
        count: usize,
        cards: impl IntoElement,
        reviewable: bool,
        active: bool,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        use super::board_drawer::{DraggedCard, legal};
        // The slot stays above the virtual viewport, so it remains visible even
        // when the receiving column is scrolled away from its first item.
        let slot = (cx.has_active_drag()
            && self
                .overview
                .drop_over
                .is_some_and(|(over, from)| over == column && legal(from, column)))
        .then(|| {
            let spot = self.overview.spots.slot.clone();
            div()
                .relative()
                .flex_shrink_0()
                .mb(px(CARD_GAP))
                .child(
                    gpui::canvas(
                        move |bounds, _, _| spot.set(Some((column, bounds))),
                        |_, _, _, _| (),
                    )
                    .absolute()
                    .size_full(),
                )
                .rounded(px(T::CARD_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_dashed()
                .border_color(T::color(T::control()))
                .bg(T::color(T::raised()))
                .with_animation(
                    gpui::SharedString::from(format!("slot-{}", column.key())),
                    Animation::new(Duration::from_millis(260)).with_easing(gpui::ease_out_quint()),
                    |d, t| d.h(px(T::BOARD_SLOT * t)),
                )
        });
        div()
            .id(gpui::SharedString::from(format!("drop-{}", column.key())))
            .rounded(px(T::CARD_RADIUS))
            // From the moment a card is lifted every column says whether it takes it:
            // the ones that do are outlined, the rest fade back.
            .border(px(T::BORDER_WIDTH))
            .border_color(gpui::transparent_black())
            .when_some(self.dragging_from(cx), |d, from| {
                if from == column || legal(from, column) {
                    d.bg(T::color(T::raised()))
                        .border_color(T::color(T::card_border()))
                } else {
                    d.opacity(T::BOARD_REFUSED_OPACITY)
                }
            })
            .on_drag_move::<DraggedCard>(cx.listener(
                move |this, e: &gpui::DragMoveEvent<DraggedCard>, _, cx| {
                    this.follow_lift(e.event.position, column, e.bounds.size.width, cx);
                    let inside = e.bounds.contains(&e.event.position);
                    let from = e.drag(cx).from;
                    let now_over = this.overview.drop_over.map(|(c, _)| c);
                    if inside && now_over != Some(column) {
                        this.overview.drop_over = Some((column, from));
                        cx.notify();
                    } else if !inside && now_over == Some(column) {
                        this.overview.drop_over = None;
                        cx.notify();
                    }
                },
            ))
            .on_drop(cx.listener(move |this, card: &DraggedCard, _, cx| {
                // The board's own drop (let go between columns) must not also run.
                cx.stop_propagation();
                this.drop_card(card, column, cx)
            }))
            .flex_1()
            .min_w(px(COLUMN_MIN))
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(T::ROW + T::S6))
                    .px(px(T::S4))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .child({
                        let spin = active && count > 0;
                        if spin && matches!(column, Column::Running | Column::Needs) {
                            super::pace::wake(&MOTION, MOTION_STEP, cx);
                        }
                        animated_column_icon(column, spin)
                    })
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(T::color(T::primary()))
                            .child(column.label()),
                    )
                    .child(
                        div()
                            .text_color(T::color(T::muted()))
                            .child(count.to_string()),
                    )
                    .child(div().flex_1())
                    .when(
                        column == Column::Todo && self.tasks.board_quick.is_none(),
                        |d| {
                            d.child(
                                button(
                                    "overview-new-task",
                                    if self.tasks.board_draft.is_empty() {
                                        t!("＋ 新待办")
                                    } else {
                                        t!("＋ 新待办 · 有草稿")
                                    },
                                )
                                .h(px(T::SEGMENT_HEIGHT))
                                .px(px(T::S8))
                                .border_0()
                                .text_size(px(T::SMALL))
                                .text_color(T::color(T::secondary()))
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.show_quick_task(window, cx),
                                )),
                            )
                        },
                    )
                    .when(reviewable, |d| {
                        d.child(
                            button("review-queue", t!("逐个验收 →"))
                                .h(px(T::SEGMENT_HEIGHT))
                                .px(px(T::S8))
                                .text_color(T::color(T::link()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.start_review(now(), cx);
                                })),
                        )
                    })
                    .when(column == Column::Stale && count > 0, |d| {
                        d.child(
                            button("cleanup", t!("逐个清理 →"))
                                .h(px(T::SEGMENT_HEIGHT))
                                .px(px(T::S8))
                                .text_color(T::color(T::link()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let at = now();
                                    let queue = this
                                        .board_sessions(at)
                                        .into_iter()
                                        .filter(|(c, _)| *c == Column::Stale)
                                        .map(|(_, s)| s.id.clone())
                                        .collect();
                                    this.start_cleanup(queue, cx);
                                })),
                        )
                    }),
            )
            .children(slot)
            .when(column == Column::Todo, |d| {
                d.children(self.board_quick_view(cx))
            })
            .child(
                div()
                    .id(gpui::SharedString::from(format!("column-{}", column.key())))
                    .flex_1()
                    .min_h_0()
                    .child(cards),
            )
    }
    pub(super) fn card_list(
        &self,
        column: Column,
        ids: Vec<String>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let state = {
            let mut columns = self.overview.columns.borrow_mut();
            let cached = columns.entry(column).or_insert_with(|| ColumnList {
                state: gpui::ListState::new(
                    ids.len(),
                    gpui::ListAlignment::Top,
                    px(T::CARD_ESTIMATE),
                ),
                ids: ids.clone(),
                revision: self.workspace.revision,
            });
            cached.sync(&ids, self.workspace.revision);
            // Only now: a card added above where the list has scrolled to keeps the view
            // where it was, so scrolling before it lands would leave it out of sight.
            if column == Column::Todo
                && let Some(fresh) = &self.overview.fresh_todo
                && !fresh.revealed.get()
                && ids.contains(&fresh.id)
            {
                fresh.revealed.set(true);
                cached.state.scroll_to(gpui::ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.),
                });
            }
            cached.state.clone()
        };
        let count = ids.len();
        let entity = cx.entity();
        gpui::list(state, move |index, window, app| {
            let animate = window.is_window_active();
            entity.update(app, |this, cx| {
                let card = if column == Column::Todo {
                    let card = this.todo_card(
                        this.workspace
                            .tasks
                            .iter()
                            .find(|t| t.id == ids[index])
                            .expect("board row belongs to the current workspace"),
                        cx,
                    );
                    if (this.overview.fresh_todo.as_ref()).is_some_and(|f| f.id == ids[index]) {
                        super::composer::fresh_card(card, &ids[index]).into_any_element()
                    } else {
                        card.into_any_element()
                    }
                } else if let Some(id) = ids[index].strip_prefix("todo:") {
                    (this.workspace.tasks.iter())
                        .find(|t| t.id == id)
                        .map_or_else(
                            || div().into_any_element(),
                            |t| this.done_todo_card(t, cx).into_any_element(),
                        )
                } else if remote::is_remote(&ids[index]) {
                    (this.remote_board_card(&ids[index], cx))
                        .map_or_else(|| div().into_any_element(), |c| c.into_any_element())
                } else {
                    this.board_card(
                        this.workspace
                            .agent_sessions
                            .iter()
                            .find(|s| s.id == ids[index])
                            .expect("board row belongs to the current workspace"),
                        column,
                        now(),
                        animate,
                        cx,
                    )
                    .into_any_element()
                };
                let gap = if index + 1 < count { CARD_GAP } else { 0. };
                // The lifted card leaves its place empty, outlined, until it lands.
                let hole = ids
                    .get(index)
                    .is_some_and(|id| this.lifted(cx).is_some_and(|l| &l.id == id));
                div()
                    .relative()
                    .pb(px(gap))
                    .child(card)
                    .when(hole, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .bottom(px(gap))
                                .rounded(px(T::CARD_RADIUS))
                                .bg(T::color(board_surface(app_style(cx))()))
                                .border(px(T::BORDER_WIDTH))
                                .border_dashed()
                                .border_color(T::color(T::control())),
                        )
                    })
                    .into_any_element()
            })
        })
        .size_full()
        .pb(px(T::S20 * 2.))
    }
}
