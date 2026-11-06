//! Transcript and SQLite source declarations shared by file-backed adapters.
use super::agent::{EventRule, assistant, jsonl, user};
use serde::Deserialize;
use std::path::PathBuf;

/// Where a declared CLI writes JSON-lines transcripts and how a record names things.
/// Pointers are RFC 6901 JSON pointers into each record.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transcripts {
    /// Absolute, or under the home with `~/`. Left out when a `source` writes them.
    #[serde(default)]
    pub root: PathBuf,
    #[serde(default = "jsonl")]
    pub extension: String,
    pub session: String,
    pub cwd: String,
    /// Parent session id on child records; absent or null on a top-level session.
    #[serde(default)]
    pub parent: Option<String>,
    /// Which record is whose: `role` points at a value equal to `user` or `assistant`.
    pub role: Option<String>,
    #[serde(default = "user")]
    pub user: String,
    #[serde(default = "assistant")]
    pub assistant: String,
    /// The message text of a user or assistant record.
    pub text: Option<String>,
    /// api 2: the conversation's name as the CLI keeps it; the latest non-empty value a
    /// record holds names the card, unless the user named it in Mooring.
    #[serde(default)]
    pub title: Option<String>,
    /// api 2: records that mark a turn, a question for the user, a tool call or its
    /// result, so a declared CLI groups like a built-in one. Every matching rule applies.
    #[serde(default)]
    pub events: Vec<EventRule>,
}

/// A conversation kept in a SQLite database instead of transcript files. The daemon polls
/// it read-only and writes each session's new rows as JSON lines into a directory of its
/// own, which then flows through the same watching, parsing and binding as any transcript:
/// the `transcripts` pointers and `events` apply to those rows.
///
/// `sessions` is asked `?1` = the largest `_t` it returned before, and returns `_session`,
/// `_cwd` and `_t` per changed session. Its other columns (such as `_title` or
/// `_parent`) form a metadata record, written before messages and on each change. `records` is asked `?1` = a session, `?2`/`?3` =
/// the `_t`/`_id` of the last row it returned for it, and returns rows after that, in
/// order, each with `_t` and `_id`. A column holding JSON text is read as JSON.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordSource {
    pub sqlite: PathBuf,
    pub sessions: String,
    pub records: String,
    #[serde(default = "poll_ms")]
    pub poll_ms: u64,
}
fn poll_ms() -> u64 {
    1500
}
