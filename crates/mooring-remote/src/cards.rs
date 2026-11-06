//! The reply of `cards`.
use crate::protocol::{Card, Machine, Space, Todo};
use serde::{Deserialize, Serialize};

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
