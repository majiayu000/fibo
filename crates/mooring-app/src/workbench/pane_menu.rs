//! The pane menu's rows, and its 中断: for an Agent the host decides how and watches it end,
//! the same two requests as a phone's 停下. Ctrl-C would open a menu on a running Codex and
//! quit an idle one; in a shell it is what the row says.
use super::*;

pub(super) const INTERRUPT_TURN: &str = "中断这一轮";
pub(super) const INTERRUPT_SHELL: &str = "中断（Ctrl-C）";

/// The pane menu's items in four groups (None draws a rule between them): layout,
/// text, the session and its todo, then what stops or closes something.
/// `agent`: the terminal holds a conversation.
pub(super) fn menu_rows(zoomed: bool, agent: bool) -> Vec<Option<&'static str>> {
    vec![
        Some("向右分屏"),
        Some("向下分屏"),
        Some(if zoomed { "还原" } else { "放大" }),
        Some("移到标签…"),
        None,
        Some("复制"),
        Some("粘贴"),
        Some("搜索滚动内容"),
        Some("在编辑器里看滚动内容"),
        None,
        Some("为什么是这个状态"),
        Some("为这个待办开一个会话"),
        Some("挂到待办…"),
        Some("取消关联"),
        None,
        Some(if agent {
            INTERRUPT_TURN
        } else {
            INTERRUPT_SHELL
        }),
        Some("停止 Agent"),
        Some("关闭画面"),
    ]
}

/// Why 中断这一轮 is off: an Agent that is not working has no turn to interrupt, and a key
/// meant for one could quit it. None: it is on.
pub(super) fn interrupt_off(phase: Option<Phase>, interrupting: bool) -> Option<&'static str> {
    if interrupting {
        return Some("正在中断，等它停下");
    }
    (phase != Some(Phase::Working)).then_some("它这会儿没在干活，没有要中断的")
}

impl Workbench {
    pub(super) fn interrupt_blocked(&self, session: Option<&AgentSession>) -> Option<&'static str> {
        let phase = session.map(|s| status_of(&self.workspace, s).phase());
        interrupt_off(phase, self.layout.interrupting.is_some())
    }
    /// Interrupts the pane's turn. The reply comes when it has ended; what stops it from
    /// ending shows in the notice strip. One at a time: a second key could land on whatever
    /// replaced the menu the first one answered.
    pub(super) fn interrupt_turn(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.layout.interrupting.is_some() {
            return;
        }
        self.layout.interrupting = Some(id.to_owned());
        let first = self.store.send(Request::Interrupt { id: id.to_owned() });
        let then = self
            .store
            .send(Request::AwaitInterrupt { id: id.to_owned() });
        let task = cx.background_spawn(async move {
            first.await?;
            then.await
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.layout.interrupting = None;
                if let Err(e) = result {
                    this.error = Some(e.to_string());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row says what it does in each kind of pane: a shell gets Ctrl-C and says so, an
    /// Agent gets whatever its CLI takes, which is not Ctrl-C for Codex.
    #[test]
    fn the_interrupt_row_names_what_it_does() {
        let row = |agent| {
            menu_rows(false, agent)
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
        };
        assert!(row(true).contains(&INTERRUPT_TURN));
        assert!(!row(true).iter().any(|r| r.contains("Ctrl-C")));
        assert!(row(false).contains(&INTERRUPT_SHELL));
        assert!(!row(false).contains(&INTERRUPT_TURN));
    }

    /// Only a working turn can be interrupted; on an idle Codex a key meant for it would quit.
    #[test]
    fn interrupting_is_only_for_a_turn_that_is_working() {
        assert_eq!(interrupt_off(Some(Phase::Working), false), None);
        for idle in [Phase::Idle, Phase::Waiting] {
            assert!(interrupt_off(Some(idle), false).is_some());
        }
        assert!(interrupt_off(None, false).is_some());
        assert!(
            interrupt_off(Some(Phase::Working), true).is_some(),
            "one at a time"
        );
    }
}
