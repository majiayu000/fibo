//! How full a conversation's context window is. Only what the CLI itself states: Codex writes
//! the window in its log, and Claude Code hands it to its status line, which the `claude-quota`
//! plugin saves per conversation. Without either nothing is shown: windows differ by model and
//! plan (a 1M window is common), so a guessed one would be wrong in both directions.
use crate::model::Workspace;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Below this nothing is shown: there is nothing to decide yet.
pub const SHOWN_FROM: u8 = 50;
/// From here it is close to being compacted.
pub const NEAR: u8 = 80;
/// The plugin keeping Claude Code's status line, and its folder of one file per conversation.
const STATUS_PLUGIN: &str = "claude-quota";
const STATUS_FOLDER: &str = "contexts";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fill {
    pub used: u64,
    pub window: u64,
}

impl Fill {
    fn new(used: u64, window: u64) -> Option<Self> {
        (window > 0).then_some(Self { used, window })
    }
    pub fn percent(&self) -> u8 {
        (self.used.saturating_mul(100) / self.window.max(1)).min(100) as u8
    }
    pub fn shown(&self) -> bool {
        self.percent() >= SHOWN_FROM
    }
    pub fn near(&self) -> bool {
        self.percent() >= NEAR
    }
    /// What the ring's hover says: 「上下文 84% · 已用 840K / 1M」.
    pub fn text(&self) -> String {
        t!(
            "上下文 {pct}% · 已用 {used} / {max}",
            pct = self.percent(),
            used = tokens(self.used),
            max = tokens(self.window),
        )
    }
}

fn tokens(n: u64) -> String {
    if n >= 1_000_000 {
        let tenths = n / 100_000;
        if tenths.is_multiple_of(10) {
            format!("{}M", tenths / 10)
        } else {
            format!("{}.{}M", tenths / 10, tenths % 10)
        }
    } else {
        format!("{}K", n / 1000)
    }
}

/// Codex's `token_count`'s `info`: the last request's input against the window it states.
pub fn from_codex(info: &Value) -> Option<Fill> {
    Fill::new(
        info["last_token_usage"]["input_tokens"].as_u64()?,
        info["model_context_window"].as_u64()?,
    )
}

/// Claude Code's status line. `current_usage` is null before the first reply.
pub fn from_claude_status(doc: &Value) -> Option<Fill> {
    let window = &doc["context_window"];
    let usage = window["current_usage"].as_object()?;
    let read = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    Fill::new(
        read("input_tokens")
            + read("cache_creation_input_tokens")
            + read("cache_read_input_tokens"),
        window["context_window_size"].as_u64()?,
    )
}

/// A conversation id is used as a file name only if it cannot leave the folder.
fn plain_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parsed files by when they changed: this runs about once a second on the window's thread.
static READ: Mutex<Option<Parsed>> = Mutex::new(None);
type Parsed = HashMap<PathBuf, (SystemTime, Option<Fill>)>;
/// Each open conversation's reading, by session id.
pub type Fills = FxHashMap<String, Fill>;

fn saved_fill(folder: &Path, id: &str) -> Option<Fill> {
    if !plain_id(id) {
        return None;
    }
    let path = folder.join(format!("{id}.json"));
    let changed = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    let mut read = READ.lock().unwrap_or_else(|e| e.into_inner());
    let read = read.get_or_insert_with(HashMap::new);
    if let Some((at, fill)) = read.get(&path)
        && *at == changed
    {
        return *fill;
    }
    let fill = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|doc| from_claude_status(&doc));
    read.insert(path, (changed, fill));
    fill
}

/// `of`, with the plugins directory given: what the conversation's log states, else what a
/// plugin saved for it. Only the CLIs that hand it over have either.
pub fn of_in(w: &Workspace, plugins: Option<&Path>) -> Fills {
    let folder = plugins.map(|p| crate::setup::data_dir(p, STATUS_PLUGIN).join(STATUS_FOLDER));
    w.agent_sessions
        .iter()
        .filter(|s| s.current)
        .filter_map(|s| {
            let logged = w.content.get(&s.id).and_then(|c| c.context);
            let fill = logged.or_else(|| saved_fill(folder.as_deref()?, &s.cli_session_id))?;
            Some((s.id.clone(), fill))
        })
        .collect()
}

/// `of_in` for this install.
pub fn of(w: &Workspace) -> Fills {
    let plugins = crate::plugin::config().map(crate::plugin::dir);
    of_in(w, plugins.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn percent_shown_and_near_follow_their_lines() {
        let at = |used| Fill::new(used, 1000).unwrap();
        assert_eq!(at(499).percent(), 49);
        assert!(!at(499).shown());
        assert!(at(500).shown() && !at(500).near());
        assert!(at(800).near());
        // A request can pass the window just before it is compacted.
        assert_eq!(at(2000).percent(), 100);
        assert_eq!(Fill::new(1, 0), None);
    }

    #[test]
    fn the_hover_names_both_numbers() {
        let fill = Fill::new(840_000, 1_000_000).unwrap();
        assert_eq!(fill.text(), "上下文 84% · 已用 840K / 1M");
        assert_eq!(tokens(1_500_000), "1.5M");
        assert_eq!(tokens(475_000), "475K");
    }

    #[test]
    fn each_cli_is_read_as_it_writes() {
        let codex =
            json!({"last_token_usage": {"input_tokens": 237_500}, "model_context_window": 475_000});
        assert_eq!(from_codex(&codex).unwrap().percent(), 50);
        assert_eq!(
            from_codex(&json!({"last_token_usage": {"input_tokens": 1}})),
            None
        );
        // The numbers of a real status line: a 1M window, most of it read from the cache.
        let claude = json!({"context_window": {"context_window_size": 1_000_000, "current_usage": {
            "input_tokens": 2, "output_tokens": 1931,
            "cache_creation_input_tokens": 2040, "cache_read_input_tokens": 369_092}}});
        let fill = from_claude_status(&claude).unwrap();
        assert_eq!((fill.used, fill.percent()), (371_134, 37));
        let before_a_reply =
            json!({"context_window": {"context_window_size": 200_000, "current_usage": null}});
        assert_eq!(from_claude_status(&before_a_reply), None);
    }

    #[test]
    fn a_reading_comes_from_the_log_or_what_the_plugin_saved_and_never_for_a_closed_session() {
        let plugins = std::env::temp_dir().join(format!("kl-fills-{}", std::process::id()));
        let folder = crate::setup::data_dir(&plugins, STATUS_PLUGIN).join(STATUS_FOLDER);
        std::fs::create_dir_all(&folder).unwrap();
        let session = |id: &str, current| crate::model::AgentSession {
            id: id.into(),
            runtime: crate::model::Agent::Codex,
            cli_session_id: format!("cli-{id}"),
            terminal: None,
            current,
            task: None,
            outcome: Default::default(),
            updated: 0,
            cwd: String::new(),
            transcript: String::new(),
            name: String::new(),
            binding: None,
        };
        let saved = json!({"context_window": {"context_window_size": 100, "current_usage": {"input_tokens": 60}}});
        for id in ["cli-saved", "cli-gone"] {
            std::fs::write(folder.join(format!("{id}.json")), saved.to_string()).unwrap();
        }
        let mut w = Workspace {
            agent_sessions: vec![
                session("logged", true),
                session("saved", true),
                session("gone", false),
                session("none", true),
            ],
            ..Default::default()
        };
        w.content.entry("logged".into()).or_default().context = Fill::new(90, 100);
        let fills = of_in(&w, Some(&plugins));
        assert_eq!(fills["logged"].percent(), 90);
        assert_eq!(fills["saved"].percent(), 60);
        assert_eq!(fills.len(), 2);
        // Without the plugin only what the log states is left.
        assert_eq!(of_in(&w, None).len(), 1);
        let _ = std::fs::remove_dir_all(&plugins);
    }

    #[test]
    fn an_id_cannot_name_a_file_outside_the_folder() {
        assert!(plain_id("c09ef1b2-1aca-45a6"));
        for bad in ["", "../x", "a/b", "a.json", "a b"] {
            assert!(!plain_id(bad), "{bad}");
        }
    }
}
