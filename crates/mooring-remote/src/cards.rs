//! The replies of `cards` and `modes`.
use crate::protocol::{Card, Machine, Space, Todo};
use serde::{Deserialize, Serialize};

/// The reply to `modes`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModeOptions {
    pub options: Vec<ModeOption>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeOption {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub current: bool,
    /// After this it runs commands without asking; needs `allow_full`.
    #[serde(default)]
    pub full: bool,
}

/// The reply to `cards`: [`List`] as what changed since the `rev` asked about. An absent field
/// has not changed; all of them absent, nothing has.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cards {
    /// What to send as `since` next, on this connection only.
    pub rev: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<Machine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spaces: Option<Vec<Space>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todos: Option<Vec<Todo>>,
    /// Every running terminal in order, when the set or its order changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<String>>,
    /// The cards that differ, whole.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<Card>,
}

/// Capabilities of the actual host adapter; old hosts leave them absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentControls {
    pub steer: bool,
    pub send_now: bool,
    pub effort: bool,
    #[serde(default)]
    pub suggestion: bool,
    pub busy_input: Option<String>,
}
