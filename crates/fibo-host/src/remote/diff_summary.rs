//! `diff` for a role without `dirs` (只看, 能答审批): the files this terminal's current turn
//! changed and their line counts, as the workbench already worked them out from the
//! conversation's log (`TurnChanges`, the window's 看改动). No git runs in a work tree for
//! such a role. A folder may hold what a stranger wrote, and git reads that folder's own
//! config, attributes and hooks: three reviews of a list of what to refuse each found
//! another way through (#121, #125), so the list went and git with it.
use fibo_core::protocol::*;
use fibo_remote::protocol as rp;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

pub(super) const BY_TERMINAL_ONLY: &str = "这台设备只能看终端里这一轮改了哪些文件";

pub(super) fn diff(socket: &Path, request: &Value) -> io::Result<Value> {
    // A folder names nothing the workbench has worked out.
    let (Some(terminal), None) = (request["terminal"].as_str(), request["cwd"].as_str()) else {
        return Err(io::Error::other(BY_TERMINAL_ONLY));
    };
    let w = match rpc(socket, &Request::Workspace)? {
        Reply::Workspace(w) => w,
        other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    };
    let term = (w.terminals.iter().find(|x| x.id == terminal))
        .ok_or_else(|| io::Error::other("终端不存在"))?;
    let cwd = w.terminal_cwd(term);
    let files = match w.current_agent(terminal) {
        Some(session) => match rpc(
            socket,
            &Request::TurnChanges {
                session: session.id.clone(),
            },
        )? {
            Reply::TurnChanges(c) => c.files,
            Reply::Error(e) => return Err(io::Error::other(e)),
            other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        },
        None => Vec::new(),
    };
    let files: Vec<rp::DiffFile> = files
        .into_iter()
        .map(|f| rp::DiffFile {
            path: within(&cwd, &f.path),
            added: f.added,
            removed: f.removed,
            status: if f.created { "A" } else { "M" }.into(),
            patch: String::new(),
            truncated: false,
        })
        .collect();
    Ok(json!(rp::Diff {
        cwd,
        branch: String::new(),
        added: files.iter().map(|f| f.added).sum(),
        removed: files.iter().map(|f| f.removed).sum(),
        files,
        truncated: false,
        checks: super::diff::checks(&w, terminal),
        summary: true,
    }))
}

/// A path inside the terminal's folder as the full role's diff would name it.
fn within(cwd: &str, path: &str) -> String {
    Path::new(path)
        .strip_prefix(cwd)
        .ok()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| path.to_owned(), |p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn paths_inside_the_folder_are_named_from_it() {
        assert_eq!(super::within("/w/p", "/w/p/src/a.rs"), "src/a.rs");
        assert_eq!(super::within("/w/p", "/w/pq/a.rs"), "/w/pq/a.rs");
        assert_eq!(super::within("/w/p", "/etc/hosts"), "/etc/hosts");
        assert_eq!(super::within("/w/p", "/w/p"), "/w/p");
    }
}
