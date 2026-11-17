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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<Workflow>,
}
/// A workflow stays one child row; attempts are already reduced to logical steps by
/// its adapter. An unknown journal format keeps the summary but has no expansion.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    pub phase: String,
    pub phases: Vec<String>,
    pub steps: Vec<WorkflowStep>,
    pub detailed: bool,
    pub total: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub key: String,
    pub phase: String,
    pub agent: Subagent,
}
impl Workflow {
    pub fn completed(&self, live: bool) -> usize {
        self.steps
            .iter()
            .filter(|s| !s.agent.stopped && (!live || !s.agent.running()))
            .count()
    }
}
impl Subagent {
    pub fn running(&self) -> bool {
        self.ended.is_none()
    }
}
