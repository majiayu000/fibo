//! The JSON lines of the remote protocol, version [`VERSION`]: one object per line each
//! way, a line at most [`MAX_LINE`] bytes. A client sends [`Request`]s, `hello` first;
//! every line back is an [`Incoming`]. Unknown fields are ignored both ways, so either
//! side may add some without a new version.
use crate::role::Role;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const VERSION: u32 = 1;
pub const MAX_LINE: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    Hello {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u32>,
        /// Names this client for the `request` ids its writes carry; stays the same
        /// across its reconnects (a device id). Without it, `request` is ignored.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client: Option<String>,
    },
    /// Replies [`List`].
    List,
    /// The last `lines` (default 50, at most 2000) of a screen as plain text: [`Text`].
    Read {
        terminal: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lines: Option<u64>,
    },
    /// Pastes `text`; `enter` then presses Enter.
    Send {
        terminal: String,
        text: String,
        #[serde(default)]
        enter: bool,
        /// See [`Hello::requests`]; on `keys` and `answer` too.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// Presses named keys, all or none: `enter` `esc` `tab` `shift-tab` `backspace`
    /// `space` `up` `down` `left` `right` `home` `end` `page-up` `page-down` `delete`,
    /// `ctrl-a`…`ctrl-z`, or any single character.
    Keys {
        terminal: String,
        keys: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// Raw keyboard input, as a terminal emulator encodes it (base64): another desktop
    /// typing into this terminal. At most 64 KiB once decoded.
    Input {
        terminal: String,
        data: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// Answers the approval asking `question`; refused if it is no longer on screen.
    /// `always` takes the menu's 「don't ask again」 option instead, refused when it has
    /// none: never a plain allow in its place.
    Answer {
        terminal: String,
        question: String,
        allow: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        always: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// A conversation as chat: [`Chat`]. Names the `terminal` running it (its current
    /// conversation) or the `session`. Without `after` or `before`, the newest `limit`
    /// (default 50, at most 500); `after` gives those newer than a `seq`, `before` older.
    Chat {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
        /// Each Agent message also carries [`Tools`]: the tool calls since the message
        /// before it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        tools: bool,
    },
    /// Opens a stream of [`StreamBody::Event`]s, the current state first.
    Events,
    /// Opens a stream of one terminal's screen: [`StreamBody::Screen`], then output.
    /// `take`: as its writer, as a pane there would take it over; that machine's own
    /// panes turn read only until one takes it back. Then `resize` sizes it.
    Watch {
        terminal: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        take: bool,
    },
    /// A shell in a new tab, in `cwd` (else its space's directory): `{terminal, tab}`.
    /// With `agent` (as cards name it: `claude`, `codex`…) the shell runs that Agent,
    /// `prompt` its first message; the shell stays once it exits.
    Open {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        /// Picks a conversation back up (`lastsession` names it), in its Agent and its own
        /// directory, as the window does; `agent` and `cwd` are not read. If it still runs,
        /// the reply names that terminal with `existing` and nothing opens.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// The conversation last worked on in `cwd` with `agent`, for 「接着上次的对话」: [`Last`].
    LastSession {
        cwd: String,
        agent: String,
    },
    /// Makes a folder a space: [`Space`]. One that already is keeps its space.
    AddSpace {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// Tokens each Agent's local logs say it used since `since` (Unix seconds, the phone's
    /// midnight): [`Usage`].
    Usage {
        since: u64,
    },
    /// Closes a terminal and stops what runs in it.
    End {
        terminal: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// `done`, `deferred`, `abandoned` or `open` for a conversation; with the card's `turn`, refused
    /// once the Agent began another.
    Outcome {
        session: String,
        outcome: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// Sizes the terminal a taken `watch` stream shows.
    Resize {
        stream: u64,
        cols: u16,
        rows: u16,
    },
    Close {
        stream: u64,
    },
    /// Answered `{}` at once: a client's heartbeat, to find a link that died quietly.
    Ping,
    /// What changed in a git work tree against HEAD, untracked files included: [`Diff`].
    /// Names the `terminal` whose directory it is, or the `cwd`. Read only.
    Diff {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// The folders in `path` (default home, `~` expands), or those under home whose name
    /// holds `query`: [`Dirs`].
    Dirs {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
    },
    /// Makes one folder in an existing one: `{path}`.
    Mkdir {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// Interrupts the Agent's turn, as the desktop's 中断 does; refused unless it runs.
    Stop {
        terminal: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<String>,
    },
    /// The account limits each Agent last reported, within a week: [`Quotas`].
    Quota,
}

impl Op {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::List => "list",
            Self::Read { .. } => "read",
            Self::Send { .. } => "send",
            Self::Keys { .. } => "keys",
            Self::Input { .. } => "input",
            Self::Answer { .. } => "answer",
            Self::Chat { .. } => "chat",
            Self::Events => "events",
            Self::Watch { .. } => "watch",
            Self::Resize { .. } => "resize",
            Self::Open { .. } => "open",
            Self::End { .. } => "end",
            Self::Outcome { .. } => "outcome",
            Self::Close { .. } => "close",
            Self::Ping => "ping",
            Self::Diff { .. } => "diff",
            Self::Dirs { .. } => "dirs",
            Self::Mkdir { .. } => "mkdir",
            Self::Stop { .. } => "stop",
            Self::Quota => "quota",
            Self::LastSession { .. } => "lastsession",
            Self::AddSpace { .. } => "addspace",
            Self::Usage { .. } => "usage",
        }
    }
}

/// Every op, in the order `hello` lists them.
pub const OPS: &[&str] = &[
    "hello",
    "list",
    "read",
    "send",
    "keys",
    "input",
    "answer",
    "chat",
    "events",
    "watch",
    "resize",
    "open",
    "end",
    "outcome",
    "close",
    "ping",
    "diff",
    "dirs",
    "mkdir",
    "stop",
    "quota",
    "lastsession",
    "addspace",
    "usage",
];

/// A line from the machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Incoming {
    Stream(StreamMessage),
    /// `ok`'s shape depends on the op: [`Hello`], [`List`], [`Text`], [`Opened`], or `{}`.
    Reply {
        id: u64,
        ok: Value,
    },
    Failed {
        id: u64,
        error: String,
    },
    /// A line that could not be read as a request.
    Error {
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamMessage {
    /// The id of the request that opened the stream.
    pub stream: u64,
    #[serde(flatten)]
    pub body: StreamBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StreamBody {
    Event {
        event: Event,
    },
    /// The whole screen: base64 escape sequences that rebuild it, cursor, modes and
    /// alternate screen included, in a fresh emulator. Sent again after falling behind.
    Screen {
        cols: u16,
        rows: u16,
        vt: String,
    },
    /// Output after the screen, base64; feed it on.
    Output {
        vt: String,
    },
    /// Another client changed the size.
    Resized {
        cols: u16,
        rows: u16,
    },
    Exited {
        #[serde(default)]
        code: Option<i32>,
    },
    End {
        reason: String,
    },
}

/// One change, as `mooring events --json` prints it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// started, waiting, error, done, approval, approval_gone, terminal_started,
    /// terminal_exited, session_switched (a terminal now shows another conversation:
    /// `session` is it, `detail` the one before), command_done, command_failed, or
    /// lagged (events were lost: `list` again).
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Machine {
    /// Stays the same across restarts.
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub version: u32,
    pub machine: Machine,
    /// The ops this client may use.
    pub ops: Vec<String>,
    pub role: Role,
    /// `send`, `keys` and `answer` may carry a `request` id, unique within the `client`
    /// named in `hello`: sent again after a lost reply, it runs once and gets the first
    /// reply back, for a day. Absent from machines before 11-04.
    #[serde(default)]
    pub requests: bool,
    /// `macos` or `linux`; absent from machines before v2.
    #[serde(default)]
    pub os: Option<String>,
    /// The Mooring build answering, e.g. `0.1.0`; `version` above is the protocol's.
    #[serde(default)]
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct List {
    pub machine: Machine,
    pub spaces: Vec<Space>,
    pub terminals: Vec<Card>,
    /// What the board's 待办 holds; absent from hosts before 11-05.
    #[serde(default)]
    pub todos: Vec<Todo>,
}

/// A todo no conversation works on yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub space: String,
    pub title: String,
    pub agent: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub cwd: String,
}

/// A running terminal and what its Agent is doing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub space: String,
    pub tab: String,
    pub cwd: String,
    pub phase: Phase,
    pub agent: Option<String>,
    pub session: Option<String>,
    pub title: Option<String>,
    /// The approval it asks, when it asks one.
    pub approval: Option<String>,
    /// What it waits on the user for.
    pub waiting: Option<String>,
    /// The state the desktop shows (`Status::key`: waiting, error, running, review…)
    /// and its word; absent from hosts before 11-03 and for shells.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub status_label: Option<String>,
    /// Waiting on you or failed, as the desktop counts 要你处理.
    #[serde(default)]
    pub needs_you: bool,
    /// An Agent runs here but no conversation is tied to it (e.g. a Codex on the shared
    /// daemon): `agent` says which, this says so in the user's words. Phase stays `shell`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The conversation's current turn, for `outcome`'s `turn`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
    /// How the conversation was settled: open, done, deferred or abandoned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// When the conversation last changed, in Unix seconds, so a board can mix its card
    /// with the desktop's own in time order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<u64>,
    /// The branch checked out in `cwd`; a detached HEAD gives its short commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// When the turn began, in Unix seconds, while the state is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_since: Option<u64>,
    /// While running, what the log says it does now (`运行 cargo test`, `改 chat.tsx`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doing: Option<String>,
    /// The model that last answered, as the log names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Its permission mode as its log last said it, in words to show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Working,
    Waiting,
    Idle,
    /// No Agent conversation: a shell or another program.
    Shell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub text: String,
}

/// The reply to `chat`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chat {
    pub session: String,
    /// Oldest first.
    pub messages: Vec<Message>,
    /// Older messages exist before the first: ask again with `before`.
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Grows with each message; the cursor for `after` and `before`.
    pub seq: i64,
    /// Seconds since the epoch.
    pub at: u64,
    pub role: Speaker,
    pub text: String,
    /// With `chat`'s `tools`, on Agent messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Tools>,
}

/// The tool calls between an Agent message and the message before it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tools {
    /// Shell commands run.
    pub commands: u32,
    /// Files changed, in the order first touched.
    pub files: Vec<FileLines>,
    /// Seconds this stretch of work took; absent when the log cannot say.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub took: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLines {
    pub path: String,
    pub added: u32,
    pub removed: u32,
}

/// The reply to `diff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    /// The work tree's top.
    pub cwd: String,
    /// Empty on a detached HEAD.
    pub branch: String,
    pub files: Vec<DiffFile>,
    pub added: u32,
    pub removed: u32,
    /// Some patches were cut or left out: the whole is over 512 KiB.
    pub truncated: bool,
    /// With `terminal`: the test and check commands its conversation last ran.
    #[serde(default)]
    pub checks: Vec<Check>,
}

/// A test or check command as the log recorded it: its exit code, never the Agent's words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub command: String,
    /// What its output counted (`8 passed`); empty when it counted nothing readable.
    #[serde(default)]
    pub counts: String,
    pub ok: bool,
    pub at: u64,
    /// The conversation changed files after it ran, so it does not cover them.
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    pub added: u32,
    pub removed: u32,
    /// `M`, `A`, `D`, `R` or `?` (untracked).
    pub status: String,
    /// Unified diff, at most 64 KiB; empty for a binary file.
    pub patch: String,
    pub truncated: bool,
}

/// The reply to `dirs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dirs {
    /// The folder listed; empty for a `query`.
    pub path: String,
    /// Its parent; absent at `/` and for a `query`.
    #[serde(default)]
    pub parent: Option<String>,
    pub home: String,
    pub dirs: Vec<Dir>,
    /// Terminal directories in use now, most recent first.
    pub recent: Vec<String>,
    /// A `query` ran out of its second of walking: there may be more.
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dir {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitInfo>,
    /// Live Agent conversations whose terminal is in it.
    pub sessions: u32,
    /// A terminal works in it or below it now.
    pub recent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitInfo {
    pub branch: String,
    /// Changed and untracked files; absent when counting took too long (a huge tree).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<u32>,
    /// A linked work tree, not the main checkout.
    pub worktree: bool,
}

/// The reply to `lastsession`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Last {
    /// Absent when this directory has no such conversation.
    #[serde(default)]
    pub session: Option<LastSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastSession {
    pub id: String,
    pub title: String,
    pub updated: u64,
    /// The terminal it still runs in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
}

/// The reply to `usage`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub since: u64,
    pub agents: Vec<AgentUsage>,
    /// The logs are not all read yet; ask again for the rest.
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentUsage {
    pub agent: String,
    /// Input and output, new to the model: cache writes count, cache reads do not.
    pub tokens: u64,
    /// Read back from its cache, counted apart: a long session re-reads its whole context
    /// on every call, so these run a hundred times the rest.
    pub cached: u64,
}

/// The reply to `quota`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quotas {
    pub agents: Vec<AgentQuota>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentQuota {
    /// `claude`, `codex`… as cards name them.
    pub agent: String,
    pub plan: String,
    /// When the CLI recorded it, Unix seconds.
    pub at: u64,
    pub windows: Vec<QuotaWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub minutes: u32,
    /// Percent used.
    pub used: u16,
    /// Unix seconds; 0 when unknown.
    pub resets_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    User,
    Agent,
}

/// The reply to `events` and `watch`: the stream is open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Opened {
    pub stream: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cols: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requests_are_flat_objects_tagged_by_op() {
        let r = Request {
            id: 7,
            op: Op::Keys {
                terminal: "rt".into(),
                keys: vec!["up".into()],
                request: None,
            },
        };
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            json!({ "id": 7, "op": "keys", "terminal": "rt", "keys": ["up"] })
        );
        let r: Request = serde_json::from_value(json!({ "id": 1, "op": "list" })).unwrap();
        assert_eq!(r.op, Op::List);
        let r: Request =
            serde_json::from_value(json!({ "id": 2, "op": "send", "terminal": "t", "text": "hi" }))
                .unwrap();
        assert_eq!(r.op.name(), "send");
        assert!(OPS
            .iter()
            .all(|op| op.chars().all(|c| c.is_ascii_lowercase())));
    }

    #[test]
    fn incoming_lines_sort_into_replies_failures_and_streams() {
        let line = |v: Value| serde_json::from_value::<Incoming>(v).unwrap();
        assert!(matches!(
            line(json!({ "id": 1, "ok": {} })),
            Incoming::Reply { id: 1, .. }
        ));
        assert!(matches!(
            line(json!({ "id": 1, "error": "x" })),
            Incoming::Failed { .. }
        ));
        assert!(matches!(
            line(json!({ "error": "x" })),
            Incoming::Error { .. }
        ));
        let Incoming::Stream(s) =
            line(json!({ "stream": 4, "type": "screen", "cols": 80, "rows": 24, "vt": "" }))
        else {
            panic!()
        };
        assert_eq!(
            s.body,
            StreamBody::Screen {
                cols: 80,
                rows: 24,
                vt: String::new()
            }
        );
        let Incoming::Stream(s) = line(
            json!({ "stream": 8, "type": "event", "event": { "event": "done", "terminal": "t", "at": 1 } }),
        ) else {
            panic!()
        };
        let StreamBody::Event { event } = s.body else {
            panic!()
        };
        assert_eq!(
            (event.event.as_str(), event.rest["at"].as_u64()),
            ("done", Some(1))
        );
    }
}
