//! The replies to `toolcalls` and `toolcall`: what an Agent did between two of its messages,
//! read from its raw log and with secrets hidden before they leave the machine.
use serde::{Deserialize, Serialize};

/// The reply to `toolcalls`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCalls {
    pub session: String,
    /// On the stretch's first page: what it adds up to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<crate::protocol::Tools>,
    /// Oldest first. A page can come back shorter than asked: a script whose commands have
    /// rows of their own says nothing they do not.
    pub calls: Vec<ToolCallInfo>,
    /// More follow: ask again with this as `after`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<i64>,
    /// The raw log is gone: labels are what the index kept, sizes unknown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallInfo {
    /// For `toolcall`'s `call`.
    pub id: String,
    /// The tool as the CLI names it.
    pub name: String,
    /// `command`, `edit`, `read`, `search`, `web`, `agent` or `other`.
    pub kind: String,
    /// One line: the command, the file, the search.
    pub label: String,
    /// What the Agent said it does, in its own words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    pub at: u64,
    /// How long it took, when the log can say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub took_ms: Option<u64>,
    /// A result says so: an error, or a non-zero exit.
    pub failed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    /// No result logged yet: still going, or it never came.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub open: bool,
    /// Bytes of output as logged; absent when it could not be read in time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// The reply to `toolcall`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallDetail {
    #[serde(flatten)]
    pub info: ToolCallInfo,
    /// How the input reads: `command`, `diff`, `code`, `text` or `json`.
    pub input_kind: String,
    pub input: ToolText,
    /// Absent while it has no result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<ToolText>,
    /// What a script does, step by step, where it could be read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ToolStep>,
    /// What could not be read, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolStep {
    pub tool: String,
    pub label: String,
    /// Built at run time: the label says so and nothing more can be read.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unread: bool,
}

/// A long text's two ends. Offsets are bytes of the text as sent, with secrets hidden.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolText {
    /// The whole text's bytes.
    pub total: u64,
    pub head: String,
    /// Empty when `head` is the whole text.
    pub tail: String,
    /// Where `tail` begins; what lies between the end of `head` and this is left to fetch.
    pub tail_at: u64,
}

/// The reply to `toolcall` with `part`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSlice {
    pub part: String,
    pub offset: u64,
    pub text: String,
    /// Where the next stretch begins; the text's end when this was the last.
    pub next: u64,
    pub total: u64,
}
