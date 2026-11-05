//! The replies about what a terminal's screen asks of its user.
use serde::{Deserialize, Serialize};

/// The reply to `ask`: what an approval asks about, read on the machine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    /// The question on screen, as a card's `approval` has it.
    pub question: String,
    /// The title as the CLI writes it: `Bash command`, `Edit file`.
    pub kind: String,
    /// The whole command, the file, the tool and its arguments. Empty when the screen's
    /// layout is not one the machine knows: nothing is guessed.
    pub subject: String,
    /// What the CLI says it does.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// A file's new contents or a diff, as the prompt shows them.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// The subject ran off the top of what the machine could read: there is more of it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}
