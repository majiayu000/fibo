//! A message sent while the Agent works: queue it for after the step at hand, or put it into
//! the step now (steer). Each CLI's adapter says which keys do which (`Adapter::turn_keys`).
//! `send` with `how` pastes the text, then presses that Agent's key for it; without `how` it
//! presses Enter as before.
use super::{named, ok_reply, text};
use fibo_core::protocol::*;
use serde_json::Value;
use std::io;
use std::path::Path;

/// `how` on a `send`: None when absent, so the send presses Enter as it always did.
pub(super) fn how(request: &Value) -> io::Result<Option<bool>> {
    match request.get("how").and_then(Value::as_str) {
        None => Ok(None),
        Some("queue") => Ok(Some(false)),
        Some("steer") => Ok(Some(true)),
        Some(other) => Err(io::Error::other(t!(
            "how 只能是 queue 或 steer，不是 {other}",
            other
        ))),
    }
}

/// Paste and submit at the host, which checks the live screen before either write.
pub(super) fn send(
    socket: &Path,
    request: &Value,
    client: Option<&str>,
    session: Option<String>,
    steer: bool,
) -> io::Result<Value> {
    let input = Request::SendTurn(TurnInput {
        session,
        id: text(request, "terminal")?.to_owned(),
        text: text(request, "text")?.to_owned(),
        steer,
    });
    ok_reply(rpc(socket, &named(client, request, input))?)
}

/// Sends what the Agent has queued now, without waiting for the step at hand: its steer key
/// with nothing typed. On Claude Code 2.1 the queue says `ctrl+enter to send now`, and that
/// key on an empty box sends every queued message at once (measured 2026-11-09).
pub(super) fn send_now(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let input = Request::SendTurn(TurnInput {
        session: request["session"].as_str().map(str::to_owned),
        id: text(request, "terminal")?.to_owned(),
        text: String::new(),
        steer: true,
    });
    ok_reply(rpc(socket, &named(client, request, input))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibo_core::agent::Agent;
    use fibo_host::{agent_adapter, TurnKeys};
    use serde_json::json;

    #[test]
    fn each_measured_cli_queues_and_steers_on_its_own_keys() {
        let keys = |a| agent_adapter(a).turn_keys();
        let claude = TurnKeys {
            queue: b"\r",
            steer: b"\x1b[13;5u",
        };
        assert_eq!(keys(Agent::Claude), Some(claude));
        let codex = TurnKeys {
            queue: b"\t",
            steer: b"\r",
        };
        assert_eq!(keys(Agent::Codex), Some(codex));
        assert_eq!(keys(Agent::Grok), None);
    }

    #[test]
    fn how_is_queue_steer_or_absent() {
        assert_eq!(how(&json!({})).unwrap(), None);
        assert_eq!(how(&json!({ "how": "queue" })).unwrap(), Some(false));
        assert_eq!(how(&json!({ "how": "steer" })).unwrap(), Some(true));
        assert!(how(&json!({ "how": "now" })).is_err());
    }
}
