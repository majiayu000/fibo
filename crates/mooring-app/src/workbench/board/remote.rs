//! Other machines' sessions on the board, mixed into the same columns as this machine's
//! (docs/multi-machine/spec.md, step 4; direction A, chosen 11-04): a card names its
//! machine, opens that terminal in the peek, and is dragged to 做完了 and back like a
//! card of this machine, the outcome going to that machine's host. Which column, what
//! state and which words all come from that host's card, never decided again here; a
//! machine that dropped off keeps its last cards, faded, and they cannot be moved.
use super::super::board_drawer::{DraggedCard, lift_card};
use super::super::machines::shown_title;
use super::*;
use crate::machines::Link;
use mooring_core::protocol::RequestId;
use mooring_remote::protocol::{Card, Op};

const PREFIX: &str = "remote:";

/// One card of another machine as the board places it.
#[derive(Clone)]
pub(in crate::workbench) struct RemoteCard {
    /// Which of `Workbench::machines`.
    pub(in crate::workbench) index: usize,
    pub(in crate::workbench) machine: String,
    pub(in crate::workbench) online: bool,
    pub(in crate::workbench) card: Card,
    pub(in crate::workbench) column: Column,
}

/// A board id for machine `index`'s terminal: never equal to a session id of this one.
pub(in crate::workbench) fn remote_id(index: usize, terminal: &str) -> String {
    format!("{PREFIX}{index}:{terminal}")
}
fn parse(id: &str) -> Option<(usize, &str)> {
    let (index, terminal) = id.strip_prefix(PREFIX)?.split_once(':')?;
    Some((index.parse().ok()?, terminal))
}
pub(in crate::workbench) fn is_remote(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// The column another machine's card goes in, read off what its host said: the outcome,
/// the state and whether it is the user's to handle (`needs_you`), as `column_of` places
/// this machine's. A terminal with no conversation stays off the board.
pub(in crate::workbench) fn remote_column(card: &Card) -> Option<Column> {
    card.session.as_ref()?;
    match card.outcome.as_deref() {
        Some("done") => return Some(Column::Done),
        Some("abandoned" | "deferred") => return None,
        _ => {}
    }
    Some(match Status::from_key(card.status.as_deref()?)? {
        Status::Fresh | Status::Closed | Status::Deferred => return None,
        Status::Running => Column::Running,
        _ if card.needs_you => Column::Needs,
        Status::Stale => Column::Stale,
        _ => Column::Review,
    })
}

/// In 要你处理 what holds a live question comes first, then what waited longest;
/// elsewhere the latest first. Ties keep the order given.
fn order(column: Column, keyed: &mut [(bool, u64, String)]) {
    if column == Column::Needs {
        keyed.sort_by_key(|(asks, at, _)| (!*asks, *at));
    } else {
        keyed.sort_by_key(|(_, at, _)| std::cmp::Reverse(*at));
    }
}

impl Workbench {
    /// Every other machine's card the board shows now. A filter for this machine's
    /// spaces, changes or closed sessions shows none of them; the Agent filter applies.
    pub(in crate::workbench) fn remote_cards(&self) -> Vec<RemoteCard> {
        let o = &self.overview;
        if o.filter_space.is_some() || o.changed_only || o.closed_only {
            return Vec::new();
        }
        (0..self.machines.len())
            .flat_map(|index| self.machine_cards(index))
            .filter(|r| {
                o.agent.is_none_or(|a| {
                    (r.card.agent.as_deref()).is_some_and(|k| k.eq_ignore_ascii_case(a.key()))
                })
            })
            .collect()
    }
    fn machine_cards(&self, index: usize) -> Vec<RemoteCard> {
        let view = self.machines[index].view.lock().unwrap();
        let machine =
            (view.machine.as_ref()).map_or_else(|| view.spec.host.clone(), |m| m.name.clone());
        let online = view.link == Link::Online;
        (view.terminals.iter())
            .filter_map(|card| {
                Some(RemoteCard {
                    index,
                    machine: machine.clone(),
                    online,
                    card: card.clone(),
                    column: remote_column(card)?,
                })
            })
            .collect()
    }
    fn remote_card_at(&self, id: &str) -> Option<RemoteCard> {
        let (index, terminal) = parse(id)?;
        (index < self.machines.len())
            .then(|| self.machine_cards(index))?
            .into_iter()
            .find(|r| r.card.id == terminal)
    }
    /// One column's ids, this machine's sessions (already in board order) and other
    /// machines' cards together, by the same rule `sort_column` uses.
    pub(in crate::workbench) fn mixed_ids(
        &self,
        column: Column,
        local: &[&AgentSession],
        remote: &[RemoteCard],
    ) -> Vec<String> {
        let mut keyed: Vec<(bool, u64, String)> = (local.iter())
            .map(|s| {
                let asks = needs_attention(&self.workspace, s);
                (asks, self.session_updated(s), s.id.clone())
            })
            .collect();
        keyed.extend(remote.iter().filter(|r| r.column == column).map(|r| {
            let asks = r.card.approval.is_some() || r.card.waiting.is_some();
            let at = r.card.updated.unwrap_or(0);
            (asks, at, remote_id(r.index, &r.card.id))
        }));
        order(column, &mut keyed);
        keyed.into_iter().map(|(_, _, id)| id).collect()
    }

    /// The board's card for `id`, or nothing once that machine stopped listing it.
    pub(in crate::workbench) fn remote_board_card(
        &self,
        id: &str,
        cx: &Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        Some(self.remote_card_view(self.remote_card_at(id)?, cx))
    }
    fn remote_card_view(&self, r: RemoteCard, cx: &Context<Self>) -> gpui::Stateful<gpui::Div> {
        let id = remote_id(r.index, &r.card.id);
        let title = shown_title(&r.card);
        let status = (r.card.status.as_deref()).and_then(Status::from_key);
        let agent = (r.card.agent.as_deref()).and_then(|k| {
            Agent::all()
                .into_iter()
                .find(|a| a.key().eq_ignore_ascii_case(k))
        });
        // What it asks, unless that only repeats the title.
        let quote = (r.column == Column::Needs)
            .then(|| r.card.approval.clone().or_else(|| r.card.waiting.clone()))
            .flatten()
            .filter(|q| !q.trim().is_empty() && q.trim() != title.trim());
        let when = r.card.updated.map(|at| short_ago(at, now()));
        let movable = r.online && self.machine_manages(r.index);
        let open = (r.index, r.card.clone(), r.machine.clone());
        let link = T::color(T::link());
        let mut frame = card_frame(gpui::SharedString::from(format!("card-{id}")))
            .aria_label(gpui::SharedString::from(format!(
                "{} 上的会话：{title}",
                r.machine
            )))
            .when(!r.online, |d| d.opacity(T::BOARD_REFUSED_OPACITY))
            .on_click(cx.listener(move |this, _, window, cx| {
                let (index, card, name) = open.clone();
                this.overview.card_menu = None;
                this.open_remote_peek(index, card, &name, window, cx);
            }));
        if movable {
            let dragged = DraggedCard {
                id: id.clone(),
                title: title.clone(),
                from: r.column,
            };
            frame = frame.on_drag(dragged, lift_card(cx.entity().downgrade()));
        }
        let state = match (r.online, r.column, status) {
            (false, _, _) => Some(
                div()
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .child("断开")
                    .into_any_element(),
            ),
            (_, Column::Done, _) => Some(
                div()
                    .text_size(px(T::META))
                    .text_color(Status::Closed.colors().1)
                    .child(format!("✓ {}", Status::Closed.label()))
                    .into_any_element(),
            ),
            (_, _, Some(g)) => {
                let (dot, ink, _) = g.colors();
                let word = r
                    .card
                    .status_label
                    .clone()
                    .unwrap_or_else(|| g.label().into());
                Some(pill(word, dot.opacity(0.42), Some(dot), ink).into_any_element())
            }
            _ => None,
        };
        frame
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .when_some(agent, |d, a| d.child(agent_mark(a, T::color(T::muted()))))
                    .child(name_pill(r.machine.clone(), link.opacity(0.4), None, link))
                    .when_some(when, |d, when| {
                        d.child(
                            svg()
                                .path("icons/clock.svg")
                                .size(px(T::BOARD_CLOCK))
                                .text_color(T::color(T::muted())),
                        )
                        .child(when)
                    }),
            )
            .child(card_title(title))
            .when_some(quote, |d, q| {
                d.child(
                    div()
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::secondary()))
                        .line_clamp(1)
                        .child(q),
                )
            })
            .child(div().flex().items_center().gap(px(T::S6)).children(state))
    }
    /// Whether machine `index` is linked now and lets this desktop open, end and settle.
    pub(in crate::workbench) fn machine_manages(&self, index: usize) -> bool {
        (self.machines.get(index)).is_some_and(|m| {
            m.slot()
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|l| l.managing)
        })
    }

    /// A remote card dropped on 做完了, or back on 要你处理: the outcome goes to that
    /// machine's host with the turn the card showed, so an Agent that began another turn
    /// meanwhile keeps its card. The card moves at once; that machine's next list keeps
    /// it there or puts it back.
    pub(in crate::workbench) fn settle_remote(
        &mut self,
        card: &DraggedCard,
        to: Column,
        cx: &mut Context<Self>,
    ) {
        let Some(r) = self.remote_card_at(&card.id) else {
            return;
        };
        let Some(session) = r.card.session.clone() else {
            return;
        };
        let linked = self.machines[r.index].slot().lock().unwrap().clone();
        let Some(linked) = linked.filter(|l| l.managing) else {
            let text = format!("{}没连上，或那边的 Mooring 太旧，不能在这里标", r.machine);
            self.show_toast(text, None, true, cx);
            return;
        };
        let (outcome, text) = if to == Column::Done {
            (
                "done",
                format!("「{}」移到{}", card.title, Status::Closed.label()),
            )
        } else {
            let needs = mooring_core::status::NEEDS_YOU;
            ("open", format!("「{}」退回{needs}", card.title))
        };
        {
            let mut view = self.machines[r.index].view.lock().unwrap();
            if let Some(c) = view.terminals.iter_mut().find(|c| c.id == r.card.id) {
                c.outcome = Some(outcome.to_owned());
                c.needs_you = outcome == "open";
            }
        }
        self.show_toast(format!("{text}（{}）", r.machine), None, false, cx);
        let op = Op::Outcome {
            session,
            outcome: outcome.to_owned(),
            turn: r.card.turn.clone(),
            request: Some(RequestId::fresh().id),
        };
        let sent = cx.background_spawn(async move {
            linked
                .client
                .request(op, std::time::Duration::from_secs(10))
        });
        let machine = r.machine;
        cx.spawn(async move |this, cx| {
            if let Err(e) = sent.await {
                let _ = this.update(cx, |this, cx| {
                    this.show_toast(format!("{machine}没标上：{e}"), None, true, cx);
                });
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(status: &str, needs_you: bool, outcome: &str) -> Card {
        serde_json::from_value(serde_json::json!({
            "id": "t", "space": "s", "tab": "", "cwd": "/", "phase": "idle",
            "session": "s1", "status": status, "needs_you": needs_you, "outcome": outcome,
        }))
        .unwrap()
    }

    #[test]
    fn a_remote_card_goes_where_its_host_placed_it() {
        assert_eq!(
            remote_column(&card("running", false, "open")),
            Some(Column::Running)
        );
        assert_eq!(
            remote_column(&card("waiting", true, "open")),
            Some(Column::Needs)
        );
        assert_eq!(
            remote_column(&card("review", true, "open")),
            Some(Column::Needs)
        );
        // A finished turn whose tab was closed waits in 停下了, as at home.
        assert_eq!(
            remote_column(&card("review", false, "open")),
            Some(Column::Review)
        );
        assert_eq!(
            remote_column(&card("stale", false, "open")),
            Some(Column::Stale)
        );
        assert_eq!(
            remote_column(&card("review", false, "done")),
            Some(Column::Done)
        );
        assert_eq!(remote_column(&card("fresh", false, "open")), None);
        assert_eq!(remote_column(&card("review", true, "abandoned")), None);
        let mut shell = card("running", false, "open");
        shell.session = None;
        assert_eq!(remote_column(&shell), None, "no conversation, no card");
    }

    #[test]
    fn remote_ids_round_trip_and_never_look_local() {
        let id = remote_id(2, "a:b");
        assert!(is_remote(&id));
        assert_eq!(parse(&id), Some((2, "a:b")));
        assert_eq!(parse("2:t"), None);
    }

    #[test]
    fn mixed_columns_keep_one_order() {
        let mut keyed = vec![
            (false, 30, "local-new".into()),
            (true, 50, "remote-asks".into()),
            (false, 10, "remote-old".into()),
        ];
        order(Column::Needs, &mut keyed);
        let ids: Vec<_> = keyed.iter().map(|k| k.2.as_str()).collect();
        assert_eq!(ids, ["remote-asks", "remote-old", "local-new"]);
        order(Column::Review, &mut keyed);
        let ids: Vec<_> = keyed.iter().map(|k| k.2.as_str()).collect();
        assert_eq!(ids, ["remote-asks", "local-new", "remote-old"]);
    }
}
