//! The board's moving parts: a card's detail drawer, dragging a card where dropping it is
//! an action, and the note at the bottom that says what happened, with 撤销.
use super::board::Column;
use super::*;
use gpui::{Animation, AnimationExt};
use std::time::Duration;

mod drawer;

/// What a card carries while it is dragged: a session, or a todo not handed out yet.
#[derive(Clone)]
pub(super) struct DraggedCard {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) from: Column,
}
impl gpui::Render for DraggedCard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // GPUI's own drag image stays empty: the board draws the lifted card itself
        // (Workbench::lift_view), whole and at the card's width, so it can also fly it
        // into its slot or back home.
        div()
    }
}

/// A card picked up off the board, in window coordinates: where the cursor holds it,
/// where it came from, and, once let go, where it is flying.
#[derive(Clone)]
pub(super) struct Lift {
    pub(super) id: String,
    pub(super) from: Column,
    /// The cursor's offset inside the card.
    pub(super) offset: gpui::Point<gpui::Pixels>,
    /// The card's top-left when it was picked up; a refused drop flies back here.
    pub(super) origin: gpui::Point<gpui::Pixels>,
    pub(super) width: gpui::Pixels,
    pub(super) cursor: gpui::Point<gpui::Pixels>,
    pub(super) landing: Option<Landing>,
}
/// The flight after the drop: from where it was let go to its slot or back home.
#[derive(Clone)]
pub(super) struct Landing {
    pub(super) seq: u64,
    pub(super) from: gpui::Point<gpui::Pixels>,
    pub(super) to: gpui::Point<gpui::Pixels>,
}
/// The column whose slot is open, and where that slot was drawn.
type SlotSpot = (Column, gpui::Bounds<gpui::Pixels>);
/// Where the board and the open slot were drawn, written while painting.
#[derive(Clone, Default)]
pub(super) struct BoardSpots {
    pub(super) board: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    pub(super) slot: std::rc::Rc<std::cell::Cell<Option<SlotSpot>>>,
}
const LIFT_IN: Duration = Duration::from_millis(180);
const LANDING: Duration = Duration::from_millis(380);
const SHAKE: Duration = Duration::from_millis(380);

/// Starts a lift when GPUI starts dragging a card: the board notes which card, and
/// where the cursor holds it, so it can leave a hole and draw the card under the cursor.
pub(super) fn lift_card(
    entity: gpui::WeakEntity<Workbench>,
) -> impl Fn(&DraggedCard, gpui::Point<gpui::Pixels>, &mut Window, &mut gpui::App) -> Entity<DraggedCard>
+ 'static {
    move |card, offset, window, cx| {
        let cursor = window.mouse_position();
        let (id, from) = (card.id.clone(), card.from);
        entity
            .update(cx, |this, cx| {
                this.overview.lift = Some(Lift {
                    id,
                    from,
                    offset,
                    origin: cursor - offset,
                    width: this
                        .overview
                        .column_width
                        .unwrap_or(px(T::BOARD_COLUMN_MIN)),
                    cursor,
                    landing: None,
                });
                cx.notify();
            })
            .ok();
        cx.new(|_| card.clone())
    }
}

/// Dropping is an action, so only these moves exist: start a todo, mark a stopped
/// session done, or send done work back.
pub(super) fn legal(from: Column, to: Column) -> bool {
    matches!(
        (from, to),
        (Column::Todo, Column::Running)
            | (Column::Needs, Column::Done)
            | (Column::Review, Column::Done)
            | (Column::Done, Column::Needs)
    )
}
/// Why a card cannot go where it was dropped, named by where it came from.
pub(super) fn refusal(from: Column) -> String {
    let (needs, done) = (
        mooring_core::status::needs_you_word(),
        Status::Closed.label(),
    );
    match from {
        Column::Needs => t!("{needs}只能拖到「{done}」，表示做完了", needs, done),
        Column::Running => t!(
            "{label}的状态由 Agent 自己决定",
            label = Status::Running.label()
        ),
        Column::Review => t!(
            "{label}只能拖到「{done}」",
            label = Status::Paused.label(),
            done
        ),
        Column::Todo => t!(
            "待办只能拖到「{label}」来开始",
            label = Status::Running.label()
        ),
        Column::Done => t!("{done}只能拖回「{needs}」", done, needs),
        Column::Stale => t!(
            "{label}的会话用「逐个清理」处理",
            label = Status::Stale.label()
        ),
    }
}

/// The note at the bottom of the board; `undo` puts that outcome back on that session.
#[derive(Clone)]
pub(super) struct Toast {
    pub(super) seq: u64,
    pub(super) text: String,
    pub(super) undo: Option<(String, Outcome)>,
    pub(super) refused: bool,
}
const TOAST_FOR: Duration = Duration::from_millis(4200);

/// Overshoots a little and settles, like a spring.
fn spring(t: f32) -> f32 {
    let (c1, c3) = (1.4_f32, 2.4_f32);
    1. + c3 * (t - 1.).powi(3) + c1 * (t - 1.).powi(2)
}

impl Workbench {
    pub(super) fn show_toast(
        &mut self,
        text: String,
        undo: Option<(String, Outcome)>,
        refused: bool,
        cx: &mut Context<Self>,
    ) {
        self.overview.toast_seq += 1;
        let seq = self.overview.toast_seq;
        self.overview.toast = Some(Toast {
            seq,
            text,
            undo,
            refused,
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TOAST_FOR).await;
            let _ = this.update(cx, |this, cx| {
                if this.overview.toast.as_ref().is_some_and(|t| t.seq == seq) {
                    this.overview.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn drop_card(&mut self, card: &DraggedCard, to: Column, cx: &mut Context<Self>) {
        self.overview.drop_over = None;
        // Where the card lands: its slot at the top of the column it was accepted into,
        // else back where it was picked up.
        let slot = (self.overview.spots.slot.get()).filter(|(c, _)| *c == to);
        let slot = slot.map(|(_, b)| b.origin);
        let accepted = legal(card.from, to) && to != Column::Running;
        self.land(if accepted { slot } else { None }, cx);
        if card.from == to {
            cx.notify();
            return;
        }
        if !legal(card.from, to) {
            self.shake(to, cx);
            self.show_toast(refusal(card.from), None, true, cx);
            return;
        }
        if super::board::remote::is_remote(&card.id) {
            return self.settle_remote(card, to, cx);
        }
        let (outcome, before, text) = match to {
            Column::Running => {
                self.ask_agent(card.id.clone(), false, cx);
                self.show_toast(
                    t!("选一个 Agent 开始「{title}」", title = card.title),
                    None,
                    false,
                    cx,
                );
                return;
            }
            Column::Done => (
                Outcome::Completed,
                Outcome::Open,
                t!(
                    "「{title}」移到{label}",
                    title = card.title,
                    label = Status::Closed.label()
                ),
            ),
            _ => (
                Outcome::Open,
                Outcome::Completed,
                t!(
                    "「{title}」退回{needs_you_word}",
                    title = card.title,
                    needs_you_word = mooring_core::status::needs_you_word()
                ),
            ),
        };
        self.edit(
            Edit::SetOutcome {
                seen: None,
                id: card.id.clone(),
                outcome,
            },
            cx,
        );
        self.show_toast(text, Some((card.id.clone(), before)), false, cx);
    }
    /// Let go somewhere that is not a column: the card flies back home.
    pub(super) fn drop_off_board(&mut self, cx: &mut Context<Self>) {
        self.overview.drop_over = None;
        self.land(None, cx);
    }
    /// Flies the lifted card from under the cursor to `to`, or home, then lets it go.
    fn land(&mut self, to: Option<gpui::Point<gpui::Pixels>>, cx: &mut Context<Self>) {
        let Some(lift) = self.overview.lift.as_mut() else {
            return;
        };
        self.overview.lift_seq += 1;
        let seq = self.overview.lift_seq;
        lift.landing = Some(Landing {
            seq,
            from: lift.cursor - lift.offset,
            to: to.unwrap_or(lift.origin),
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LANDING).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .overview
                    .lift
                    .as_ref()
                    .and_then(|l| l.landing.as_ref())
                    .is_some_and(|l| l.seq == seq)
                {
                    this.overview.lift = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// The column that refused a card shakes its head.
    fn shake(&mut self, column: Column, cx: &mut Context<Self>) {
        self.overview.lift_seq += 1;
        let seq = self.overview.lift_seq;
        self.overview.shake = Some((column, seq));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SHAKE).await;
            let _ = this.update(cx, |this, cx| {
                if this.overview.shake.is_some_and(|(_, s)| s == seq) {
                    this.overview.shake = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// The card being dragged or flying after its drop: its hole stays open meanwhile.
    pub(super) fn lifted(&self, cx: &gpui::App) -> Option<&Lift> {
        self.overview
            .lift
            .as_ref()
            .filter(|l| l.landing.is_some() || cx.has_active_drag())
    }
    /// The column a card is being dragged out of, while the drag lasts.
    pub(super) fn dragging_from(&self, cx: &gpui::App) -> Option<Column> {
        self.lifted(cx)
            .filter(|l| l.landing.is_none())
            .map(|l| l.from)
    }
    /// Notes the cursor, and the width a lifted card is drawn at, on every drag move.
    pub(super) fn follow_lift(
        &mut self,
        cursor: gpui::Point<gpui::Pixels>,
        column: Column,
        width: gpui::Pixels,
        cx: &mut Context<Self>,
    ) {
        self.overview.column_width = Some(width);
        if let Some(lift) = self.overview.lift.as_mut().filter(|l| l.landing.is_none()) {
            if lift.from == column {
                lift.width = width;
            }
            if lift.cursor != cursor {
                lift.cursor = cursor;
                cx.notify();
            }
        }
    }
    /// The whole card, drawn above the board under the cursor or in flight.
    pub(super) fn lift_view(&self, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        let lift = self.lifted(cx)?;
        let card = if lift.from == Column::Todo {
            let task = self.workspace.tasks.iter().find(|t| t.id == lift.id)?;
            self.todo_card(task, cx)
        } else if super::board::remote::is_remote(&lift.id) {
            self.remote_board_card(&lift.id, cx)?
        } else {
            let session = (self.workspace.agent_sessions.iter()).find(|s| s.id == lift.id)?;
            self.board_card(session, lift.from, now(), false, cx)
        };
        let board = self.overview.spots.board.get().origin;
        let card = card
            .w(lift.width)
            .bg(T::color(T::raised()))
            .border_color(T::color(T::control()))
            .shadow_lg();
        let ghost = div().id("board-lift").absolute().w(lift.width).child(card);
        Some(match lift.landing.clone() {
            None => {
                let at = lift.cursor - lift.offset - board;
                ghost
                    .left(at.x)
                    .top(at.y)
                    .with_animation(
                        gpui::SharedString::from(format!("board-lift-{}", lift.id)),
                        Animation::new(LIFT_IN).with_easing(spring),
                        |d, t| d.opacity(0.6 + 0.4 * t.clamp(0., 1.)),
                    )
                    .into_any_element()
            }
            Some(l) => {
                let (from, to) = (l.from - board, l.to - board);
                ghost
                    .with_animation(
                        gpui::SharedString::from(format!("board-land-{}", l.seq)),
                        Animation::new(LANDING).with_easing(spring),
                        move |d, t| {
                            d.left(from.x + (to.x - from.x) * t)
                                .top(from.y + (to.y - from.y) * t)
                        },
                    )
                    .into_any_element()
            }
        })
    }
    /// Wraps a column so it shakes after refusing a card.
    pub(super) fn shaking(
        &self,
        column: Column,
        element: gpui::Stateful<gpui::Div>,
    ) -> gpui::AnyElement {
        match self.overview.shake.filter(|(c, _)| *c == column) {
            Some((_, seq)) => element
                .relative()
                .with_animation(
                    gpui::SharedString::from(format!("board-shake-{seq}")),
                    Animation::new(SHAKE),
                    |d, t| {
                        let swing = (t * std::f32::consts::PI * 6.).sin() * (1. - t);
                        d.left(px(T::S6 * swing))
                    },
                )
                .into_any_element(),
            None => element.into_any_element(),
        }
    }
    pub(super) fn toast_view(&self, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        let toast = self.overview.toast.clone()?;
        let hue = if toast.refused {
            Tone::Warn.dot()
        } else {
            Tone::Ok.dot()
        };
        let note = div()
            .id(gpui::SharedString::from(format!(
                "board-toast-{}",
                toast.seq
            )))
            .role(gpui::accesskit::Role::Status)
            .relative()
            .h(px(T::ROW + T::S10))
            .pl(px(T::S14))
            .pr(px(T::S6))
            .flex()
            .items_center()
            .gap(px(T::S12))
            .rounded(px(T::CONTROL_RADIUS))
            .bg(T::color(T::raised()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            .shadow_lg()
            .text_size(px(T::SMALL))
            .text_color(T::color(T::primary()))
            .child(div().size(px(T::DOT)).rounded_full().bg(hue))
            .child(toast.text.clone())
            .when_some(toast.undo.clone(), |d, (id, outcome)| {
                d.child(
                    button("board-undo", t!("撤销"))
                        .h(px(T::SEGMENT_HEIGHT))
                        .bg(T::color(T::button()))
                        .text_color(T::color(T::primary()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.overview.toast = None;
                            this.edit(
                                Edit::SetOutcome {
                                    seen: None,
                                    id: id.clone(),
                                    outcome: outcome.clone(),
                                },
                                cx,
                            );
                        })),
                )
            })
            .with_animation(
                gpui::SharedString::from(format!("board-toast-in-{}", toast.seq)),
                Animation::new(Duration::from_millis(420)).with_easing(spring),
                |d, t| d.top(px(T::S20 * (1. - t))).opacity(t.clamp(0., 1.)),
            );
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(T::S20))
                .flex()
                .justify_center()
                .child(note)
                .into_any_element(),
        )
    }

    /// The drawer's session, still on the board.
    fn drawer_session(&self) -> Option<(Column, &AgentSession)> {
        let id = self.overview.drawer.as_ref()?;
        let at = now();
        self.board_sessions(at)
            .into_iter()
            .find(|(_, s)| &s.id == id)
    }
    /// The next or previous card in the drawer's column, wrapping around.
    pub(super) fn step_drawer(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some((column, session)) = self.drawer_session() else {
            return;
        };
        let list = self.column_sessions(column, now());
        let Some(i) = list.iter().position(|s| s.id == session.id) else {
            return;
        };
        let n = list.len() as isize;
        let next = list[((i as isize + by) % n + n) as usize % list.len()]
            .id
            .clone();
        self.overview.drawer = Some(next);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
