//! Where a conversation's tool calls are in its raw log, as the host answers `ToolCalls`.
use serde::{Deserialize, Serialize};

/// The raw log a conversation's tool rows point into, and the CLI that wrote it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLog {
    pub path: String,
    pub runtime: crate::model::Agent,
}

/// A tool call as the index has it: `text` is `Name {arguments}`, long arguments cut down
/// to the fields read later, so the whole of it is only in its raw line at `offset`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRow {
    pub seq: i64,
    pub id: String,
    pub text: String,
    pub at: u64,
    pub inode: u64,
    pub offset: u64,
    /// Its results, oldest first: a command read back later has several.
    pub results: Vec<ToolResultRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultRow {
    pub seq: i64,
    pub at: u64,
    pub code: Option<i32>,
    pub inode: u64,
    pub offset: u64,
}
