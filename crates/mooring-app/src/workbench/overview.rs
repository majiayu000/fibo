//! 鸟瞰: every open session as grouped cards.
use super::*;

mod groups;
mod session_card;
mod todo_section;
mod view;

/// The overview's filters and folds, the 「你在这里」 card and 逐个清理's queue.
pub(super) struct OverviewState {
    pub(super) since: u64,
    pub(super) filter_space: Option<String>,
    pub(super) changed_only: bool,
    /// Only this space's sessions whose terminal was closed, from the sidebar's 「已关闭」.
    pub(super) closed_only: bool,
    pub(super) here: Option<String>,
    /// The session whose 「你在这里」 card shows its secondary actions.
    pub(super) here_more: Option<String>,
    pub(super) show_stale: bool,
    pub(super) show_closed: bool,
    pub(super) cleanup: Vec<String>,
    pub(super) cleanup_index: usize,
    /// What each queued session became while this round of 逐个清理 ran, by session id.
    pub(super) cleanup_results: HashMap<String, Outcome>,
    pub(super) show_running: bool,
    pub(super) show_review: bool,
    /// The overview's scroll, so a group's pill can bring that group into view.
    pub(super) scroll: gpui::ScrollHandle,
    pub(super) columns: std::cell::RefCell<HashMap<super::board::Column, super::board::ColumnList>>,
    pub(super) show_hidden: bool,
    /// The board shows only this CLI's sessions and todos.
    pub(super) agent: Option<mooring_core::model::Agent>,
    /// The session whose detail drawer is open on the board.
    pub(super) drawer: Option<String>,
    /// The column a dragged card hangs over, and the column it came from.
    pub(super) drop_over: Option<(super::board::Column, super::board::Column)>,
    /// The card picked up off the board, or flying after its drop.
    pub(super) lift: Option<super::board_drawer::Lift>,
    pub(super) lift_seq: u64,
    /// A column shaking after it refused a card.
    pub(super) shake: Option<(super::board::Column, u64)>,
    /// The last width a column was drawn at, for a lifted card's first frame.
    pub(super) column_width: Option<gpui::Pixels>,
    pub(super) spots: super::board_drawer::BoardSpots,
    pub(super) toast: Option<super::board_drawer::Toast>,
    pub(super) toast_seq: u64,
    pub(super) card_menu: Option<String>,
    /// Opened by coming back after a while, not by the user: titled 「欢迎回来」.
    pub(super) welcome: bool,
    /// Holds keyboard focus while the overview shows, so Esc returns to the panes.
    pub(super) focus: gpui::FocusHandle,
    /// Where the top bar's toggle was drawn, for native click evidence.
    #[cfg(feature = "visual-test")]
    pub(super) toggle_at: Option<gpui::Bounds<gpui::Pixels>>,
}
impl Workbench {
    /// Swaps the panes for the overview in the same window: top bar, sidebar and tabs stay.
    pub(super) fn show_overview(&mut self, cx: &mut Context<Self>) {
        self.overview.closed_only = false;
        self.overview.welcome = false;
        self.overview.cleanup.clear();
        self.nav.overview = true;
        self.nav.task_detail = None;
        self.nav.pending_focus = true;
        self.sync_panes(cx);
        self.request(Request::Workspace, cx);
    }
    /// 逐个清理 over these sessions, in order.
    pub(super) fn start_cleanup(&mut self, queue: Vec<String>, cx: &mut Context<Self>) {
        self.overview.cleanup = queue;
        self.overview.cleanup_index = 0;
        self.overview.cleanup_results.clear();
        self.overview.here = None;
        cx.notify();
    }
    /// Notes where the toggle is drawn so native evidence can press the real button.
    #[cfg(feature = "visual-test")]
    pub(super) fn record_toggle(
        &self,
        button: gpui::Stateful<gpui::Div>,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let entity = cx.entity();
        button.relative().child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, _, _, cx| {
                    entity.update(cx, |this, _| this.overview.toggle_at = Some(bounds));
                },
            )
            .absolute()
            .size_full(),
        )
    }
    #[cfg(not(feature = "visual-test"))]
    pub(super) fn record_toggle(
        &self,
        button: gpui::Stateful<gpui::Div>,
        _: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        button
    }
    /// Back to the tab that was in view, its focused pane taking the keyboard again.
    pub(super) fn leave_overview(&mut self, cx: &mut Context<Self>) {
        self.overview.closed_only = false;
        self.overview.welcome = false;
        self.overview.cleanup.clear();
        self.overview.card_menu = None;
        self.nav.overview = false;
        self.nav.pending_focus = true;
        self.sync_panes(cx);
        cx.notify();
    }
}

/// The one solid control of a view.
pub(super) fn solid(b: gpui::Stateful<gpui::Div>) -> gpui::Stateful<gpui::Div> {
    b.bg(T::color(T::accent()))
        .text_color(T::color(T::on_accent()))
        .font_weight(gpui::FontWeight::SEMIBOLD)
}
/// An ordinary button: a quiet fill, no outline.
pub(super) fn quiet(b: gpui::Stateful<gpui::Div>) -> gpui::Stateful<gpui::Div> {
    b.bg(T::color(T::button()))
        .text_color(T::color(T::primary()))
}
/// A section's title line: status dot, name, count, a note and actions at the right.
fn section_head(
    group: Option<Status>,
    title: &str,
    count: usize,
    note: Option<&str>,
    right: Vec<gpui::AnyElement>,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(T::S8))
        .when_some(group, |d, g| d.child(dot(g)))
        .child(
            div()
                .text_size(px(T::CARD_TITLE))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(group.map_or(T::color(T::primary()), |g| g.colors().1))
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_color(T::color(T::muted()))
                .child(count.to_string()),
        )
        .when_some(note, |d, note| {
            d.child(label(note.to_owned()).text_color(T::color(T::muted())))
        })
        .child(div().flex_1())
        .child(div().flex().gap(px(T::S6)).children(right))
}
