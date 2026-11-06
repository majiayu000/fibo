//! Which group a session's state puts it in, decided once for every surface: the board's
//! columns (this machine's cards, other machines' and the phone's, which read the
//! `column` their host computed), and the sidebar's buckets and row tiers.
use crate::model::{AgentSession, Outcome, Workspace};
use crate::sidebar::Bucket;
use crate::status::{self, Status};

/// The board's columns, left to right in the order work moves; what waits on you or
/// failed has its own column, 要你处理 (decided 11-02).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Column {
    Todo,
    Running,
    Needs,
    Review,
    Done,
    Stale,
}
impl Column {
    pub const ALL: [Self; 6] = [
        Self::Todo,
        Self::Running,
        Self::Needs,
        Self::Review,
        Self::Done,
        Self::Stale,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Todo => "待办",
            Self::Running => Status::Running.label(),
            Self::Needs => status::NEEDS_YOU,
            // Stopped and put off ones; finished turns are in 要你处理 since 11-03.
            Self::Review => Status::Paused.label(),
            Self::Done => Status::Closed.label(),
            Self::Stale => Status::Stale.label(),
        }
    }
    /// The name in the remote protocol's `Card::column` and in element ids.
    pub fn key(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Running => "running",
            Self::Needs => "needs",
            Self::Review => "review",
            Self::Done => "done",
            Self::Stale => "stale",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }
}

/// How long a conversation you marked done stays in 做完了.
const DONE_FOR: u64 = 86_400;

/// Which column an open or just-finished session belongs in; `None` keeps it off the board.
pub fn column(w: &Workspace, s: &AgentSession, now: u64) -> Option<Column> {
    if w.session_quiet(s) {
        return None;
    }
    if !s.outcome.needs_review(now) {
        return (s.outcome == Outcome::Completed && completed_within(w, &s.id, now, DONE_FOR))
            .then_some(Column::Done);
    }
    Some(match status::of(w, s, now) {
        // Just opened, no turn yet: nothing to put on a card.
        Status::Fresh => return None,
        Status::Running => Column::Running,
        // The one count of 要你处理 (status::needs_you): the chip, the sidebar and this
        // column agree. A finished or failed one whose tab you closed waits in 停下了.
        _ if w.needs_you(s, now) => Column::Needs,
        Status::Stale => Column::Stale,
        // A turn you stopped, or one put off whose time came: 停下了.
        _ => Column::Review,
    })
}
fn completed_within(w: &Workspace, id: &str, now: u64, window: u64) -> bool {
    w.outcome_history
        .iter()
        .rev()
        .find(|h| h.session == id)
        .is_some_and(|h| h.after == Outcome::Completed && now.saturating_sub(h.at) < window)
}

/// The sidebar group a state falls in. 要你处理 holds every stopped turn that is yours
/// (11-03): asking, failed, finished for you to check, or with work left in the
/// background; the sidebar narrows it to what `status::needs_you` counts.
pub fn bucket(g: Status) -> Bucket {
    match g {
        _ if g.needs_you() => Bucket::Attention,
        Status::Running => Bucket::Running,
        Status::Fresh => Bucket::Fresh,
        Status::Stale => Bucket::Stale,
        // Marked complete, put off or interrupted: no longer moving on its own.
        _ => Bucket::Stopped,
    }
}

/// What needs you, in this order: waiting on you, failed, a finished turn, one with work
/// left in the background.
const NEEDS_ORDER: [Status; 4] = [
    Status::Waiting,
    Status::Error,
    Status::Review,
    Status::Background,
];
/// Sidebar row tiers by the shown state: what needs you (`attention`, in `NEEDS_ORDER`),
/// then live work, then the rest, then sessions opened but not yet spoken to, then what
/// has gone quiet. Inside a tier the newest comes first.
pub fn rank(g: Status, attention: bool) -> (u8, usize) {
    if attention {
        let at = NEEDS_ORDER.iter().position(|o| *o == g);
        return (0, at.unwrap_or(NEEDS_ORDER.len()));
    }
    match bucket(g) {
        Bucket::Running => (1, 0),
        Bucket::Fresh => (3, 0),
        Bucket::Stale => (4, 0),
        _ => (2, 0),
    }
}
