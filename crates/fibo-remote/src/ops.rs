//! The ops a machine may list in `hello`. `image` keeps a picture for a later `send`;
//! `steer` says `send` takes `how` (`queue` or `steer`, see the host's `turn_input`); `effort`
//! says `modes` and `setmode` take the kind `effort`; `spacequiet` puts a space aside or takes it
//! back, and `list` names the ones put aside in `quiet`. `sendnow` sends what the Agent has
//! queued without waiting for the step at hand; `toolslast` says `toolcalls` takes `last`;
//! `openimages` says `open` takes `images`, pictures for the new Agent's first message;
//! `spacename` says `addspace` takes `name`, and without `path` makes a space with no folder.
use crate::protocol::Op;

/// Every op, in the order `hello` lists them.
pub const OPS: &[&str] = &[
    "hello",
    "list",
    "cards",
    "read",
    "send",
    "keys",
    "input",
    "answer",
    "ask",
    "trust",
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
    "toolcalls",
    "toolcall",
    "backfill",
    "todoadd",
    "todooutcome",
    "suggestion",
    "modes",
    "setmode",
    "image",
    "steer",
    "effort",
    "spacequiet",
    "sendnow",
    "toolslast",
    "openimages",
    "spacename",
    "updatespace",
];

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
            Self::Ask { .. } => "ask",
            Self::Trust { .. } => "trust",
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
            Self::UpdateSpace { .. } => "updatespace",
            Self::Usage { .. } => "usage",
            Self::ToolCalls { .. } => "toolcalls",
            Self::Backfill { .. } => "backfill",
            Self::ToolCall { .. } => "toolcall",
            Self::Suggestion { .. } => "suggestion",
            Self::Cards { .. } => "cards",
            Self::Modes { .. } => "modes",
            Self::SetMode { .. } => "setmode",
        }
    }
}
