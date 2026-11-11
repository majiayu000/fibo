//! The progress panel: what Agents wrote back on a todo, newest first, each line leading
//! to the conversation that wrote it. The user's own words stay in 说明, above it.
use super::*;
use mooring_core::model::TaskLog;
use mooring_core::time::ago;

/// A todo noted in a conversation keeps that origin; everything else is progress.
pub(super) fn has_progress(log: Option<&TaskLog>) -> bool {
    log.is_some_and(|log| log.source.is_some() || !log.notes.is_empty())
}

impl Workbench {
    pub(super) fn progress_panel(&self, task: &Task, cx: &Context<Self>) -> Option<gpui::Div> {
        let log = self
            .workspace
            .task_logs
            .get(&task.id)
            .filter(|log| has_progress(Some(log)))?;
        let at = now();
        let mut panel = panel().child(panel_head(t!("进展"), log.notes.len().to_string()));
        for (i, note) in log.notes.iter().enumerate().rev() {
            panel = panel.child(
                div()
                    .px(px(T::S16))
                    .py(px(T::S10))
                    .flex()
                    .flex_col()
                    .gap(px(T::S4))
                    .border_t_1()
                    .border_color(T::color(T::border()))
                    .child(
                        div()
                            .text_color(T::color(T::body()))
                            .child(note.text.clone()),
                    )
                    .child(self.origin_line(
                        ("progress", i),
                        ago(note.at, at),
                        note.session.as_deref(),
                        cx,
                    )),
            );
        }
        if let Some(source) = &log.source {
            let when = log.created.map_or(String::new(), |c| ago(c, at));
            panel = panel.child(
                div()
                    .px(px(T::S16))
                    .py(px(T::S10))
                    .border_t_1()
                    .border_color(T::color(T::border()))
                    .child(self.origin_line(("noted", 0), when, Some(source), cx)),
            );
        }
        Some(panel)
    }
    /// `10 分钟前 · 来自「会话」`; the name opens that conversation, when it still exists.
    fn origin_line(
        &self,
        key: (&'static str, usize),
        when: String,
        session: Option<&str>,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let session = session.and_then(|id| {
            self.workspace
                .agent_sessions
                .iter()
                .find(|s| s.id == id)
                .cloned()
        });
        let line = div()
            .id(key)
            .flex()
            .flex_wrap()
            .gap(px(T::S4))
            .text_size(px(T::CHIP_SIZE))
            .text_color(T::color(T::muted()))
            .child(when);
        match session {
            Some(s) => {
                let name = self.workspace.session_title(&s);
                line.child("·").child(
                    div()
                        .id((key.0, key.1 + 1))
                        .cursor_pointer()
                        .hover(|d| d.text_color(T::color(T::link())))
                        .child(t!("来自「{from}」", from = &name))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.nav.task_detail = None;
                            this.enter_card(&s, window, cx);
                        })),
                )
            }
            None => line,
        }
    }
}
