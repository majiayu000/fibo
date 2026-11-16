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
