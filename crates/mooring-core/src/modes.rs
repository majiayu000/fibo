//! Which model and permission mode a running Agent uses, as a client changes them: the host
//! looks at the terminal and presses the keys, so no client counts keys or guesses the state.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeKind {
    Model,
    Permission,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModeRequest {
    /// What the CLI offers now and which is on: [`ModeReply::Options`].
    Options { id: String, kind: ModeKind },
    /// Picks one by its [`ModeOption::key`]; the reply says what the CLI then shows.
    /// `allow_full` is the only way to an option marked `full`: its client asked the person.
    Set {
        id: String,
        kind: ModeKind,
        choice: String,
        #[serde(default)]
        allow_full: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModeReply {
    Options(ModeOptions),
    /// In the same words the cards use.
    Now {
        model: String,
        permission: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeOption {
    pub key: String,
    /// In the words to show.
    pub label: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub current: bool,
    /// After this it runs commands without asking.
    #[serde(default)]
    pub full: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeOptions {
    pub options: Vec<ModeOption>,
    /// Anything about the list a person should read: what is not on it, and why.
    #[serde(default)]
    pub note: String,
}

/// What the last change set on a terminal: a log says so only when the CLI next writes, which
/// for a model is its next answer. `log_*` is what the log said then; once it says something
/// else, it is newer and this no longer counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalModes {
    pub session: Option<String>,
    #[serde(default)]
    pub model: Option<(String, String)>,
    #[serde(default)]
    pub permission: Option<(String, String)>,
}

/// Terminal -> what was last set there. Live state, not stored.
pub type Modes = HashMap<String, TerminalModes>;

impl TerminalModes {
    /// `field` of a card whose log says `logged`: what was set, until the log moves on.
    pub fn read(set: &Option<(String, String)>, logged: &str) -> Option<String> {
        set.as_ref()
            .filter(|(_, then)| then == logged)
            .map(|(now, _)| now.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_set_counts_until_the_log_says_something_else() {
        let set = Some(("Sonnet 5.5".to_owned(), "claude-opus".to_owned()));
        assert_eq!(
            TerminalModes::read(&set, "claude-opus").as_deref(),
            Some("Sonnet 5.5")
        );
        assert_eq!(TerminalModes::read(&set, "claude-sonnet-5-5"), None);
        assert_eq!(TerminalModes::read(&None, "x"), None);
    }
}
