//! Writes that change this machine's workbench for a client elsewhere (multi-machine
//! step 4, phone v2): open a terminal (with an Agent and its first message, or a
//! conversation to pick back up), end one, settle a conversation, interrupt an Agent, make
//! a folder or a space. Each may carry the client's `request` id and then takes effect
//! once, like the window's own edits.
use super::{named, named_id, ok_reply, text};
use crate::control;
use mooring_core::{agent::Agent, model::*, protocol::*, status};
use mooring_platform::shell;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

/// An Agent as cards name it (`claude`, `codex`): any CLI the registry knows.
pub(super) fn agent_named(name: &str) -> io::Result<Agent> {
    (Agent::all().into_iter())
        .find(|a| control::agent_label(*a) == name)
        .ok_or_else(|| io::Error::other(format!("不认识的 agent {name}")))
}

/// `open`: a shell in a new tab of the space that holds `cwd` (else the first space),
/// started in `cwd` (else the space's directory). Replies `{terminal, tab}`. With `agent`
/// the shell runs it, `prompt` its first message, as the window resumes a conversation;
/// with `resume` it picks that conversation back up, in its own directory; with `task` it
/// starts that todo's Agent in its space, handed the todo.
pub(super) fn open(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let agent = match request["agent"].as_str().filter(|a| !a.is_empty()) {
        None => None,
        Some(name) => Some(agent_named(name)?),
    };
    let w = super::todos::workspace(socket)?;
    // A todo to work on: its Agent, in its space, handed its words and progress so far,
    // and the conversation started is counted as working for it.
    let task = match request["task"].as_str().filter(|t| !t.is_empty()) {
        None => None,
        Some(id) => Some(
            (w.tasks.iter().find(|t| t.id == id))
                .ok_or_else(|| io::Error::other("这个待办不在了"))?,
        ),
    };
    let agent = agent.or(task.map(|t| t.agent));
    let handoff = task.map(|t| w.task_handoff(t, "fibo todo note"));
    let prompt =
        (request["prompt"].as_str().filter(|p| !p.trim().is_empty())).or(handoff.as_deref());
    let resume = request["resume"].as_str().filter(|r| !r.is_empty());
    if prompt.is_some() && agent.is_none() && resume.is_none() {
        return Err(io::Error::other("prompt 要和 agent 一起"));
    }
    // Named after the request, a resend finds what the first one opened.
    let name = named_id(client, request).map_or_else(
        || uuid::Uuid::new_v4().simple().to_string(),
        |r| format!("{}-{}", r.client, r.id).replace([':', '/'], "-"),
    );
    let (terminal, tab) = (format!("remote-{name}"), format!("remote-tab-{name}"));
    let session = resume
        .map(|id| {
            (w.agent_sessions.iter().find(|s| s.id == id))
                .ok_or_else(|| io::Error::other("这段对话找不到了"))
        })
        .transpose()?;
    // Running already: never a second copy of it. The resend of this very open is not that.
    if let Some(live) = session.and_then(|s| status::live_terminal(&w, s)) {
        if live.id != terminal {
            let tab = w.terminal_tab(&live.id).map(|t| t.id.clone());
            return Ok(json!({ "terminal": live.id, "tab": tab, "existing": true }));
        }
    }
    let asked = session
        .map(|s| s.cwd.as_str())
        .or_else(|| request["cwd"].as_str())
        .or_else(|| {
            let space = task.map(|t| t.space.as_str())?;
            (w.spaces.iter().find(|s| s.id == space)).map(|s| s.cwd.as_str())
        })
        .filter(|c| !c.is_empty());
    // Picked on the phone: that space and no other. Without one, a todo's own space; else
    // (an older phone) the space whose folder holds the directory, else the first not put aside.
    let picked = request["space"].as_str().filter(|s| !s.is_empty());
    let picked = picked
        .map(|id| {
            (w.spaces.iter().find(|s| s.id == id))
                .ok_or_else(|| io::Error::other("这个空间已经不在了"))
        })
        .transpose()?;
    let space = picked
        .or_else(|| task.and_then(|t| w.spaces.iter().find(|s| s.id == t.space)))
        .or_else(|| {
            asked.and_then(|cwd| {
                (w.spaces.iter())
                    .filter(|s| !s.cwd.is_empty() && Path::new(cwd).starts_with(&s.cwd))
                    .max_by_key(|s| s.cwd.len())
            })
        })
        .or_else(|| (w.spaces.iter()).find(|s| !w.space_quiet(&s.id)))
        .or_else(|| w.spaces.first())
        .cloned();
    // A computer with no space yet (a fresh install, never opened in the window): the folder
    // becomes its first, rather than nothing opening at all. A resend finds it by its folder.
    let space = match space {
        Some(space) => space,
        None => {
            let folder = asked
                .filter(|c| Path::new(c).is_dir())
                .map(str::to_owned)
                .or_else(mooring_platform::dirs::home_str)
                .ok_or_else(|| io::Error::other("那台电脑还没有空间"))?;
            let folder = folder.trim_end_matches('/').to_owned();
            let space = Space {
                id: uuid::Uuid::new_v4().simple().to_string(),
                name: (Path::new(&folder).file_name())
                    .map_or_else(|| folder.clone(), |n| n.to_string_lossy().into_owned()),
                cwd: folder,
            };
            match rpc(
                socket,
                &Request::Edit {
                    request: None,
                    edit: Edit::AddSpace(space.clone()),
                },
            )? {
                Reply::Workspace(_) => space,
                other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
            }
        }
    };
    let space = &space;
    // Asked for by hand, so it is back; and the list leaves a space put aside out, so what
    // opened there would vanish from the phone.
    if w.space_quiet(&space.id) {
        let edit = Edit::SetSpaceQuiet {
            id: space.id.clone(),
            quiet: false,
        };
        match rpc(
            socket,
            &Request::Edit {
                edit,
                request: None,
            },
        )? {
            Reply::Workspace(_) => (),
            other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        }
    }
    // A conversation's log belongs to its directory: resumed anywhere else, the CLI would
    // not find it.
    if session.is_some() && !asked.is_some_and(|c| Path::new(c).is_dir()) {
        return Err(io::Error::other("这段对话的目录已经不在了"));
    }
    let cwd = asked.map_or_else(|| space.cwd.clone(), str::to_owned);
    let cwd = Some(cwd)
        .filter(|c| Path::new(c).is_dir())
        .or_else(mooring_platform::dirs::home_str)
        .ok_or_else(|| io::Error::other("目录不存在"))?;
    let shell = shell::interactive();
    let command = match session {
        Some(s) => Some(w.resume_command(s)),
        None => agent.map(|a| a.command()),
    };
    let launch = Request::Launch {
        id: terminal.clone(),
        space: space.id.clone(),
        argv: command.map_or_else(|| shell::stay(&shell), |c| agent_argv(&shell, &c, prompt)),
        cwd,
        cols: 100,
        rows: 30,
        scheme: Default::default(),
        colors: None,
    };
    rpc(socket, &launch)?;
    if !w.tabs.iter().any(|t| t.id == tab) {
        let place = Edit::CreateTab {
            tab: Tab {
                id: tab.clone(),
                space: space.id.clone(),
                default_task: (task.map(|t| t.id.clone()))
                    .or_else(|| session.and_then(|s| s.task.clone())),
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
    Ok(json!({ "terminal": terminal, "tab": tab, "space": space.id }))
}

/// `addspace`: a folder as a space of its own, named after it. One that already is a space
/// keeps it, so a resend after a lost reply finds what the first one made.
pub(super) fn add_space(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let path = super::dirs::expand(super::text(request, "path")?)?;
    if !path.is_dir() {
        return Err(io::Error::other("目录不存在"));
    }
    let cwd = path.to_string_lossy().trim_end_matches('/').to_owned();
    let w = match rpc(socket, &Request::Workspace)? {
        Reply::Workspace(w) => w,
        other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    };
    let space = match (w.spaces.iter()).find(|s| s.cwd.trim_end_matches('/') == cwd) {
        Some(space) => space.clone(),
        None => {
            let space = Space {
                id: uuid::Uuid::new_v4().simple().to_string(),
                name: (path.file_name())
                    .map_or_else(|| cwd.clone(), |n| n.to_string_lossy().into_owned()),
                cwd,
            };
            edit_once(socket, request, client, Edit::AddSpace(space.clone()))?;
            space
        }
    };
    Ok(json!({ "id": space.id, "name": space.name, "cwd": space.cwd }))
}

/// 先不管 a space, or take it back: the same switch as the window's. Its conversations come
/// back to the phone on the next `list`.
pub(super) fn space_quiet(
    socket: &Path,
    request: &Value,
    client: Option<&str>,
) -> io::Result<Value> {
    let id = super::text(request, "space")?.to_owned();
    let quiet =
        (request["quiet"].as_bool()).ok_or_else(|| io::Error::other("quiet 要是 true 或 false"))?;
    edit_once(socket, request, client, Edit::SetSpaceQuiet { id, quiet })?;
    Ok(json!({}))
}

/// The shell running `command` and then itself, as the window's resume does (`launch_argv`).
/// The prompt goes as an argument the shell does not read.
fn agent_argv(shell: &str, command: &str, prompt: Option<&str>) -> Vec<String> {
    // `--`: a prompt starting with `-` is not read as a flag.
    let command = match prompt {
        Some(p) if p.starts_with('-') => format!("{command} --"),
        _ => command.to_owned(),
    };
    shell::run_then_stay(shell, &command, prompt)
}

/// `stop`: interrupts the Agent's turn, as the window's 中断这一轮 does. The host checks it is
/// working inside the named act, so a resend gets the first reply even though it has stopped
/// since. A key sent is not a turn stopped: this replies once the host says so.
pub(super) fn stop(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let id = text(request, "terminal")?.to_owned();
    let interrupt = Request::Interrupt { id: id.clone() };
    ok_reply(rpc(socket, &named(client, request, interrupt))?)?;
    match rpc(socket, &Request::AwaitInterrupt { id })? {
        Reply::Interrupted { closed_background } => {
            Ok(json!({ "closed_background": closed_background }))
        }
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

/// `mkdir`: one new folder in an existing one. A folder already there is what was asked
/// for, so a resend after a lost reply succeeds too.
pub(super) fn mkdir(request: &Value) -> io::Result<Value> {
    let path = super::dirs::expand(text(request, "path")?)?;
    let parent = (path.parent())
        .filter(|p| p.is_dir())
        .ok_or_else(|| io::Error::other("上一级文件夹不存在"))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.is_empty() || name == "." || name == ".." {
        return Err(io::Error::other("文件夹名不对"));
    }
    match std::fs::create_dir(parent.join(name)) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Err(io::Error::other("这里已经有同名文件"))
        }
        r => r?,
    }
    Ok(json!({ "path": path.to_string_lossy() }))
}

/// `end`: closes a terminal's pane and stops it.
pub(super) fn end(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let edit = Edit::ClosePane {
        terminal: text(request, "terminal")?.to_owned(),
    };
    edit_once(socket, request, client, edit)
}

/// How long a conversation put off from a client stays out of the way.
const DEFERRED_FOR: u64 = 3600;

/// `outcome`: `done`, `deferred` (back in an hour), `abandoned` or `open` for a
/// conversation. With the `turn` the client saw (a card's `turn`), refused once the Agent began another turn.
pub(super) fn outcome(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let id = text(request, "session")?.to_owned();
    let outcome = match text(request, "outcome")? {
        "done" => Outcome::Completed,
        "deferred" => Outcome::Deferred {
            reason: "在手机上选了稍后提醒".into(),
            until: mooring_core::time::now() + DEFERRED_FOR,
        },
        "abandoned" => Outcome::Abandoned,
        "open" => Outcome::Open,
        other => {
            return Err(io::Error::other(format!(
                "outcome 只能是 done、deferred、abandoned 或 open，不是 {other}"
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

pub(super) fn edit_once(
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
