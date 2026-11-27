//! 会话优先's list: what needs you, what runs, then 今天 and 更早. With other machines
//! configured their rows sort into the same sections by the same rule; each machine's own
//! host said what state they are in.
use super::sidebar_list::{End, Listed, RecentOrder, RowDetail, recent_order, split_today};
use super::sidebar_machines::MachineRow;
use super::sidebar_wasm::{AddRow, Closed};
use super::*;
use fibo_core::sidebar::Bucket;

mod history_row;

#[derive(Clone, Copy)]
enum Item<'a> {
    Here(Listed<'a>),
    There(&'a MachineRow<'a>),
}

impl Workbench {
    /// A group of what no longer moves, moving with its fold. The session in front stays
    /// when the group closes.
    fn history_rows(
        &self,
        key: &str,
        default: bool,
        items: Vec<Item>,
        current: Option<&str>,
        out: &mut Vec<gpui::AnyElement>,
        cx: &Context<Self>,
    ) {
        let open = self.sidebar.open(key, default);
        let showing = self.sidebar.showing(key, default);
        let (start, mut keep) = (out.len(), Vec::new());
        for item in items {
            let stays = match item {
                Item::Here(l) => current == Some(l.0.id.as_str()),
                Item::There(r) => r.shown,
            };
            if !showing && !stays {
                continue;
            }
            if !open && stays {
                keep.push(out.len());
            }
            out.push(match item {
                Item::Here(l) => {
                    self.note_row(out.len(), &l);
                    self.history_row(l, current, cx)
                }
                Item::There(r) => self.machine_row(r, None, cx),
            });
        }
        self.sidebar.reveal(key, start, &keep, out);
    }

    pub(super) fn recent_rows(
        &self,
        sessions: Vec<Listed>,
        closed: &Closed,
        current: Option<&str>,
        push: &mut AddRow,
        out: &mut Vec<gpui::AnyElement>,
        cx: &Context<Self>,
    ) {
        // Pinned ones are in 置顶, above (`session_list`).
        let mut remote = self.listed_remote_rows(cx);
        remote.retain(|r| r.pin.is_none());
        let when = |l: &Listed| match self.row_bucket(l) {
            Bucket::Running => self
                .turn_started(l.0)
                .unwrap_or_else(|| self.session_updated(l.0)),
            _ => self.session_updated(l.0),
        };
        let rows: Vec<(Item, Bucket, Status, u64)> = sessions
            .iter()
            .map(|l| (Item::Here(*l), self.row_bucket(l), l.1, when(l)))
            .chain(
                remote
                    .iter()
                    .map(|r| (Item::There(r), r.bucket, r.l.1, r.when)),
            )
            .collect();
        let RecentOrder {
            need,
            running,
            stopped,
            quiet,
        } = recent_order(&rows);
        // What it asks or why it failed; with neither to show, what it is at now.
        let second = |from: &Workbench, l: &Listed| {
            from.row_detail(l.0, l.1).or_else(|| {
                from.workspace
                    .content
                    .get(&l.0.id)
                    .map(|c| fibo_core::content::short(&c.headline(), 100))
                    .filter(|h| !h.trim().is_empty())
                    .map(RowDetail::Text)
            })
        };
        for (key, title, rows) in [
            ("recent:need", fibo_core::status::needs_you_word(), need),
            ("recent:running", Status::Running.label(), running),
        ] {
            if rows.is_empty() {
                continue;
            }
            out.push(self.flat_fold(key, title, rows.len(), true, cx));
            let open = self.sidebar.open(key, true);
            let showing = self.sidebar.showing(key, true);
            let (start, mut keep) = (out.len(), Vec::new());
            for item in rows {
                let stays = match item {
                    Item::Here(l) => current == Some(l.0.id.as_str()),
                    Item::There(r) => r.shown,
                };
                if !showing && !stays {
                    continue;
                }
                if !open && stays {
                    keep.push(out.len());
                }
                // 等后台 says what will wake it, and a finished turn what it left
                // running: both are worth the second line.
                let asks = |from: &Workbench, l: &Listed| {
                    l.1.blocked()
                        || l.1 == Status::Background
                        || fibo_core::status::left_behind_detail(&from.workspace, l.0).is_some()
                };
                let detail = |from: &Workbench, l: &Listed| {
                    let ask = || asks(from, l).then(|| second(from, l)).flatten();
                    recent_detail_in(&from.workspace, l.0, key, ask)
                };
                match item {
                    Item::Here(l) => push(out, l, detail(self, &l), End::State),
                    Item::There(r) => out.push(self.machine_row(r, detail(r.from, &r.l), cx)),
                }
            }
            self.sidebar.reveal(key, start, &keep, out);
        }
        // What is no longer moving, open in a tab or closed: today's in full, the
        // rest folded into one line, as flat.html has it.
        let history: Vec<(Item, u64)> = stopped
            .into_iter()
            .chain(quiet)
            .chain(closed.rows.iter().copied().map(Item::Here))
            .map(|item| {
                let at = match item {
                    Item::Here(l) => self.session_updated(l.0),
                    Item::There(r) => r.when,
                };
                (item, at)
            })
            .collect();
        let (today, older) = split_today(history, crate::model::time::now());
        if !today.is_empty() {
            out.push(self.flat_fold("recent:today", t!("今天停着的"), today.len(), true, cx));
            self.history_rows("recent:today", true, today, current, out, cx);
        }
        // Older ones a module was not handed are still counted, and open in 鸟瞰.
        let unlisted = closed.total.saturating_sub(closed.rows.len());
        if !older.is_empty() || unlisted > 0 {
            // A heading beside 今天, not a line under it: as a quiet line it hung under a
            // folded 今天 as if it were one of its rows. Narrowed to one space the list is
            // short, so it opens by itself.
            let default = self.space_filter().is_some();
            let open = self.sidebar.open("recent:older", default);
            let count = older.len() + unlisted;
            out.push(self.flat_fold("recent:older", t!("更早停着的"), count, default, cx));
            self.history_rows("recent:older", default, older, current, out, cx);
            if open && unlisted > 0 {
                out.push(self.flat_closed_more(unlisted, cx));
            }
        }
    }
}

/// A row's second line in 要你处理 or 在跑. What asks you or failed gets one (`ask`): the
/// question is what you act on. A running one gets one only once its log names a tool or
/// command this turn (`正在改 tabs.rs`), never the ask its title already says, so a list of
/// fresh turns still reads one line each (#516). A finished turn reads from its mark.
fn recent_detail_in(
    w: &Workspace,
    s: &AgentSession,
    section: &str,
    ask: impl FnOnce() -> Option<RowDetail>,
) -> Option<RowDetail> {
    match section {
        "recent:need" => ask(),
        "recent:running" => super::sessions::running_line_in(w, s).map(RowDetail::Text),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    /// 「这个排列位置有很大的问题」: 更早 drawn as a quiet indented line hung under a folded
    /// 今天 as if it were one of its rows. It is a heading like 今天, beside it.
    #[test]
    fn earlier_is_a_heading_beside_today() {
        let src = include_str!("sidebar_recent.rs");
        let body = &src[..src.find("#[cfg(test)]").unwrap()];
        assert!(body.contains("self.flat_fold(\"recent:older\""));
        assert!(body.contains("self.flat_fold(\"recent:today\""));
    }

    /// 在跑's rows said nothing of what the Agent was doing (#516). Here and on another
    /// machine they say its latest record this turn; 要你处理 still says only what it asks.
    #[test]
    fn a_running_row_says_what_its_turn_is_at() {
        use super::super::sidebar_list::RowDetail;
        let mut w = super::super::tests::fixture();
        let s = w.agent_sessions[0].clone();
        w.content.insert(
            s.id.clone(),
            std::sync::Arc::new(super::super::sessions::running_turn(
                "crates/app/src/tabs.rs",
            )),
        );
        let running = super::recent_detail_in(&w, &s, "recent:running", || {
            panic!("a running row has no question to show")
        });
        assert!(matches!(running, Some(RowDetail::Text(t)) if t == "正在改 tabs.rs"));
        assert!(super::recent_detail_in(&w, &s, "recent:need", || None).is_none());
        // Both kinds of row take their second line from it.
        let src = include_str!("sidebar_recent.rs");
        let body = &src[..src.find("#[cfg(test)]").unwrap()];
        assert!(body.contains("Item::Here(l) => push(out, l, detail(self, &l), End::State)"));
        assert!(body.contains("self.machine_row(r, detail(r.from, &r.l), cx)"));
    }
}
