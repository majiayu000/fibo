//! 未关联的终端: Agents and programs in a tab here that no conversation is tied to, whether
//! Mooring cannot read it (Codex on the shared daemon) or none is kept (a Claude Code that
//! writes no record). They are not history, so they get a heading of their own, and their
//! rows take the columns of the rows around them.
use super::*;
use crate::ui::theme::Flat as F;
use crate::workbench::sidebar_flat::{flat_row_box, flat_selected_edge, flat_state};

const TITLE: &str = tk!("未关联的终端");

/// One such terminal: its short note, the reason a hover shows, and its dot's state.
pub(in crate::workbench) struct Unlinked {
    pub terminal: String,
    pub note: String,
    pub why: String,
    pub group: Status,
}

/// The heading, or none: no such terminal, no empty group.
fn heading_text(count: usize) -> Option<(&'static str, usize)> {
    (count > 0).then_some((mooring_core::i18n::say(TITLE), count))
}

/// Whether this row is the selected one. A peek is what is in front: its session's row is
/// selected until it closes, and there is one selection at a time.
fn row_selected(focused: Option<&str>, terminal: &str, peeking: bool) -> bool {
    !peeking && focused == Some(terminal)
}

/// The list a row sits in, whose rows it lines up with.
#[derive(Clone, Copy)]
enum Look {
    /// 会话优先's `flat_row`: the state at the head.
    Flat,
    /// `dense_row`: the Agent's mark at the head.
    Dense,
    /// `session_row`: the same head, larger text.
    Standard,
}

fn row_box(id: gpui::SharedString, look: Look, focused: bool) -> gpui::Stateful<gpui::Div> {
    match look {
        Look::Flat => flat_row_box(id, focused, false),
        Look::Dense | Look::Standard => div()
            .id(id)
            .h(px(T::ROW))
            .px(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S8))
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .when(focused, |d| d.bg(T::color(T::raised())))
            .hover(|s| s.bg(T::color(T::raised()))),
    }
}

fn row_mark(look: Look, group: Status) -> gpui::AnyElement {
    match look {
        Look::Flat => flat_state(group, false),
        // Centered where the neighbours draw their Agent's mark.
        Look::Dense | Look::Standard => div()
            .w(px(T::BOARD_MARK))
            .flex_shrink_0()
            .flex()
            .justify_center()
            .child(seen_dot(group))
            .into_any_element(),
    }
}

fn row_text(look: Look, note: String) -> gpui::Div {
    let text = div().flex_1().min_w_0().truncate();
    match look {
        Look::Flat => text
            .text_size(px(F::TITLE))
            .line_height(px(F::TITLE_LINE))
            .text_color(T::color(F::quiet())),
        Look::Dense => text
            .font_family(T::UI_FONT)
            .text_size(px(T::SMALL))
            .text_color(T::color(T::muted())),
        Look::Standard => text
            .text_size(px(T::MONO_SIZE))
            .text_color(T::color(T::muted())),
    }
    .child(note)
}

impl Workbench {
    /// The group under the column's lists, in the chosen preset's look.
    pub(in crate::workbench) fn unlinked_rows(
        &self,
        rows: Vec<Unlinked>,
        recent: bool,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let Some((title, count)) = heading_text(rows.len()) else {
            return vec![];
        };
        let look = match (recent, self.sidebar.dense()) {
            (true, _) => Look::Flat,
            (false, true) => Look::Dense,
            (false, false) => Look::Standard,
        };
        let head = if recent {
            Self::flat_group(title, count)
        } else {
            self.heading(title.to_owned(), count, cx)
        };
        std::iter::once(head.into_any_element())
            .chain(rows.into_iter().map(|row| {
                let Unlinked {
                    terminal,
                    note,
                    why,
                    group,
                } = row;
                let focused =
                    row_selected(self.nav.focused.as_deref(), &terminal, self.peek.is_some());
                let id = gpui::SharedString::from(format!("unread-row-{terminal}"));
                row_box(id, look, focused)
                    .child(row_mark(look, group))
                    .child(row_text(look, note))
                    .when(recent && focused, |d| d.child(flat_selected_edge()))
                    .tooltip(move |_, cx| {
                        cx.new(|_| super::super::render::TextTip(why.clone().into()))
                            .into()
                    })
                    .on_mouse_down(gpui::MouseButton::Right, {
                        let terminal = terminal.clone();
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            this.layout.menu = Some(PaneMenu {
                                terminal: terminal.clone(),
                                position: event.position,
                                page: MenuPage::Main,
                                path: None,
                            });
                            cx.notify();
                        })
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        // As a session row's click does, or the peek stays over the terminal.
                        this.close_peek(cx);
                        this.focus_terminal(&terminal, window, cx);
                    }))
                    .into_any_element()
            }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AbsoluteLength, DefiniteLength, Length, Styled, TextStyleRefinement};

    fn points(length: Option<impl Into<Length>>) -> f32 {
        match length.map(Into::into) {
            Some(Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(p)))) => p.into(),
            other => panic!("not a length in pixels: {other:?}"),
        }
    }
    /// Where a row's dot and its text start, from the row's own box and its dot's width.
    fn columns(row: &mut gpui::Stateful<gpui::Div>, dot: f32) -> (f32, f32) {
        let style = row.style();
        let start = points(style.margin.left) + points(style.padding.left);
        (start, start + dot + points(style.gap.width))
    }

    #[test]
    fn no_such_terminal_means_no_heading() {
        assert_eq!(heading_text(0), None);
        assert_eq!(heading_text(2), Some((TITLE, 2)));
    }

    #[test]
    fn a_peek_takes_the_selection_from_the_row_underneath() {
        assert!(row_selected(Some("t"), "t", false));
        assert!(!row_selected(Some("t"), "t", true));
        assert!(!row_selected(Some("other"), "t", false));
    }

    /// 「两个栏不是一样宽的」: a row's box starts where the search field and the chips do,
    /// on both sides, however its contents are placed.
    #[test]
    fn a_rows_box_lines_up_with_the_controls_above_it() {
        let id = || gpui::SharedString::from("row");
        let mut session = flat_row_box(id(), false, false);
        let mut older = crate::workbench::sidebar_flat::flat_more_box(id());
        for row in [&mut session, &mut older] {
            let style = row.style();
            assert_eq!(points(style.margin.left), T::S12);
            assert_eq!(points(style.margin.right), T::S12);
        }
    }

    /// 「会话 7」 over 3 sessions while the same column says 「未关联的终端 4」.
    #[test]
    fn the_session_counts_leave_these_terminals_out() {
        let view = include_str!("../sidebar/view.rs");
        assert!(view.contains("let listed_count = sessions.len();"));
        assert!(view.contains("self.sessions_header(listed_count, cx)"));
        assert!(view.contains("self.flat_column(sidebar, listed_count, cx)"));
    }

    /// 「好像没对齐」: dot and text in the columns of 会话优先's own rows and 更早's line,
    /// in the size of the session titles beside them.
    #[test]
    fn a_placeholder_row_takes_the_columns_of_the_rows_around_it() {
        let id = || gpui::SharedString::from("row");
        let mut placeholder = row_box(id(), Look::Flat, false);
        let mut session = flat_row_box(id(), false, false);
        let mut older = crate::workbench::sidebar_flat::flat_more_box(id());
        let at = columns(&mut placeholder, F::STATE);
        assert_eq!(at, columns(&mut session, F::STATE));
        assert_eq!(at, columns(&mut older, F::STATE));
        assert_eq!(at, (F::EDGE, F::EDGE + F::STATE + F::ROW_GAP));
        let TextStyleRefinement { font_size, .. } =
            row_text(Look::Flat, String::new()).style().text.clone();
        assert_eq!(font_size, Some(px(F::TITLE).into()));
    }

    /// 「这里为啥有个圆圈」: the 更早 line holds a blank slot, not a session's state dot.
    #[test]
    fn the_older_line_draws_no_state_dot() {
        let flat = include_str!("../sidebar_flat.rs");
        let start = flat.find("pub(super) fn flat_more(").unwrap();
        let end = start + flat[start..].find("into_any_element()").unwrap();
        assert!(!flat[start..end].contains("flat_state("));
    }
}
