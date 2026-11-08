//! A message sent while the Agent works: queue it for after the step at hand, or put it into
//! the step now (steer). Each CLI's adapter says which keys do which (`Adapter::turn_keys`).
//! `send` with `how` pastes the text, then presses that Agent's key for it; without `how` it
//! presses Enter as before.
use super::{named, ok_reply, text};
use mooring_core::agent::Agent;
use mooring_core::protocol::*;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

/// `how` on a `send`: None when absent, so the send presses Enter as it always did.
pub(super) fn how(request: &Value) -> io::Result<Option<bool>> {
    match request.get("how").and_then(Value::as_str) {
        None => Ok(None),
        Some("queue") => Ok(Some(false)),
        Some("steer") => Ok(Some(true)),
        Some(other) => Err(io::Error::other(format!(
            "how 只能是 queue 或 steer，不是 {other}"
        ))),
    }
}

/// The Agent running in a terminal now, as `modes` finds it.
fn agent_in(socket: &Path, terminal: &str) -> io::Result<Option<Agent>> {
    let Reply::Workspace(w) = rpc(socket, &Request::Workspace)? else {
        return Err(io::Error::other("unexpected reply"));
    };
    let seen = match rpc(socket, &Request::RecognitionStatus) {
        Ok(Reply::Recognition(r)) => r,
        _ => Default::default(),
    };
    Ok((w.current_agent(terminal).map(|s| s.runtime))
        .or_else(|| seen.terminal_agents.get(terminal).copied()))
}

/// Pastes `text`, then presses the key that queues it or puts it in now. Refused, before
/// anything is typed, for a terminal whose Agent has no such keys.
pub(super) fn send(
    socket: &Path,
    request: &Value,
    client: Option<&str>,
    session: Option<String>,
    steer: bool,
) -> io::Result<Value> {
    let terminal = text(request, "terminal")?.to_owned();
    let agent = agent_in(socket, &terminal)?;
    let keys = agent
        .and_then(|a| mooring_host::agent_adapter(a).turn_keys())
        .ok_or_else(|| io::Error::other("这个 Agent 分不出排队和马上发，直接发送就好"))?;
    let paste = Request::SendText {
        session,
        id: terminal.clone(),
        text: text(request, "text")?.to_owned(),
        submit: false,
    };
    let mut first = request.clone();
    if let Some(id) = request["request"].as_str() {
        first["request"] = json!(format!("{id}#text"));
    }
    ok_reply(rpc(socket, &named(client, &first, paste))?)?;
    let press = Request::SendInput {
        id: terminal,
        bytes: (if steer { keys.steer } else { keys.queue }).to_vec(),
    };
    ok_reply(rpc(socket, &named(client, request, press))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooring_host::{agent_adapter, TurnKeys};

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
