//! Grok's parent-owned task metadata. A summary's parent_session_id is fork ancestry,
//! not delegation; only subagents/<id>/meta.json owns the task relationship and outcome.
use mooring_core::content::Subagent;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) fn parent(meta: &Path) -> Option<PathBuf> {
    let subs = meta.parent()?.parent()?;
    if meta.file_name()? != "meta.json" || subs.file_name()? != "subagents" {
        return None;
    }
    Some(subs.parent()?.join("updates.jsonl"))
}
fn time(v: &Value) -> Option<u64> {
    u64::try_from(
        chrono::DateTime::parse_from_rfc3339(v.as_str()?)
            .ok()?
            .timestamp(),
    )
    .ok()
}
pub(super) fn read(transcript: &Path) -> Vec<Subagent> {
    let Some(session) = transcript.parent() else {
        return vec![];
    };
    let Some(parent_id) = session.file_name().and_then(|n| n.to_str()) else {
        return vec![];
    };
    let Ok(entries) = std::fs::read_dir(session.join("subagents")) else {
        return vec![];
    };
    let mut out: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let v: Value =
                serde_json::from_slice(&std::fs::read(entry.path().join("meta.json")).ok()?)
                    .ok()?;
            if v["parent_session_id"].as_str()? != parent_id {
                return None;
            }
            let id = v["subagent_id"].as_str()?.to_owned();
            let ended = time(&v["completed_at"]);
            Some(Subagent {
                name: v["description"]
                    .as_str()
                    .filter(|n| !n.trim().is_empty())
                    .or_else(|| v["subagent_type"].as_str())
                    .unwrap_or(&id)
                    .to_owned(),
                id,
                started: time(&v["started_at"])?,
                ended,
                stopped: ended.is_some() && v["status"] != "completed",
                // The persisted metadata doesn't identify foreground versus background.
                foreground: false,
            })
        })
        .collect();
    out.sort_by(|a, b| (a.started, &a.id).cmp(&(b.started, &b.id)));
    out
}
