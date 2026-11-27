//! Marking a session done asks about the todos it is tied to (noted in it, or worked on by
//! it): 一起完成 completes them. Only what core links (`session_todos`); nothing guessed,
//! and nothing is put in front of an Agent.
use super::*;

/// Long enough to read a title and decide.
const OFFER_FOR: Duration = Duration::from_secs(10);

impl Workbench {
    /// Called as a session's 标记完成 is sent. Next frame, so a toast the same click
    /// shows (the board's 撤销) is kept: its undo stays beside 一起完成.
    pub(super) fn offer_session_todos(&mut self, session: &str, cx: &mut Context<Self>) {
        let todos: Vec<(String, String)> = (self.workspace.session_todos(session, now()))
            .into_iter()
            .map(|t| (t.id.clone(), t.title.clone()))
            .collect();
        let Some((_, title)) = todos.first().cloned() else {
            return;
        };
        let text = match todos.len() {
            1 => t!("这个会话关联的待办「{title}」也完成吗？", title).to_owned(),
            n => t!("这个会话关联的 {n} 条待办也完成吗？", n).to_owned(),
        };
        let ids = todos.into_iter().map(|(id, _)| id).collect();
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| {
                let undo = this.overview.toast.take().and_then(|t| t.undo);
                this.put_toast(text, undo, false, ids, OFFER_FOR, cx);
            });
        })
        .detach();
    }
}
