//! 标记完成 on an Agent no conversation is tied to yet. The host reads its screen: a
//! conversation it can name finishes as any other, and without one nothing is recorded,
//! because a guess could mark someone else's conversation done.
use super::*;

impl Workbench {
    /// Whether `terminal` runs an Agent Mooring has said it has not tied to a conversation.
    pub(super) fn unbound_agent(&self, terminal: &str) -> bool {
        self.current_agent(terminal).is_none()
            && self
                .store
                .recognition()
                .terminal_notes
                .contains_key(terminal)
    }
    pub(super) fn complete_unbound(&mut self, terminal: String, cx: &mut Context<Self>) {
        let asked = self.store.send(Request::AssociateScreen {
            id: terminal.clone(),
        });
        let task = cx.background_spawn(asked);
        cx.spawn(async move |this, cx| {
            let reply = task.await;
            let _ = this.update(cx, |this, cx| {
                match reply {
                    Ok(Reply::ScreenAssociated {
                        session: Some(session),
                    }) => this.complete_pane(terminal, session, cx),
                    Ok(Reply::ScreenAssociated { session: None }) => {
                        this.layout.confirm_close = Some(Closing::Unmatched(terminal));
                    }
                    Ok(other) => this.error = Some(format!("认对话时后台答非所问：{other:?}")),
                    Err(e) => this.error = Some(format!("认不出对话：{e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
