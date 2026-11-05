//! The replies about what the Agents on a machine have used and may still use.
use serde::{Deserialize, Serialize};

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
