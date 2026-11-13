use super::*;

impl Workbench {
    /// What a row's second line says: what it stopped at (the question it asks you, or
    /// the error), what it is doing in words (`正在改 tabs.rs`, never the raw command), or
    /// how its last turn ended. One line, cut to fit. The pane header's 停在 line reads
    /// the same.
    pub(in crate::workbench) fn row_detail(&self, s: &AgentSession, g: Status) -> Option<String> {
        let first = |t: &str| {
            let line = fibo_core::content::plain_line(t);
            (!line.is_empty()).then(|| line.chars().take(T::TAB_TIP_REASON_CHARS).collect())
        };
        let content = self.workspace.content.get(&s.id);
        if matches!(g, Status::Review | Status::Paused)
            && let Some(left) = fibo_core::status::left_behind(&self.workspace, s)
        {
            return Some(left);
        }
        match bucket(g) {
            _ if g == Status::Background => content.and_then(|c| c.background?.describe()),
            // A question reads as one, in the v3 design's words: 「问你：侧栏参考哪两家？」.
            Bucket::Attention if g == Status::Waiting => self
                .attention_reason(s, g)
                .or_else(|| {
                    let text = self.attention_text(s, g)?;
                    Some(super::super::layout::ask_prompt(text.lines().next()?).to_owned())
                })
                .filter(|r| !r.is_empty())
                .map(|r| format!("{}{r}", fibo_core::i18n::say(ASKS))),
            Bucket::Attention => self.attention_reason(s, g).map(|r| t!("停在 {r}", r)),
            Bucket::Running => content.and_then(|c| c.doing()).map(|d| t!("正在{d}", d)),
            _ => content.and_then(|c| first(c.last_reply.lines().find(|l| !l.trim().is_empty())?)),
        }
    }
    pub(in crate::workbench) fn session_row(
        &self,
        (session, g, attention): Listed,
        current: Option<&str>,
        detail: Option<String>,
        end: End,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        if self.sidebar.preset().group == Grouping::Recent {
            return self.flat_row((session, g, attention), current, detail, end, cx);
        }
        if self.sidebar.dense() {
            return self.dense_row((session, g, attention), current, detail, end, cx);
        }
        let open = session.clone();
        use crate::ui::status_colors as seen;
        // Every row and tab shows its CLI's mark, in its own color (v3 mock).
        let marks = true;
        let title = match self
            .layout
            .renaming_session
            .as_ref()
            .filter(|(id, ..)| id == &session.id)
        {
            Some((_, field, _)) => div()
                .flex_1()
                .min_w_0()
                .text_size(px(T::MONO_SIZE))
                .child(field.clone()),
            None => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(T::MONO_SIZE))
                .when(!attention, |d| d.text_color(T::color(seen::seen_title())))
                .child(self.session_title(session)),
        };
        // Rows that all wait for your next step tell apart by how long they have.
        let trailing = label(match end {
            End::Text(text) => text,
            End::Nothing => String::new(),
            End::State if g == Status::Review && !attention => {
                ago(self.session_updated(session), now())
            }
            End::State => g.label().to_owned(),
        })
        .flex_shrink_0()
        .text_size(px(T::CHIP_SIZE))
        .text_color(if attention {
            g.colors().1
        } else {
            T::color(seen::seen_status())
        });
        div()
            .id(gpui::SharedString::from(format!("session-{}", session.id)))
            .min_h(px(T::ROW))
            .px(px(T::S8))
            .flex()
            .flex_col()
            .justify_center()
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .when(current == Some(session.id.as_str()), |d| {
                d.bg(T::color(T::raised()))
            })
            .hover(|s| s.bg(T::color(T::raised())))
            .when(self.peek_home(&session.id), |d| {
                d.relative().child(super::super::peek::peek_home_outline())
            })
            .when(self.marking(&session.id), |d| {
                d.relative().child(super::super::complete::spot(
                    &self.layout.spots,
                    session.id.clone(),
                    (T::S8, 0.),
                    |s| &mut s.rows,
                ))
            })
            .child(
                div()
                    .h(px(T::ROW))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .when(marks, |d| {
                        d.child(super::super::board::agent_mark(
                            session.runtime,
                            T::color(T::muted()),
                        ))
                    })
                    .child(title)
                    .child(if attention { dot(g) } else { seen_dot(g) })
                    .child(trailing),
            )
            .when_some(detail, |d, text| {
                d.child(
                    div()
                        .mt(-px(T::S4))
                        .mb(px(T::S4))
                        .when(marks, |d| d.pl(px(T::BOARD_MARK + T::S8)))
                        .truncate()
                        .text_size(px(T::CHIP_SIZE))
                        .text_color(T::color(T::muted()))
                        .child(text),
                )
            })
            // The hover card would cover the right-click menu.
            .when(self.layout.session_menu.is_none(), |d| {
                d.tooltip(super::super::diagnostics::tip_lines(
                    self.session_tip(session),
                ))
            })
            .on_mouse_down(
                gpui::MouseButton::Right,
                self.open_session_menu(session, cx),
            )
            // Straight to its pane; an exited one resumes in a new pane.
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.click_count() == 2 {
                        this.rename_session(&open.id, window, cx);
                        return;
                    }
                    this.peek_or_open(&open, window, cx)
                }),
            )
            .into_any_element()
    }
}
