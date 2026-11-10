//! The one answer to "what is this session doing", for every surface: sidebar, panes,
//! tabs, status bar, board, overview, palette, menu bar, plugins, phones. Each state has
//! one key, one word and one glyph; the app gives it one color (specs/status-model).
//!
//! Where the reading came from (the screen, a hook, the log) is evidence, not a state:
//! a turn running in a Mooring terminal and one running in a log are both 在跑.
//! Whether you have looked yet is attention, not a state: an unseen finished turn is
//! still 等你验收.
use crate::content::{Content, Turn};
use crate::model::{AgentSession, Outcome, ProcessState, Task, Terminal, Workspace};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Waiting,
    Error,
    Running,
    /// The turn ended and only a server, a monitor or a scheduled wakeup it left is still
    /// alive. Counts with 要你处理 (a shell that will end keeps the session 在跑).
    Background,
    /// The turn ended; whether the work is done is yours to say (2026-11-02).
    Review,
    /// A live agent that has not started a turn yet: just opened, or `/clear`ed.
    Fresh,
    /// The turn was interrupted, or the agent is gone without finishing one.
    Paused,
    /// Untouched past the stale days.
    Stale,
    /// You put it off until a time that has not come yet.
    Deferred,
    /// You marked it complete (or abandoned it): the only 做完了.
    Closed,
}

impl Status {
    pub const ALL: [Self; 10] = [
        Self::Waiting,
        Self::Error,
        Self::Running,
        Self::Background,
        Self::Review,
        Self::Fresh,
        Self::Paused,
        Self::Stale,
        Self::Deferred,
        Self::Closed,
    ];
    /// The name in a theme file, the plugin protocol and logs.
    pub fn key(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Error => "error",
            Self::Running => "running",
            Self::Background => "background",
            Self::Review => "review",
            Self::Fresh => "fresh",
            Self::Paused => "paused",
            Self::Stale => "stale",
            Self::Deferred => "deferred",
            Self::Closed => "closed",
        }
    }
    /// The state named by [`Status::key`], e.g. from another machine's cards.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }
    /// One word per state everywhere; nothing else spells these (tests/guard.rs).
    pub fn label(self) -> &'static str {
        match self {
            Self::Waiting => crate::t!("等你回答"),
            Self::Error => crate::t!("出错了"),
            Self::Running => crate::t!("在跑"),
            Self::Background => crate::t!("等后台"),
            Self::Review => crate::t!("等你验收"),
            Self::Fresh => crate::t!("新开的"),
            Self::Paused => crate::t!("停下了"),
            Self::Stale => crate::t!("很久没动"),
            Self::Deferred => crate::t!("稍后再看"),
            Self::Closed => crate::t!("做完了"),
        }
    }
    /// The word where only a few letters fit (the menu bar's count cards). The same as
    /// [`Status::label`] in Chinese; English drops "for you".
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Waiting => crate::t!("等你回答#short"),
            Self::Review => crate::t!("等你验收#short"),
            _ => self.label(),
        }
    }
    /// The terminal style's symbol. ✓ is yours to give; a turn waiting for it is the open
    /// diamond beside 等你回答's ◆.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Waiting => "◆",
            Self::Error => "✕",
            Self::Running => "◐",
            Self::Background => "◔",
            Self::Review => "◇",
            Self::Fresh => "○",
            Self::Paused => "‖",
            Self::Stale => "·",
            Self::Deferred => "◷",
            Self::Closed => "✓",
        }
    }
    /// Stopped and yours (要你处理, 11-03 定不再分处理和验收): waiting on you, failed, or a
    /// turn that ended for you to check, with or without work left in the background.
    pub fn needs_you(self) -> bool {
        matches!(
            self,
            Self::Waiting | Self::Error | Self::Review | Self::Background
        )
    }
    /// The Agent cannot go on without you: the urgent part of 要你处理, the only one that
    /// sounds and colors the 要你处理 chip orange or red (等你验收 gets a silent banner).
    pub fn blocked(self) -> bool {
        matches!(self, Self::Waiting | Self::Error)
    }
    /// Work in flight: a turn under way. 等后台 is not: its turn ended and only a monitor
    /// or a scheduled wakeup will bring it back, so it waits for you with 等你验收.
    pub fn working(self) -> bool {
        matches!(self, Self::Running)
    }
    /// The coarse phase scripts and the phone wait on: at work, asking you, or not
    /// moving (everything else, failed included).
    pub fn phase(self) -> Phase {
        match self {
            Self::Running => Phase::Working,
            Self::Waiting => Phase::Waiting,
            _ => Phase::Idle,
        }
    }
}

/// What a live Agent is doing at the coarsest grain, always taken from a `Status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Working,
    Waiting,
    Idle,
}
impl Phase {
    /// The word `mooring list`, `mooring wait` and the phone protocol use.
    pub fn key(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Idle => "idle",
        }
    }
}

pub use crate::program_state::of_program;

/// When the turn behind 在跑 began, for 「在跑 · 12 分钟」; `None` in any other state.
pub fn running_since(w: &Workspace, s: &AgentSession, now: u64) -> Option<u64> {
    (of(w, s, now) == Status::Running)
        .then(|| w.content.get(&s.id)?.running_since())
        .flatten()
}

/// The todos still to start, the board's 待办 on every surface: not put away or closed,
/// and no conversation works on them yet.
pub fn open_todos(w: &Workspace, now: u64) -> Vec<&Task> {
    let open = |t: &&Task| {
        w.task_outcomes
            .get(&t.id)
            .unwrap_or(&Outcome::Open)
            .needs_review(now)
            && !w
                .agent_sessions
                .iter()
                .any(|s| s.task.as_ref() == Some(&t.id))
            && !w.space_quiet(&t.space)
    };
    w.tasks.iter().filter(open).collect()
}

/// Every todo you have not closed, started or not: a todo outlives the sessions run for it.
/// Newest first, so what you just wrote is where you are looking; todos from before times
/// were kept, and those written in the same second, go by the order they were added.
pub fn unfinished_todos(w: &Workspace, now: u64) -> Vec<&Task> {
    let mut todos: Vec<&Task> = (w.tasks.iter().rev())
        .filter(|t| {
            (w.task_outcomes.get(&t.id))
                .unwrap_or(&Outcome::Open)
                .needs_review(now)
                && !w.space_quiet(&t.space)
        })
        .collect();
    todos.sort_by_key(|t| {
        std::cmp::Reverse(w.task_logs.get(&t.id).and_then(|l| l.created).unwrap_or(0))
    });
    todos
}

/// The running Mooring terminal whose current conversation this is.
pub fn live_terminal<'a>(w: &'a Workspace, s: &AgentSession) -> Option<&'a Terminal> {
    if !s.current {
        return None;
    }
    let id = s.terminal.as_ref()?;
    w.terminals
        .iter()
        .find(|t| &t.id == id && t.state == ProcessState::Running)
}

/// No turn has started in this conversation: no record yet, or one with no ask and no
/// turn state. An empty Claude reads as this, not as 等你验收.
pub fn fresh(c: Option<&Content>) -> bool {
    c.is_none_or(|c| c.asks.is_empty() && c.turn == Turn::Unknown)
}

/// Where a todo stands. Its outcome is yours; the rest is read from its sessions and
/// its acceptance evidence. The only place a todo's state is decided: the todo detail,
/// the sidebar's todo rows and the board read this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    NotStarted,
    /// A session of it is still open.
    Working,
    /// Every session you started for it is marked done, and you have not closed it.
    Settled,
    /// Every acceptance criterion has evidence.
    Proven,
    Completed,
    Deferred,
    Abandoned,
}
impl TaskState {
    /// What a client keys its look on; `label` is what it shows.
    pub fn key(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Working => "working",
            Self::Settled => "settled",
            Self::Proven => "proven",
            Self::Completed => "completed",
            Self::Deferred => "deferred",
            Self::Abandoned => "abandoned",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::NotStarted => crate::t!("未开始"),
            Self::Working => crate::t!("进行中"),
            Self::Settled => crate::t!("会话都做完了"),
            Self::Proven => crate::t!("验收齐了"),
            Self::Completed => crate::t!("已完成"),
            Self::Deferred => crate::t!("稍后再看"),
            Self::Abandoned => crate::t!("已放弃"),
        }
    }
    /// Closed by you: its sessions no longer ask for you and take no new conversation.
    pub fn closed(self) -> bool {
        matches!(self, Self::Completed | Self::Abandoned)
    }
}
pub fn task_state(w: &Workspace, task: &Task, now: u64) -> TaskState {
    match w.task_outcomes.get(&task.id).unwrap_or(&Outcome::Open) {
        Outcome::Completed => return TaskState::Completed,
        Outcome::Abandoned => return TaskState::Abandoned,
        Outcome::Deferred { until, .. } if *until > now => return TaskState::Deferred,
        _ => (),
    }
    if !task.acceptance.is_empty() && task.proven() == task.acceptance.len() {
        return TaskState::Proven;
    }
    let mut sessions = w
        .agent_sessions
        .iter()
        .filter(|s| s.task.as_ref() == Some(&task.id))
        .peekable();
    if sessions.peek().is_none() {
        return TaskState::NotStarted;
    }
    if sessions.all(|s| of(w, s, now) == Status::Closed) {
        TaskState::Settled
    } else {
        TaskState::Working
    }
}
/// The todo a session works for is one you closed.
fn task_closed(w: &Workspace, s: &AgentSession) -> bool {
    s.task.as_ref().is_some_and(|id| {
        matches!(
            w.task_outcomes.get(id),
            Some(Outcome::Completed | Outcome::Abandoned)
        )
    })
}

/// The name of the group `needs_you` counts (waiting on you, failed, or finished for you
/// to check), wherever it is shown: the window's chip and board column, the menu bar,
/// the sidebar's groups.
pub fn needs_you_word() -> &'static str {
    crate::t!("要你处理")
}

/// Whether a session asks for you now: stopped and yours (`Status::needs_you`), in a Mooring terminal
/// that is open in a tab (its process may have failed), not one you put off or closed
/// (nor its todo), and not hidden
/// as automation.
/// The one place 要你处理 is counted: the window's chip and board, the menu bar, the
/// phone and notifications all ask this.
pub fn needs_you(w: &Workspace, s: &AgentSession, now: u64) -> bool {
    asks_for_you(w, s, now) && !w.session_quiet(s)
}

/// `needs_you` without asking whether its space is put aside: what a 先不管 line counts as
/// waiting.
pub fn asks_for_you(w: &Workspace, s: &AgentSession, now: u64) -> bool {
    // The session's own fields first: the window asks this of every session on every
    // frame, and most fail here before the content lookup hashes their id.
    let shown = || match w.content.get(&s.id) {
        Some(c) => c.hidden_for(s).is_none(),
        None => s.current && s.binding.is_some(),
    };
    s.managed()
        // The terminal's conversation, in a tab: one whose process failed still asks.
        && s.current
        && s.outcome.needs_review(now)
        && s.terminal.as_ref().is_some_and(|t| w.terminal_tab(t).is_some())
        && shown()
        && !task_closed(w, s)
        && of(w, s, now).needs_you()
}

/// The state of what runs in a terminal. An Agent asking to be trusted with its folder is
/// waiting on you, with or without a conversation: it does nothing until you answer.
pub fn of_terminal(w: &Workspace, terminal: &str, asks_trust: bool, now: u64) -> Option<Status> {
    if asks_trust {
        return Some(Status::Waiting);
    }
    w.current_agent(terminal).map(|s| of(w, s, now))
}

/// What this session's finished turn left running in its live terminal: a note on its
/// state, not a state. Closing that terminal may end it, so 逐个验收 keeps out of it.
pub fn left_behind(w: &Workspace, s: &AgentSession) -> Option<String> {
    live_terminal(w, s)?;
    w.content.get(&s.id)?.left_behind()
}

/// The session's state now. The only place that decides it.
pub fn of(w: &Workspace, s: &AgentSession, now: u64) -> Status {
    let content = w.content.get(&s.id);
    if live_terminal(w, s).is_some() {
        live(w, s, content, now)
    } else {
        recorded(w, s, content, now)
    }
}

/// What you decided overrides what the agent last did, once its turn is over.
fn decided(w: &Workspace, s: &AgentSession, now: u64) -> Option<Status> {
    // Closing a todo is your outcome for every session working on it.
    if task_closed(w, s) {
        return Some(Status::Closed);
    }
    match s.outcome {
        Outcome::Completed | Outcome::Abandoned => Some(Status::Closed),
        Outcome::Deferred { until, .. } if until > now => Some(Status::Deferred),
        _ => None,
    }
}

/// An agent in a Mooring terminal: what it is doing right now wins.
fn live(w: &Workspace, s: &AgentSession, c: Option<&Content>, now: u64) -> Status {
    let background = c.and_then(|c| c.background).unwrap_or_default();
    let turn = c.map_or(Turn::Unknown, |c| c.turn);
    match turn {
        Turn::Waiting => return Status::Waiting,
        Turn::Running => return Status::Running,
        // Its subagents, or a job it waits for, still work for it: the turn is not over.
        Turn::Completed if background.keeps_working() => return Status::Running,
        _ => (),
    }
    if let Some(d) = decided(w, s, now) {
        return d;
    }
    if fresh(c) {
        return Status::Fresh;
    }
    match turn {
        Turn::Error => Status::Error,
        // Only what will wake it. A server wakes nothing, so that turn is simply over.
        _ if background.wakes() => Status::Background,
        Turn::Interrupted => Status::Paused,
        _ => Status::Review,
    }
}

/// Every other session: what its log and its terminal's exit say.
fn recorded(w: &Workspace, s: &AgentSession, c: Option<&Content>, now: u64) -> Status {
    if let Some(d) = decided(w, s, now) {
        return d;
    }
    let default = Content {
        updated: s.updated,
        ..Default::default()
    };
    let c = c.unwrap_or(&default);
    // External logs prove activity, not the existence of a live OS process indefinitely.
    let running = c.turn == Turn::Running && (s.current || now.saturating_sub(c.updated) < 900);
    let last_turn = if c.turn_started > 0 {
        c.turn_started
    } else {
        c.updated
    };
    let since = match s.outcome {
        Outcome::Deferred { until, .. } => last_turn.max(until),
        _ => last_turn.max(w.review_since(&s.id)),
    };
    if !running && now.saturating_sub(since) > u64::from(w.settings.stale_days) * 86400 {
        return Status::Stale;
    }
    // A question in the log waits for you only while someone can answer it. A session
    // bound to a Mooring terminal reads its record here only once its agent has left
    // that terminal (or the terminal ended, or its row is gone): nobody is left to ask.
    let left_its_terminal = s.terminal.is_some();
    if c.turn == Turn::Waiting && !left_its_terminal {
        return Status::Waiting;
    }
    let failed = w
        .terminals
        .iter()
        .find(|t| s.terminal.as_ref() == Some(&t.id))
        .is_some_and(|t| {
            matches!(t.state, ProcessState::Failed | ProcessState::Interrupted)
                || t.exit_code.is_some_and(|c| c != 0)
        });
    let background = c.background.unwrap_or_default();
    if c.turn == Turn::Error || failed {
        Status::Error
    } else if c.turn == Turn::Completed && s.current && background.keeps_working() {
        Status::Running
    } else if c.turn == Turn::Completed && s.current && background.wakes() {
        Status::Background
    } else if c.turn == Turn::Completed {
        Status::Review
    } else if running {
        Status::Running
    } else {
        Status::Paused
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Background;
    use crate::model::Agent;

    const NOW: u64 = 10 * 86400;

    fn session(terminal: Option<&str>, current: bool) -> AgentSession {
        AgentSession {
            id: "s".into(),
            runtime: Agent::Codex,
            cli_session_id: "c".into(),
            terminal: terminal.map(Into::into),
            current,
            task: None,
            outcome: Outcome::Open,
            updated: NOW,
            cwd: "/tmp".into(),
            transcript: "f".into(),
            binding: None,
            name: String::new(),
        }
    }
    fn content(turn: Turn) -> Content {
        Content {
            updated: NOW,
            turn_started: NOW,
            turn,
            asks: vec![crate::content::Quote::default()],
            ..Default::default()
        }
    }
    /// A workspace whose terminal `t` runs, or has exited with `exit`.
    fn workspace(c: Option<Content>, exit: Option<i32>) -> Workspace {
        let mut w = Workspace::default();
        w.terminals.push(Terminal {
            id: "t".into(),
            space: String::new(),
            argv: vec![],
            cwd: "/tmp".into(),
            pid: None,
            state: if exit.is_some() {
                ProcessState::Exited
            } else {
                ProcessState::Running
            },
            exit_code: exit,
            error: None,
        });
        if let Some(c) = c {
            w.content.insert("s".into(), c);
        }
        w
    }
    fn live_status(c: Option<Content>) -> Status {
        of(&workspace(c, None), &session(Some("t"), true), NOW)
    }
    fn recorded_status(c: Option<Content>) -> Status {
        of(&workspace(c, Some(0)), &session(Some("t"), false), NOW)
    }

    #[test]
    fn live_agent_reads_what_it_does_now() {
        assert_eq!(live_status(Some(content(Turn::Waiting))), Status::Waiting);
        assert_eq!(live_status(Some(content(Turn::Running))), Status::Running);
        assert_eq!(live_status(None), Status::Fresh);
        assert_eq!(live_status(Some(content(Turn::Error))), Status::Error);
        assert_eq!(
            live_status(Some(content(Turn::Interrupted))),
            Status::Paused
        );
        assert_eq!(live_status(Some(content(Turn::Completed))), Status::Review);
        let with = |b: Background| {
            let mut c = content(Turn::Completed);
            c.background = Some(b);
            live_status(Some(c))
        };
        assert_eq!(
            with(Background {
                agents: 1,
                ..Default::default()
            }),
            Status::Running
        );
        assert_eq!(
            with(Background {
                shells: 1,
                ..Default::default()
            }),
            Status::Review,
            "a server wakes nothing: the turn is over"
        );
        assert_eq!(
            with(Background {
                crons: 1,
                ..Default::default()
            }),
            Status::Background
        );
        // Of three shells, one a job it waits for (a test run): the turn is not over.
        // Only servers left (an http.server, a relay): it is, and nothing is being waited on.
        assert_eq!(
            with(Background {
                shells: 3,
                jobs: 1,
                ..Default::default()
            }),
            Status::Running
        );
        assert_eq!(
            with(Background {
                shells: 3,
                jobs: 0,
                ..Default::default()
            }),
            Status::Review
        );
    }

    #[test]
    fn your_decision_overrides_an_ended_turn_but_not_a_running_one() {
        let mut w = workspace(Some(content(Turn::Completed)), None);
        let mut s = session(Some("t"), true);
        s.outcome = Outcome::Completed;
        assert_eq!(of(&w, &s, NOW), Status::Closed);
        s.outcome = Outcome::Deferred {
            reason: String::new(),
            until: NOW + 1,
        };
        assert_eq!(of(&w, &s, NOW), Status::Deferred);
        assert_eq!(of(&w, &s, NOW + 1), Status::Review);
        w.content.get_mut("s").unwrap().turn = Turn::Running;
        s.outcome = Outcome::Completed;
        assert_eq!(of(&w, &s, NOW), Status::Running);
    }

    #[test]
    fn recorded_session_reads_its_log() {
        // An agent outside Mooring: its log is all there is.
        assert_eq!(
            of(
                &workspace(Some(content(Turn::Waiting)), Some(0)),
                &session(None, false),
                NOW
            ),
            Status::Waiting
        );
        // Its Mooring terminal has ended: nobody is left to answer.
        assert_eq!(
            recorded_status(Some(content(Turn::Waiting))),
            Status::Paused
        );
        assert_eq!(
            recorded_status(Some(content(Turn::Completed))),
            Status::Review
        );
        assert_eq!(
            recorded_status(Some(content(Turn::Interrupted))),
            Status::Paused
        );
        assert_eq!(recorded_status(Some(content(Turn::Error))), Status::Error);
        // Exited non-zero: failed whatever the log said last.
        let w = workspace(Some(content(Turn::Completed)), Some(1));
        assert_eq!(of(&w, &session(Some("t"), false), NOW), Status::Error);
        // A log still running within 15 minutes is running; past the stale days, stale.
        let s = session(None, false);
        let w = workspace(Some(content(Turn::Running)), None);
        assert_eq!(of(&w, &s, NOW + 60), Status::Running);
        assert_eq!(of(&w, &s, NOW + 4 * 86400), Status::Stale);
    }

    #[test]
    fn every_status_has_its_own_key_word_and_glyph() {
        for (i, a) in Status::ALL.iter().enumerate() {
            for b in &Status::ALL[i + 1..] {
                assert_ne!(a.key(), b.key());
                assert_ne!(a.label(), b.label());
                assert_ne!(a.glyph(), b.glyph());
            }
        }
    }

    /// orca#10633: the agent quit at a permission prompt and its Mooring terminal is
    /// back at the shell, or the terminal itself is gone. Nobody is left to answer, so it
    /// does not sit under 要你处理 until it goes stale.
    #[test]
    fn an_agent_gone_from_its_terminal_is_not_waiting() {
        let w = workspace(Some(content(Turn::Waiting)), None);
        assert_eq!(of(&w, &session(Some("t"), false), NOW), Status::Paused);
        let w = workspace(Some(content(Turn::Waiting)), Some(129));
        assert_eq!(of(&w, &session(Some("t"), false), NOW), Status::Error);
        // Its terminal's row is gone: nobody is left to answer either.
        let mut w = workspace(Some(content(Turn::Waiting)), None);
        w.terminals.clear();
        assert_eq!(of(&w, &session(Some("t"), true), NOW), Status::Paused);
        // Still at the prompt in its terminal: waiting.
        let w = workspace(Some(content(Turn::Waiting)), None);
        assert_eq!(of(&w, &session(Some("t"), true), NOW), Status::Waiting);
    }
}
