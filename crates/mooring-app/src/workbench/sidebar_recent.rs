//! 会话优先's list: what needs you, what runs, then 今天 and 更早. With other machines
//! configured their rows sort into the same sections by the same rule; each machine's own
//! host said what state they are in.
use super::sidebar_list::{End, Listed, RecentOrder, recent_order, split_today};
use super::sidebar_machines::RemoteRow;
use super::sidebar_wasm::{AddRow, Closed};
use super::*;
use mooring_core::sidebar::Bucket;

#[derive(Clone, Copy)]
enum Item<'a> {
    Here(Listed<'a>),
    There(&'a RemoteRow),
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
                    self.session_row(l, current, None, End::State, cx)
                }
                Item::There(r) => self.remote_row(r, cx),
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
        let remote = self.listed_remote_rows();
        let when = |l: &Listed| match self.row_bucket(l) {
            Bucket::Running => self
                .turn_started(l.0)
                .unwrap_or_else(|| self.session_updated(l.0)),
            _ => self.session_updated(l.0),
        };
        let rows: Vec<(Item, Bucket, u64)> = sessions
            .iter()
            .map(|l| (Item::Here(*l), self.row_bucket(l), when(l)))
            .chain(remote.iter().map(|r| (Item::There(r), r.bucket, r.when)))
            .collect();
        let RecentOrder {
            need,
            running,
            stopped,
            quiet,
        } = recent_order(&rows);
        // What it asks or why it failed; with neither to show, what it is at now.
        let second = |l: &Listed| {
            self.row_detail(l.0, l.1).or_else(|| {
                self.workspace
                    .content
                    .get(&l.0.id)
                    .map(|c| mooring_core::content::short(&c.headline(), 100))
                    .filter(|h| !h.trim().is_empty())
            })
        };
        // Only what asks you or failed gets a second line: the question is what you
        // act on. A finished turn or a running one reads from its state mark alone,
        // so 要你处理 and 在跑 fit one screen even with dozens of sessions.
        for (key, title, rows, lines) in [
            (
                "recent:need",
                mooring_core::status::needs_you_word(),
                need,
                true,
            ),
            ("recent:running", Status::Running.label(), running, false),
        ] {
            if rows.is_empty() {
                continue;
            }
            out.push(self.flat_fold(key, title, rows.len(), cx));
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
                match item {
                    Item::Here(l) => {
                        // 等后台 says what will wake it, and a finished turn what it left
                        // running: both are worth the second line.
                        let asks = l.1.blocked()
                            || l.1 == Status::Background
                            || mooring_core::status::left_behind(&self.workspace, l.0).is_some();
                        let d = if lines && asks { second(&l) } else { None };
                        push(out, l, d, End::State);
                    }
                    Item::There(r) => out.push(self.remote_row(r, cx)),
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
            out.push(self.flat_fold("recent:today", t!("今天"), today.len(), cx));
            self.history_rows("recent:today", true, today, current, out, cx);
        }
        // Older ones a module was not handed are still counted, and open in 鸟瞰.
        let unlisted = closed.total.saturating_sub(closed.rows.len());
        if !older.is_empty() || unlisted > 0 {
            // Narrowed to one space the list is short, so it opens by itself.
            let open = self
                .sidebar
                .open("recent:older", self.space_filter().is_some());
            out.push(self.flat_more(
                "recent:older",
                t!(
                    "更早 {unlisted} 个 · 停着或做完了",
                    unlisted = older.len() + unlisted
                ),
                open,
                cx,
            ));
            let default = self.space_filter().is_some();
            self.history_rows("recent:older", default, older, current, out, cx);
            if open && unlisted > 0 {
                out.push(self.flat_closed_more(unlisted, cx));
            }
        }
    }
}
