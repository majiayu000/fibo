use super::*;
use crate::ui::theme::Flat as F;
use crate::workbench::sidebar_flat::ONE_LINE_ROW;

impl Workbench {
    pub(super) fn history_row(
        &self,
        listed: Listed,
        current: Option<&str>,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let (session, status, attention) = listed;
        let id = session.id.as_str();
        // Keep the selected row and rows that can change height in the ordinary layout.
        if current == Some(id)
            || self.sidebar.motion.moving()
            || self.layout.done_open.as_deref() == Some(id)
            || self
                .layout
                .renaming_session
                .as_ref()
                .is_some_and(|(s, ..)| s == id)
            || self
                .workspace
                .content
                .get(id)
                .is_some_and(|c| !c.subagents.is_empty())
        {
            return self.session_row(listed, current, None, End::State, cx);
        }
        #[cfg(feature = "visual-test")]
        if crate::workbench::visual::full_history() {
            return self.session_row(listed, current, None, End::State, cx);
        }

        let workbench = cx.entity().downgrade();
        let session = session.clone();
        // Reserve the row's exact place in the scroll layout. Prepaint knows this frame's
        // position and clip, including a jump or a change in the rows above it.
        gpui::canvas(
            move |bounds, window, cx| {
                if !bounds.intersects(&window.content_mask().bounds) {
                    #[cfg(feature = "visual-test")]
                    crate::workbench::visual::skipped_history();
                    return None;
                }
                let mut row = workbench
                    .update(cx, |w, cx| {
                        w.flat_row(
                            w,
                            None,
                            (&session, status, attention),
                            None,
                            None,
                            End::State,
                            cx,
                        )
                        .mx(px(0.))
                        .w_full()
                        .into_any_element()
                    })
                    .ok()?;
                row.prepaint_as_root(bounds.origin, bounds.size.into(), window, cx);
                Some(row)
            },
            |_, row, window, cx| {
                if let Some(mut row) = row {
                    row.paint(window, cx);
                }
            },
        )
        .mx(px(F::ROW_EDGE))
        .h(px(ONE_LINE_ROW))
        .flex_shrink_0()
        .into_any_element()
    }
}
