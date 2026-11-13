//! The left column: spaces, the current space's sessions and its (folded) todos.
use super::*;
mod space_edit;
mod space_menu;
mod todos;
mod view;

pub(super) use fibo_core::status_group::rank as sidebar_rank;
/// A closed session the sidebar still counts: no outcome yet and not gone quiet past
/// the stale days; older ones wait in the overview's 很久没动.
pub(super) fn closed_recently(w: &Workspace, s: &AgentSession) -> bool {
    s.outcome.needs_review(now()) && status_of(w, s) != Status::Stale
}

/// A space's row figures: its open sessions, those waiting on you and those failed, and
/// the state its dot shows.
pub(super) struct SpaceCounts {
    count: usize,
    pub(super) waiting: usize,
    pub(super) errors: usize,
    group: Status,
}

impl Workbench {
    /// 鸟瞰 narrowed to the closed sessions the column counted, ready to resume.
    pub(super) fn show_closed(&mut self, cx: &mut Context<Self>) {
        self.show_overview(cx);
        self.overview.closed_only = true;
        self.overview.changed_only = false;
        self.overview.filter_space = if self.sidebar.preset().all_spaces() {
            self.space_filter().map(str::to_owned)
        } else {
            Some(self.nav.space.clone())
        };
    }
    /// The space 会话优先 is narrowed to, while that space still exists.
    pub(super) fn space_filter(&self) -> Option<&str> {
        self.sidebar.space_filter.as_deref().filter(|f| {
            self.workspace.spaces.iter().any(|s| s.id == *f) && !self.workspace.space_quiet(f)
        })
    }
    /// Narrows the list to a space and shows it, or lists every space again.
    pub(super) fn toggle_space_filter(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.space_filter() == Some(id) {
            self.sidebar.space_filter = None;
            cx.notify();
            return;
        }
        self.sidebar.space_filter = Some(id.to_owned());
        self.set_machine_scope(None);
        self.pick_space(id, cx);
    }
    /// A space's two colors on a round dot, as the 鸟瞰 board shows spaces.
    pub(super) fn space_dot(&self, id: &str, size: f32) -> gpui::Div {
        let [from, to] = T::space_tile(self.space_mesh(id));
        div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_full()
            .bg(gpui::linear_gradient(
                135.,
                gpui::linear_color_stop(T::color(from), 0.),
                gpui::linear_color_stop(T::color(to), 1.),
            ))
    }
    /// A session row's space: its dot and name, quiet, at the row's end.
    pub(super) fn space_tag(&self, id: &str, name: &str) -> gpui::Div {
        use crate::ui::theme::Flat as F;
        div()
            .flex_shrink_0()
            .max_w(px(F::TAG_MAX))
            .flex()
            .items_center()
            .gap(px(F::CHIP_GAP))
            .child(self.space_dot(id, F::TAG_DOT))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(T::UI_FONT)
                    .text_size(px(F::TAG))
                    .line_height(px(F::LINE_SMALL))
                    .text_color(T::color(F::third()))
                    .child(name.to_owned()),
            )
    }

    /// Whether a space is the one shown; over the overview the spaces are its filter.
    fn space_active(&self, id: &str) -> bool {
        if self.nav.overview {
            self.overview.filter_space.as_deref() == Some(id)
        } else {
            self.nav.space == id
        }
    }
    /// A space's open sessions, how many wait on you or failed, and the state its dot shows.
    pub(super) fn space_counts(&self, id: &str) -> SpaceCounts {
        let sessions: Vec<_> = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| {
                self.session_hidden(s).is_none()
                    && self.session_open(s)
                    && self.workspace.session_space(s) == Some(id)
            })
            .collect();
        let needs = |g: Status| {
            sessions
                .iter()
                .filter(|s| {
                    s.managed()
                        && s.outcome.needs_review(now())
                        && status_of(&self.workspace, s) == g
                })
                .count()
        };
        let (waiting, errors) = (needs(Status::Waiting), needs(Status::Error));
        let group = if errors > 0 {
            Status::Error
        } else if waiting > 0 {
            Status::Waiting
        } else {
            sessions
                .iter()
                .filter(|s| s.outcome.needs_review(now()))
                .map(|s| status_of(&self.workspace, s))
                .find(|g| !matches!(g, Status::Waiting | Status::Error))
                .unwrap_or(Status::Stale)
        };
        SpaceCounts {
            count: sessions.len(),
            waiting,
            errors,
            group,
        }
    }
    /// Shows a space: its first tab, or over the overview, its sessions only.
    pub(super) fn pick_space(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.nav.overview {
            self.overview.filter_space = Some(id.to_owned());
        }
        self.nav.space = id.to_owned();
        self.nav.tab = self
            .workspace
            .tabs
            .iter()
            .find(|t| t.space == id)
            .map(|t| t.id.clone());
        self.sync_panes(cx);
        cx.notify();
    }
}
