use super::*;

pub(super) fn run(shared: &Shared, id: &str, place: bool) -> io::Result<Reply> {
    let mut host = lock(shared)?;
    let w = &host.workspace;
    let Some(t) = w
        .terminals
        .iter()
        .find(|t| t.id == id && t.state == ProcessState::Running)
    else {
        return Err(io::Error::other(t!("无窗格终端已退出或已放入窗格")));
    };
    if w.terminal_tab(id).is_some() {
        return Err(io::Error::other(t!("无窗格终端已退出或已放入窗格")));
    }
    let live = host
        .terminals
        .get(id)
        .ok_or_else(|| io::Error::other(t!("无窗格终端已退出或已放入窗格")))?;
    if !place {
        // Keep the layout stable until its own process group has been signalled.
        stop_terminal(live)?;
        return Ok(Reply::Ok);
    }
    let space = w
        .spaces
        .iter()
        .find(|space| space.id == t.space)
        .or_else(|| w.spaces.first())
        .ok_or_else(|| io::Error::other(t!("空间不存在")))?;
    let mut next = w.clone();
    next.edit(Edit::CreateTab {
        tab: fibo_core::model::Tab {
            id: uuid::Uuid::new_v4().to_string(),
            space: space.id.clone(),
            default_task: None,
            title: String::new(),
            layout: Some(fibo_core::layout::Layout::Leaf(id.to_owned())),
        },
    })
    .map_err(io::Error::other)?;
    host.commit(next)?;
    Ok(Reply::Workspace(host.snapshot()))
}
