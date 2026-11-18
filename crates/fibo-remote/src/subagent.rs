//! A subagent a session started, as the window keeps it and as a card carries it to
//! another machine's window.
use serde::{Deserialize, Serialize};

/// One subagent a session started: its description (its type without one), and when it
/// started and ended.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subagent {
    /// The CLI's id for it, which a task notification names it by.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub started: u64,
    /// `None` while it runs.
    #[serde(default)]
    pub ended: Option<u64>,
    /// The parent turn waits on it.
    #[serde(default)]
    pub foreground: bool,
    /// It failed or was stopped before finishing.
    #[serde(default)]
    pub stopped: bool,
}
impl Subagent {
    pub fn running(&self) -> bool {
        self.ended.is_none()
    }
}
