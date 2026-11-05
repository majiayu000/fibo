//! `lastsession`: which conversation 「接着上次的对话」 would pick up. `open` with `resume`
//! then runs it the way the window does (`writes::open`).
use mooring_core::agent::Agent;
use mooring_core::{model::*, protocol::*, status};
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

/// The conversation last worked on in `cwd` with `agent`, among those the window lists:
/// hidden ones (sub-agents, print-mode runs) are not, and one that never got a question has
/// nothing to go back to.
pub(super) fn last(w: &Workspace, cwd: &str, agent: Agent) -> Option<rp::LastSession> {
    let cwd = cwd.trim_end_matches('/');
    let shown = |s: &&AgentSession| {
        s.runtime == agent
            && s.cwd.trim_end_matches('/') == cwd
            && w.content
                .get(&s.id)
                .is_some_and(|c| c.hidden_for(s).is_none() && !c.asks.is_empty())
    };
    let s = (w.agent_sessions.iter().filter(shown)).max_by_key(|s| w.session_updated(s))?;
    Some(rp::LastSession {
        id: s.id.clone(),
        title: w.session_title(s),
        updated: w.session_updated(s),
        terminal: status::live_terminal(w, s).map(|t| t.id.clone()),
    })
}

pub(super) fn last_session(socket: &Path, request: &Value) -> io::Result<Value> {
    let cwd = super::dirs::expand(super::text(request, "cwd")?)?;
    let agent = super::writes::agent_named(super::text(request, "agent")?)?;
    let w = match rpc(socket, &Request::Workspace)? {
        Reply::Workspace(w) => w,
        other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    };
    let session = last(&w, &cwd.to_string_lossy(), agent);
    Ok(json!(rp::Last { session }))
}
