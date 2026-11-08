//! A message sent while the Agent works: queue it for after the step at hand, or put it into
//! the step now (steer). Each CLI has both, on different keys, measured on the real CLIs
//! (docs/phone-app/cli-turn-input.md): Claude Code queues on Enter and steers on Ctrl+Enter
//! (`ESC[13;5u`, the kitty key code it reads); Codex steers on Enter and queues on Tab.
//! `send` with `how` pastes the text, then presses that Agent's key for it; without `how` it
//! presses Enter as before.
use super::{named, ok_reply, text};
use mooring_core::agent::Agent;
use mooring_core::protocol::*;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

/// The keys for each way, for a CLI measured to have both.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct TurnKeys {
    pub queue: &'static [u8],
    pub steer: &'static [u8],
}

pub(super) fn keys(agent: Agent) -> Option<TurnKeys> {
    if agent == Agent::Claude {
        Some(TurnKeys {
            queue: b"\r",
            steer: b"\x1b[13;5u",
        })
    } else if agent == Agent::Codex {
        Some(TurnKeys {
            queue: b"\t",
            steer: b"\r",
        })
    } else {
        None
    }
}

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
        .and_then(keys)
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

    #[test]
    fn each_measured_cli_queues_and_steers_on_its_own_keys() {
        let claude = keys(Agent::Claude).unwrap();
        assert_eq!(
            (claude.queue, claude.steer),
            (&b"\r"[..], &b"\x1b[13;5u"[..])
        );
        let codex = keys(Agent::Codex).unwrap();
        assert_eq!((codex.queue, codex.steer), (&b"\t"[..], &b"\r"[..]));
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
