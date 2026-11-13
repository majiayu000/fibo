//! Todos from a phone: note one down, and close or reopen one. Closing is the user's call
//! (`fibo todo` refuses it to Agents); a phone is the user's own hand, so it may.
use super::{text, writes::edit_once};
use fibo_core::{model::*, protocol::*};
use serde_json::Value;
use std::io;
use std::path::Path;

/// `todoadd`: `title`, the `space` id it goes in, and optionally `said`, your words.
pub(super) fn add(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let title = text(request, "title")?.trim().to_owned();
    if title.is_empty() {
        return Err(io::Error::other("待办要有标题"));
    }
    let w = workspace(socket)?;
    let space = text(request, "space")?;
    let space = (w.spaces.iter().find(|s| s.id == space))
        .ok_or_else(|| io::Error::other("没有这个空间"))?;
    let task = Task {
        id: uuid::Uuid::new_v4().to_string(),
        space: space.id.clone(),
        title,
        description: request["said"].as_str().unwrap_or("").trim().to_owned(),
        acceptance: vec![],
        agent: w.settings.default_agent,
    };
    edit_once(
        socket,
        request,
        client,
        Edit::AddTaskFrom {
            task,
            session: None,
        },
    )
}

/// `todooutcome`: `task` and `outcome`, `done`, `abandoned` or `open` (to undo either).
pub(super) fn outcome(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let id = text(request, "task")?.to_owned();
    let outcome = match text(request, "outcome")? {
        "done" => Outcome::Completed,
        "abandoned" => Outcome::Abandoned,
        "open" => Outcome::Open,
        other => {
            return Err(io::Error::other(format!(
                "outcome 只能是 done、abandoned 或 open，不是 {other}"
            )))
        }
    };
    if !workspace(socket)?.tasks.iter().any(|t| t.id == id) {
        return Err(io::Error::other("这个待办不在了"));
    }
    edit_once(
        socket,
        request,
        client,
        Edit::SetTaskOutcome {
            id,
            outcome,
            seen: None,
        },
    )
}

pub(super) fn workspace(socket: &Path) -> io::Result<Workspace> {
    match rpc(socket, &Request::Workspace)? {
        Reply::Workspace(w) => Ok(w),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}
