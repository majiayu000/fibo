//! Writes that change this machine's workbench for a client elsewhere (multi-machine
//! step 4): open a terminal, end one, settle a conversation. Each may carry the client's
//! `request` id and then takes effect once, like the window's own edits.
use super::{named_id, text};
use mooring_core::{model::*, protocol::*};
use serde_json::{json, Value};
use std::io;
use std::path::Path;

/// `open`: a shell in a new tab of the space that holds `cwd` (else the first space),
/// started in `cwd` (else the space's directory). Replies `{terminal, tab}`.
pub(super) fn open(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let w = match rpc(socket, &Request::Workspace)? {
        Reply::Workspace(w) => w,
        other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    };
    let asked = request["cwd"].as_str().filter(|c| !c.is_empty());
    let space = asked
        .and_then(|cwd| {
            (w.spaces.iter())
                .filter(|s| !s.cwd.is_empty() && Path::new(cwd).starts_with(&s.cwd))
                .max_by_key(|s| s.cwd.len())
        })
        .or_else(|| w.spaces.first())
        .ok_or_else(|| io::Error::other("那台电脑还没有空间"))?;
    let cwd = asked.map_or_else(|| space.cwd.clone(), str::to_owned);
    let cwd = Some(cwd)
        .filter(|c| Path::new(c).is_dir())
        .or_else(|| std::env::var("HOME").ok())
        .ok_or_else(|| io::Error::other("目录不存在"))?;
    // Named after the request, a resend finds what the first one opened.
    let name = named_id(client, request).map_or_else(
        || uuid::Uuid::new_v4().simple().to_string(),
        |r| format!("{}-{}", r.client, r.id).replace([':', '/'], "-"),
    );
    let (terminal, tab) = (format!("remote-{name}"), format!("remote-tab-{name}"));
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let launch = Request::Launch {
        id: terminal.clone(),
        space: space.id.clone(),
        argv: vec![shell, "-l".into()],
        cwd,
        cols: 100,
        rows: 30,
        scheme: Default::default(),
    };
    rpc(socket, &launch)?;
    if !w.tabs.iter().any(|t| t.id == tab) {
        let place = Edit::CreateTab {
            tab: Tab {
                id: tab.clone(),
                space: space.id.clone(),
                default_task: None,
                title: String::new(),
                layout: Some(mooring_core::layout::Layout::Leaf(terminal.clone())),
            },
        };
        let placed = rpc(
            socket,
            &Request::Edit {
                request: None,
                edit: place,
            },
        );
        // A resend that raced the first one finds the tab already there.
        if let Err(e) = placed {
            if !e.to_string().contains("标签已存在") {
                return Err(e);
            }
        }
    }
    Ok(json!({ "terminal": terminal, "tab": tab }))
}

/// `end`: closes a terminal's pane and stops it.
pub(super) fn end(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let edit = Edit::ClosePane {
        terminal: text(request, "terminal")?.to_owned(),
    };
    edit_once(socket, request, client, edit)
}

/// `outcome`: `done`, `abandoned` or `open` for a conversation. With the `turn` the
/// client saw (a card's `turn`), refused once the Agent began another turn.
pub(super) fn outcome(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let id = text(request, "session")?.to_owned();
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
    let seen = match request["turn"].as_str() {
        None => None,
        Some(turn) => {
            let w = match rpc(socket, &Request::Workspace)? {
                Reply::Workspace(w) => w,
                other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
            };
            let now = (w.agent_sessions.iter().find(|s| s.id == id))
                .ok_or_else(|| io::Error::other("会话不存在"))?;
            // The turn is what the client saw; who else changed the outcome it cannot say.
            Some(SeenSession {
                outcome: now.outcome.clone(),
                turn: turn.to_owned(),
            })
        }
    };
    edit_once(
        socket,
        request,
        client,
        Edit::SetOutcome { id, outcome, seen },
    )
}

fn edit_once(
    socket: &Path,
    request: &Value,
    client: Option<&str>,
    edit: Edit,
) -> io::Result<Value> {
    let request = named_id(client, request);
    match rpc(socket, &Request::Edit { request, edit })? {
        Reply::Workspace(_) => Ok(json!({})),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}
