//! `modes` and `setmode`: the phone's model and permission tags. The host looks at the terminal
//! and presses the keys (`host/client/modes.rs`); this only carries the request.
use super::{named, text};
use fibo_core::modes::{ModeKind, ModeReply, ModeRequest};
use fibo_core::protocol::*;
use fibo_remote::protocol as rp;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

fn kind(request: &Value) -> io::Result<ModeKind> {
    match text(request, "kind")? {
        "model" => Ok(ModeKind::Model),
        "permission" => Ok(ModeKind::Permission),
        "effort" => Ok(ModeKind::Effort),
        other => Err(io::Error::other(format!("不认识的 kind {other}"))),
    }
}

pub(super) fn options(socket: &Path, request: &Value) -> io::Result<Value> {
    let ask = ModeRequest::Options {
        id: text(request, "terminal")?.to_owned(),
        kind: kind(request)?,
    };
    match rpc(socket, &Request::Modes(ask))? {
        Reply::Modes(ModeReply::Options(o)) => Ok(json!(rp::ModeOptions {
            note: o.note,
            options: (o.options.into_iter())
                .map(|x| rp::ModeOption {
                    key: x.key,
                    label: x.label,
                    detail: x.detail,
                    current: x.current,
                    full: x.full,
                })
                .collect(),
        })),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

pub(super) fn set(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let ask = ModeRequest::Set {
        id: text(request, "terminal")?.to_owned(),
        kind: kind(request)?,
        choice: text(request, "choice")?.to_owned(),
        // Only a literal `true`: nothing else a client sends opens the full mode.
        allow_full: request["allow_full"] == Value::Bool(true),
    };
    match rpc(socket, &named(client, request, Request::Modes(ask)))? {
        Reply::Modes(ModeReply::Now {
            model,
            permission,
            permission_full,
            effort,
        }) => Ok(
            json!({ "model": model, "permission": permission, "permission_full": permission_full, "effort": effort }),
        ),
        // A resend after a lost reply: the first one took.
        Reply::Ok => Ok(json!({})),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

/// `keys` cannot walk an Agent's permission modes: that is `setmode`, which alone may open the
/// mode that stops asking.
pub(super) fn not_cycling(socket: &Path, request: &Value) -> io::Result<()> {
    let cycles = request["keys"]
        .as_array()
        .is_some_and(|k| k.iter().any(|k| k.as_str() == Some("shift-tab")));
    if !cycles {
        return Ok(());
    }
    let terminal = text(request, "terminal")?;
    let Reply::Workspace(w) = rpc(socket, &Request::Workspace)? else {
        return Err(io::Error::other("unexpected reply"));
    };
    let Reply::Recognition(seen) = rpc(socket, &Request::RecognitionStatus)? else {
        return Err(io::Error::other("unexpected reply"));
    };
    let agent = (w.current_agent(terminal).map(|s| s.runtime))
        .or_else(|| seen.terminal_agents.get(terminal).copied());
    if agent.is_some_and(fibo_host::agent_modes_switch) {
        return Err(io::Error::other("换权限要用权限标签，不能直接按 Shift+Tab"));
    }
    Ok(())
}
