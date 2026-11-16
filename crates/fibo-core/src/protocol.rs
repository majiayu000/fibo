use crate::model::Workspace;
use serde::{Deserialize, Serialize};

mod frame;
mod input;
mod request;
pub use frame::*;
pub use input::{TrustPrompt, TurnInput};
pub use request::{HookFacts, HookShell, Request};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteChange {
    /// view, answer or full; its open sessions end and it reconnects with the new one.
    Role(String),
    /// Ends its open sessions now; it stays paired and may connect again.
    Kick,
    Revoke,
    /// Changed outside the host (`fibo remote --revoke`): show the app again.
    Refresh,
}

/// Phones on a relay, as the app shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStatus {
    pub enabled: bool,
    pub relay: String,
    /// A registration key is saved; the key itself stays with the host.
    #[serde(default)]
    pub keyed: bool,
    #[serde(default)]
    pub devices: Vec<RemoteDevice>,
    #[serde(flatten)]
    pub live: RemoteLive,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLive {
    pub state: RemoteState,
    /// Why it is not connected, in the user's words; empty when it is.
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub online: Vec<RemotePhone>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteState {
    #[default]
    Off,
    Connecting,
    Connected,
    /// Stopped and not retrying, e.g. the relay refused this machine; `detail` says why.
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
    pub role: String,
    /// Unix seconds.
    pub paired: u64,
    pub last_seen: Option<u64>,
}

/// One phone connected now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePhone {
    pub device: String,
    /// Unix milliseconds.
    pub since: u64,
    /// Terminals it watches.
    #[serde(default)]
    pub watching: Vec<String>,
    pub last: Option<RemoteAct>,
}

/// The last thing a phone did: send, keys or answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAct {
    pub op: String,
    pub terminal: Option<String>,
    /// Unix milliseconds.
    pub at: u64,
}

/// Names one write: `client` stays the same for the sender's life, `id` is unique
/// within it. The host remembers each outcome for a day (`REQUEST_KEPT_SECS`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId {
    pub client: String,
    pub id: String,
}
pub const REQUEST_KEPT_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize)]
pub enum Reply {
    Hello {
        major: u32,
        instance: String,
        /// Absent from hosts built before it existed, which are stale by definition.
        #[serde(default)]
        build: Build,
    },
    Workspace(Workspace),
    /// [`Request::Brief`]'s workspace reply.
    WorkspaceDelta(Box<crate::workspace_delta::Delta>),
    Cwd(String),
    NotificationClaimed(bool),
    Recognition(RecognitionStatus),
    Diagnostics(crate::diagnostics::Diagnostics),
    Attached {
        writable: bool,
        cols: u16,
        rows: u16,
    },
    /// The terminals that follow, each as a snapshot (SNAPSHOT frames, then READY), and
    /// then their PTY descriptors in this order in one SCM_RIGHTS message.
    Handoff {
        terminals: Vec<Handed>,
    },
    Ok,
    Text(String),
    /// [`Request::AssociateScreen`]: the conversation that terminal now holds; None when
    /// the screen did not say which.
    ScreenAssociated {
        session: Option<String>,
    },
    Modes(crate::modes::ModeReply),
    /// [`Request::InputSuggestion`]: the conversation in that terminal, and what it offers.
    InputSuggestion {
        session: Option<String>,
        text: Option<String>,
    },
    /// [`Request::AwaitInterrupt`]: the turn ended; whether a command it left was closed too.
    Interrupted {
        closed_background: bool,
    },
    /// `Chat`: the messages, oldest first, and whether older ones exist before them.
    /// `partial`: older lines of its log have not been read in yet ([`Request::Backfill`]).
    Chat {
        messages: Vec<ChatMessage>,
        /// A byte-offset cursor is valid only within this history generation.
        generation: String,
        more: bool,
        #[serde(default)]
        partial: bool,
    },
    /// [`Request::Backfill`]: messages found; `done` once the log is read back to its start.
    Backfilled {
        added: u32,
        done: bool,
    },
    /// `ToolCalls`; `tools` on the stretch's first page, `next` the cursor while more follow.
    ToolCalls {
        log: ToolLog,
        tools: Option<ChatTools>,
        rows: Vec<ToolRow>,
        next: Option<i64>,
    },
    ToolCall {
        log: ToolLog,
        row: ToolRow,
    },
    TurnChanges(crate::turn_diff::TurnChanges),
    TurnFile(crate::turn_diff::TurnFile),
    /// A terminal size in cells: what a split's new pane starts at (`SplitRoom`).
    Size {
        cols: u16,
        rows: u16,
    },
    /// A `Subscribe` stream's frames: first the state now, then each batch of changes.
    Events(Vec<crate::hook::Notice>),
    Error(String),
}

/// One message of a conversation as the index recorded it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Grows with each message recorded; a cursor for `after` and `before`. A reindexed
    /// log gets new ones, so a client that sees a gap reloads the newest.
    pub seq: i64,
    /// Seconds since the epoch.
    pub at: u64,
    /// The user's words; otherwise the Agent's.
    pub user: bool,
    pub text: String,
    /// Asked for with `Chat`'s `tools`, on the Agent's messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ChatTools>,
}

/// The tool calls logged between an Agent message and the message before it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatTools {
    pub commands: u32,
    /// `(path, added, removed)`, in the order first touched.
    pub files: Vec<(String, u32, u32)>,
    /// Seconds this stretch of work took: from your message, or from its first call when
    /// the message before was the Agent's own (it may have slept since). 0 when unknown.
    #[serde(default)]
    pub took: u64,
    /// Every tool call in it, whatever kind: a stretch of reading has no command or edit.
    #[serde(default)]
    pub calls: u32,
}

pub use crate::tool_rows::{ToolLog, ToolResultRow, ToolRow};

/// A running terminal passed from one host to the next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handed {
    pub id: String,
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
    pub title: String,
    pub cell: (u32, u32),
}
/// The program a host runs, fixed when it started: a rebuilt binary does not change a
/// running host, so a client compares this with the host it would start itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Build {
    pub binary: String,
    /// Content digest of `binary` when the host started; empty from hosts before it.
    #[serde(default)]
    pub digest: String,
    /// The host's process id, to stop it for a restart; 0 from hosts before it.
    #[serde(default)]
    pub pid: u32,
    /// The host can hand its running terminals to a newer one, so updating it ends none.
    #[serde(default)]
    pub handoff: bool,
    /// Chat input checks the current approval before paste and submit.
    #[serde(default)]
    pub guarded_chat: bool,
    /// What is off about how the host was started (`launch::of_host`); none from hosts
    /// before it, and an older window ignores it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launch: Vec<crate::launch::LaunchIssue>,
}
impl Build {
    pub fn current() -> Self {
        let binary = std::env::current_exe().unwrap_or_default();
        Self {
            digest: Self::digest(&binary).unwrap_or_default(),
            binary: binary.to_string_lossy().into_owned(),
            pid: std::process::id(),
            // Windows cannot pass a running console over: a newer host there waits for a restart.
            handoff: fibo_platform::terminal::CAN_HAND_OVER,
            guarded_chat: true,
            launch: vec![],
        }
    }
    /// Identifies a file by its bytes (FNV-1a and length), so a re-signed or copied
    /// binary with the same code is the same build whatever its timestamps say.
    pub fn digest(path: &std::path::Path) -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
        });
        Some(format!("{hash:016x}-{}", bytes.len()))
    }
    /// Whether this host runs other code than `sibling`, the host this client would
    /// start. A host that reports no digest cannot show it is the same.
    pub fn stale(&self, sibling: &std::path::Path) -> bool {
        self.digest.is_empty() || Self::digest(sibling).is_some_and(|d| d != self.digest)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecognitionPhase {
    Starting,
    Healthy,
    Interrupted,
    Recovering,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognitionMetrics {
    pub ps_spawns: u64,
    pub lsof_spawns: u64,
    pub directories: u64,
    pub log_opens: u64,
    pub log_bytes: u64,
    pub index_writes: u64,
}
/// Set in every managed terminal's environment, so whatever an Agent runs there (its
/// hooks above all) can say which terminal it is in and reach this host. A hook command
/// installed once for good stays a no-op outside Fibo, where these are unset.
pub const TERMINAL_ENV: &str = "MOORING_TERMINAL";
pub const RUNTIME_ENV: &str = "MOORING_RUNTIME";
/// The host executable: hooks run `"$MOORING_HOOK" hook --agent <cli>`.
pub const HOOK_ENV: &str = "MOORING_HOOK";
/// `1` in every managed terminal: what `fibo --skill` tells an Agent to check first.
pub const MARKER_ENV: &str = "MOORING_ENV";
/// Set by the user before a wrapper (a sandbox, a VM, `nono`) that hides the Agent's own
/// process: names the Agent it runs.
pub const AGENT_ENV: &str = "MOORING_AGENT";
/// The terminal hint for an Agent the host sees running but cannot tie to a conversation yet.
pub const AGENT_UNBOUND_PREFIX: &str = tk!("未对上对话：");
/// The terminal hint for a Claude Code that saves no transcript: it inherited the nested
/// session marker, so Fibo can never read or count it.
pub const CLAUDE_TRANSCRIPT_OFF: &str = tk!(
    "Claude 没有保存对话记录（继承了 CLAUDE_CODE_CHILD_SESSION），无法统计；关掉这个终端重开后再运行 claude"
);
/// A short note for a terminal's pane, and why, for an Agent seen there but not read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalNote {
    pub note: String,
    pub why: String,
}
/// What the program in a terminal says about itself through OSC 7501, the Program Status
/// Protocol: its own words, kept as reported. A program that says nothing has none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramReport {
    /// idle, working, done, blocked or error.
    pub state: String,
    /// permission, question or auth, when a blocked program said which.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 0..=100, when the program said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    /// Who reported, e.g. `claude-code` or a script's name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub app: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub msg: String,
    /// When the state word last changed, seconds since the epoch.
    #[serde(default)]
    pub since: u64,
}
/// A command the shell marked finished (OSC 133 `C` then `D`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LastCommand {
    /// None when the shell did not say.
    pub exit: Option<i32>,
    pub took_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognitionStatus {
    pub phase: RecognitionPhase,
    pub detail: String,
    pub scopes: Vec<String>,
    pub metrics: RecognitionMetrics,
    pub terminal_hints: std::collections::HashMap<String, String>,
    /// Terminal -> what its Agent's adapter says about it, in the user's words. Kept beside
    /// `terminal_hints` so the app shows any CLI's note without knowing which CLI it is.
    #[serde(default)]
    pub terminal_notes: std::collections::HashMap<String, TerminalNote>,
    /// Terminal -> the recognized unbound Agent, even before any transcript or note.
    #[serde(default)]
    pub terminal_agents: std::collections::HashMap<String, crate::agent::Agent>,
    /// Terminal -> the question an Agent asks before anything else: whether to trust the
    /// folder. Read off its screen; there may be no conversation yet. Live state, not stored.
    #[serde(default)]
    pub terminal_trust: std::collections::HashMap<String, TrustPrompt>,
    /// Terminal -> the title its program set, spinner dropped. Live state, not stored.
    #[serde(default)]
    pub terminal_titles: std::collections::HashMap<String, String>,
    /// Terminal -> the last command its shell marked finished. Live state, not stored.
    #[serde(default)]
    pub terminal_commands: std::collections::HashMap<String, LastCommand>,
    /// Terminal -> what its program last reported through OSC 7501, until the program
    /// ends or clears it. Live state, not stored.
    #[serde(default)]
    pub terminal_reports: std::collections::HashMap<String, ProgramReport>,
    #[serde(default)]
    pub terminal_modes: crate::modes::Modes,
    pub workspace_revision: u64,
    /// Understands [`Request::Brief`]: hosts before it drop a connection that sends one.
    #[serde(default)]
    pub brief: bool,
    /// Understands [`Request::BriefAll`], as `brief` does `Brief`.
    #[serde(default)]
    pub brief_all: bool,
    /// Bumped whenever the host reloads plugins: a `fibo plugins` in a terminal changed
    /// them, or a window did, and every window has to read them again.
    #[serde(default)]
    pub plugins_revision: u64,
    /// CLI -> why its official status query failed last; absent once one succeeds. The
    /// status rules then fall back to logs, hooks and the screen.
    #[serde(default)]
    pub official: std::collections::BTreeMap<String, String>,
    /// Phones on a relay. Live state, not stored.
    #[serde(default)]
    pub remote: RemoteStatus,
}
impl Default for RecognitionStatus {
    fn default() -> Self {
        Self {
            phase: RecognitionPhase::Starting,
            detail: t!("正在建立身份索引").into(),
            scopes: vec![],
            metrics: RecognitionMetrics::default(),
            terminal_hints: Default::default(),
            terminal_notes: Default::default(),
            terminal_trust: Default::default(),
            terminal_agents: Default::default(),
            terminal_titles: Default::default(),
            terminal_commands: Default::default(),
            terminal_reports: Default::default(),
            terminal_modes: Default::default(),
            workspace_revision: 0,
            brief: false,
            brief_all: false,
            plugins_revision: 0,
            official: Default::default(),
            remote: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests;
