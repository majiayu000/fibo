//! `Request::Edit`: one change to the workspace, and what a resend of it gets.
use super::*;
use mooring_core::protocol::RequestId;

/// Each edit names the objects it changes and, where it depends on what its sender saw,
/// says so itself (`Edit::SetOutcome`'s `seen`); the workspace revision moves with every
/// line an Agent writes and is never a precondition. A named edit takes effect once: a
/// resend gets the first outcome back, the workspace now if it was applied.
pub(super) fn edit(shared: &Shared, edit: Edit, request: Option<RequestId>) -> io::Result<Reply> {
    let mut host = lock(shared)?;
    if let Some(r) = &request {
        if let Some(first) = host.store.settled(r)? {
            return first
                .map(|()| Reply::Workspace(host.snapshot()))
                .map_err(io::Error::other);
        }
    }
    let refused = match apply(&mut host, edit, request.as_ref())? {
        Ok(reply) => return Ok(reply),
        Err(refused) => refused,
    };
    if let Some(r) = &request {
        host.store.refused(r, &refused)?;
    }
    Err(io::Error::other(refused))
}

/// The inner error refuses the edit for good: a resend gets it again. The outer one (the
/// store, the index) did not get as far as deciding.
fn apply(
    host: &mut Host,
    mut edit: Edit,
    request: Option<&RequestId>,
) -> io::Result<Result<Reply, String>> {
    if let Edit::SaveSessionTask { session, task } = &mut edit {
        let Some(s) = host
            .workspace
            .agent_sessions
            .iter()
            .find(|s| &s.id == session)
        else {
            return Ok(Err("会话不存在".into()));
        };
        task.description = host.index.original_words(&s.transcript)?;
        if task.description.is_empty() {
            return Ok(Err("已索引内容没有用户原话，不能建立待办".into()));
        }
        if host
            .workspace
            .content
            .get(session)
            .is_some_and(|c| c.partial_history)
        {
            task.description
                .push_str("\n\n（历史仅覆盖索引尾段及之后的增量；此前要求未知。）");
        }
    }
    // A pane split from outside the app must leave both halves usable, judged by the
    // size the pane is shown at now. The app's own splits are the user's call.
    if let Edit::SplitPane {
        target,
        axis,
        share,
        forced: false,
        ..
    } = &edit
    {
        if let Err(e) = split_room(host, target, *axis, *share) {
            return Ok(Err(e.to_string()));
        }
    }
    // Closing a tab or a pane ends what ran in it; placement alone never does.
    let stopped = match &edit {
        Edit::CloseTab { id, .. } => host.workspace.tab_terminals(id),
        Edit::ClosePane { terminal } => vec![terminal.clone()],
        Edit::RemoveSpace { id } => host
            .workspace
            .tabs
            .iter()
            .filter(|t| &t.space == id)
            .flat_map(|t| host.workspace.tab_terminals(&t.id))
            .collect(),
        _ => Vec::new(),
    };
    let mut next = host.workspace.clone();
    if let Err(e) = next.edit(edit) {
        return Ok(Err(e));
    }
    super::last_dir::remember(host, &stopped, &mut next);
    host.commit_applying(next, request)?;
    for id in &stopped {
        if let Some(Err(e)) = host.terminals.get(id).map(stop_terminal) {
            eprintln!("stop closed terminal {id}: {e}");
        }
    }
    // One that had already ended goes now; the rest go as they end.
    if !stopped.is_empty() {
        host.closed.extend(stopped);
        release_ended(host);
    }
    Ok(Ok(Reply::Workspace(host.snapshot())))
}
