use serde::{Deserialize, Serialize};

/// The host checks the conversation and approval screen before paste and submit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnInput {
    pub session: Option<String>,
    pub id: String,
    pub text: String,
    pub steer: bool,
}

/// An Agent asking to be trusted with a folder before it does anything.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustPrompt {
    pub agent: crate::agent::Agent,
    /// Its words.
    pub question: String,
    /// The folder as its screen writes it.
    pub folder: String,
}

/// An Agent asking, before any conversation, whether to update itself first (Codex's
/// 「Update available」). It waits there until you choose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePrompt {
    pub agent: crate::agent::Agent,
    /// Its words: the versions from and to.
    pub question: String,
}
impl UpdatePrompt {
    /// Its line in the window and on the phone's card.
    pub fn note(&self) -> String {
        t!(
            "{agent} · {state} · 要不要更新 {name}",
            agent = self.agent.executable(),
            state = crate::status::Status::Waiting.label(),
            name = self.agent.name()
        )
    }
}
