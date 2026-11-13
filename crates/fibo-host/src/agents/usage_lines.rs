//! The tokens one log line says were used, as each CLI writes them.
//!
//! - Claude Code logs one line per block of a reply, each with the whole reply's `usage`:
//!   a reply counts once, by its message id, across files too (a resumed session copies
//!   its history). Sub-agents' logs sit under the same root and count.
//! - Codex logs a running total with each `token_count`: its `last_token_usage` counts
//!   when the total grew, so a repeated event adds nothing.
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Deserialize)]
struct ClaudeLine {
    #[serde(rename = "type")]
    ty: Option<String>,
    timestamp: Option<Value>,
    message: Option<ClaudeMessage>,
}
#[derive(Deserialize)]
struct ClaudeMessage {
    id: Option<String>,
    usage: Option<ClaudeUsage>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct ClaudeUsage {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_input_tokens: u64,
    cache_read_input_tokens: u64,
}

pub(super) fn claude_line(
    line: &[u8],
    since: u64,
    seen: &mut HashSet<String>,
) -> Option<(u64, u64)> {
    let v: ClaudeLine = serde_json::from_slice(line).ok()?;
    if v.ty.as_deref() != Some("assistant") || at(v.timestamp.as_ref()) < since {
        return None;
    }
    let message = v.message?;
    let usage = message.usage?;
    if !seen.insert(message.id?) {
        return None;
    }
    Some((
        usage.input_tokens + usage.cache_creation_input_tokens + usage.output_tokens,
        usage.cache_read_input_tokens,
    ))
}

#[derive(Deserialize)]
struct CodexLine {
    timestamp: Option<Value>,
    payload: Option<CodexPayload>,
}
#[derive(Deserialize)]
struct CodexPayload {
    info: Option<CodexInfo>,
}
#[derive(Deserialize)]
struct CodexInfo {
    total_token_usage: Option<CodexUsage>,
    last_token_usage: Option<CodexUsage>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct CodexUsage {
    /// Holds `cached_input_tokens`: the two are not added.
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

pub(super) fn codex_line(line: &[u8], since: u64, running: &mut u64) -> Option<(u64, u64)> {
    let v: CodexLine = serde_json::from_slice(line).ok()?;
    let info = v.payload?.info?;
    let total = info.total_token_usage?.total_tokens;
    if total <= *running {
        return None;
    }
    *running = total;
    let last = info.last_token_usage?;
    (at(v.timestamp.as_ref()) >= since).then(|| {
        (
            last.input_tokens.saturating_sub(last.cached_input_tokens) + last.output_tokens,
            last.cached_input_tokens,
        )
    })
}

fn at(v: Option<&Value>) -> u64 {
    v.map_or(0, crate::content::seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_claude_reply_counts_once_however_many_lines_it_took() {
        let line = |id: &str, at: &str| {
            json!({"type":"assistant","timestamp":at,"message":{"id":id,"usage":{"input_tokens":2,"cache_creation_input_tokens":10,"cache_read_input_tokens":100,"output_tokens":5}}}).to_string().into_bytes()
        };
        let mut seen = HashSet::new();
        let since = 1_793_750_000;
        let today = "2026-11-05T00:00:00Z";
        assert_eq!(
            claude_line(&line("m1", today), since, &mut seen),
            Some((17, 100))
        );
        assert_eq!(claude_line(&line("m1", today), since, &mut seen), None);
        assert_eq!(
            claude_line(&line("m0", "2026-11-01T00:00:00Z"), since, &mut seen),
            None,
            "before today"
        );
        let replayed = json!({"type":"api-request-blob","timestamp":today,"message":{"id":"m9","usage":{"output_tokens":5}}}).to_string();
        assert_eq!(claude_line(replayed.as_bytes(), since, &mut seen), None);
    }

    #[test]
    fn a_codex_total_counts_when_it_grows_and_only_todays_part() {
        let line = |at: &str, total: u64, input: u64, cached: u64, output: u64| {
            json!({"timestamp":at,"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":total},"last_token_usage":{"input_tokens":input,"cached_input_tokens":cached,"output_tokens":output,"total_tokens":input+output}}}}).to_string().into_bytes()
        };
        let since = 1_793_750_000;
        let mut running = 0;
        let old = "2026-11-01T00:00:00Z";
        let today = "2026-11-05T00:00:00Z";
        assert_eq!(
            codex_line(&line(old, 100, 90, 0, 10), since, &mut running),
            None
        );
        assert_eq!(
            codex_line(&line(today, 160, 50, 30, 10), since, &mut running),
            Some((30, 30))
        );
        assert_eq!(
            codex_line(&line(today, 160, 50, 30, 10), since, &mut running),
            None,
            "the same event again"
        );
    }
}
