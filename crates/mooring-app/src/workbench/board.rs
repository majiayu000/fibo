//! 鸟瞰 as a board (Keepline repo, specs/overview-kanban/kanban.html): one column per
//! thing you might do next, left to right in the order work moves; what waits on you or
//! failed has its own column, 要你处理 (decided 11-02: five columns, the inbox variant is
//! not done). Each card carries one colored mark, its space's gradient; everything else
//! stays neutral so the status colors read first.
use super::*;
use crate::ui::icons::agent_icon;
use gpui::{
    Animation, AnimationExt, Hsla, Transformation, linear_color_stop, linear_gradient, relative,
    svg,
};
use mooring_core::model::Agent;
use std::time::Duration;

mod card;
mod column;
mod header;
mod pill;
pub(super) mod remote;
use card::{card_frame, card_title};
use pill::{name_pill, pill};

pub(super) use mooring_core::status_group::{Column, column as column_of};

/// Each column keeps GPUI's measured item heights and its own scroll anchor.
pub(super) struct ColumnList {
    pub(super) state: gpui::ListState,
    ids: Vec<String>,
    revision: u64,
}
impl ColumnList {
    fn sync(&mut self, ids: &[String], revision: u64) {
        let prefix = self.ids.iter().zip(ids).take_while(|(a, b)| a == b).count();
        let suffix = self.ids[prefix..]
            .iter()
            .rev()
            .zip(ids[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        if prefix + suffix != self.ids.len() || self.ids.len() != ids.len() {
            self.state
                .splice(prefix..self.ids.len() - suffix, ids.len() - prefix - suffix);
            self.ids = ids.to_vec();
        }
        if self.revision != revision {
            self.state.remeasure_items(0..self.state.item_count());
            self.revision = revision;
        }
    }
}
/// Four-color gradients for a space's avatar, picked by its id so a space keeps its look.
/// No pure red or orange as a base: those belong to 出错 and 等你回答.
pub(super) fn mesh_index(seed: &str) -> usize {
    seed.bytes().fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    }) as usize
        % T::BOARD_MESHES.len()
}
/// A round avatar in the space's gradient: three fading washes over a base, each layer
/// rounded itself since content masks are square.
pub(super) fn mesh(index: usize, size: f32) -> gpui::Div {
    let [a, b, c, base] = T::BOARD_MESHES[index % T::BOARD_MESHES.len()];
    let wash = |hex: u32, angle: f32, reach: f32| {
        let solid = T::color(hex);
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .rounded_full()
            .bg(linear_gradient(
                angle,
                linear_color_stop(solid, 0.),
                linear_color_stop(solid.opacity(0.), reach),
            ))
    };
    div()
        .relative()
        .size(px(size))
        .flex_shrink_0()
        .rounded_full()
        .bg(T::color(base))
        .child(wash(a, 135., 0.62))
        .child(wash(b, 225., 0.58))
        .child(wash(c, 0., 0.6))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded_full()
                .border(px(T::BORDER_WIDTH))
                .border_color(gpui::white().opacity(0.12)),
        )
}

/// A branch's kind from its prefix, colored like a label: feat, fix, perf, docs...
fn branch_hue(branch: &str) -> Option<u32> {
    Some(match branch.split('/').next()? {
        "feat" | "feature" => T::BRANCH_FEAT,
        "fix" | "hotfix" | "bugfix" => T::BRANCH_FIX,
        "perf" => T::BRANCH_PERF,
        "docs" | "copy" | "style" => T::BRANCH_DOCS,
        "ops" | "ci" | "chore" | "build" => T::BRANCH_OPS,
        "refactor" | "test" | "db" => T::BRANCH_REFACTOR,
        _ => return None,
    })
}

const PILL: f32 = T::BOARD_PILL;
const COLUMN_MIN: f32 = T::BOARD_COLUMN_MIN;
const CARD_GAP: f32 = T::BOARD_CARD_SPACING;
const AVATAR: f32 = T::BOARD_AVATAR;
const MARK: f32 = T::BOARD_MARK;
const HEAD_ICON: f32 = T::BOARD_HEAD_ICON;

/// The CLI's mark: in its own color when it has one (`Agent::brand_color`), else in
/// `color`; its initial when it has no mark.
pub(super) fn agent_mark(agent: Agent, color: Hsla) -> gpui::AnyElement {
    let color = agent.brand_color().map(T::color).unwrap_or(color);
    match agent_icon(agent) {
        Some(path) => svg()
            .path(path)
            .size(px(MARK))
            .flex_shrink_0()
            .text_color(color)
            .into_any_element(),
        None => div()
            .text_size(px(T::CHIP_SIZE))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(color)
            .child(agent.name().chars().next().unwrap_or('?').to_string())
            .into_any_element(),
    }
}

pub(super) fn column_icon(column: Column) -> gpui::AnyElement {
    animated_column_icon(column, false)
}
fn animated_column_icon(column: Column, animate: bool) -> gpui::AnyElement {
    let icon =
        |path: &'static str, color: Hsla| svg().path(path).size(px(HEAD_ICON)).text_color(color);
    let frame = || div().relative().size(px(HEAD_ICON)).flex_shrink_0();
    let overlay = |e: gpui::AnyElement| div().absolute().top_0().left_0().child(e);
    if !animate && column == Column::Running {
        return frame()
            .child(icon(
                "icons/ring.svg",
                Status::Running.colors().0.opacity(0.25),
            ))
            .child(overlay(
                icon("icons/arc.svg", Status::Running.colors().0).into_any_element(),
            ))
            .into_any_element();
    }
    if !animate && column == Column::Needs {
        return icon("icons/target.svg", Status::Waiting.colors().0).into_any_element();
    }
    match column {
        Column::Todo => icon("icons/dashed.svg", T::color(T::muted())).into_any_element(),
        Column::Stale => icon("icons/dashed.svg", Status::Stale.colors().0).into_any_element(),
        Column::Review => icon("icons/half.svg", Status::Paused.colors().0).into_any_element(),
        Column::Running => {
            let hue = Status::Running.colors().0;
            frame()
                .child(icon("icons/ring.svg", hue.opacity(0.25)))
                .child(overlay(
                    icon("icons/arc.svg", hue)
                        .with_transformation(Transformation::rotate(gpui::percentage(
                            super::pace::phase(SPIN),
                        )))
                        .into_any_element(),
                ))
                .into_any_element()
        }
        Column::Needs => {
            // A halo that grows from the target and fades, once per HALO.
            let hue = Status::Waiting.colors().0;
            let t = super::pace::phase(HALO);
            let k = 0.7 + 0.8 * t;
            frame()
                .child(overlay(
                    icon("icons/ring.svg", hue.opacity(0.7 * (1. - t)))
                        .with_transformation(Transformation::scale(gpui::size(k, k)))
                        .into_any_element(),
                ))
                .child(icon("icons/target.svg", hue))
                .into_any_element()
        }
        Column::Done => frame()
            .rounded_full()
            .bg(Status::Closed.colors().0)
            .flex()
            .items_center()
            .justify_center()
            .child(icon("icons/check.svg", T::color(T::terminal_bg())))
            .into_any_element(),
    }
}

impl Workbench {
    /// Spaces take the gradients in their sidebar order, so the first six never share one;
    /// a session outside every space hashes its key.
    pub(super) fn space_mesh(&self, space: &str) -> usize {
        self.workspace
            .spaces
            .iter()
            .position(|s| s.id == space)
            .unwrap_or_else(|| mesh_index(space))
    }
    pub(super) fn board_sessions(&self, now: u64) -> Vec<(Column, &AgentSession)> {
        self.workspace
            .agent_sessions
            .iter()
            .filter(|s| {
                self.session_hidden(s).is_none()
                    && self.matches_filter(s)
                    && self.overview.agent.is_none_or(|a| a == s.runtime)
            })
            .filter_map(|s| column_of(&self.workspace, s, now).map(|c| (c, s)))
            .collect()
    }
    fn board_todos(&self) -> Vec<&Task> {
        (mooring_core::status::open_todos(&self.workspace, now()).into_iter())
            .filter(|t| {
                (self.overview.filter_space.as_ref()).is_none_or(|id| id == &t.space)
                    && self.overview.agent.is_none_or(|a| a == t.agent)
                    && !self.overview.changed_only
            })
            .collect()
    }
    /// One column's sessions in board order.
    pub(super) fn column_sessions(&self, c: Column, at: u64) -> Vec<&AgentSession> {
        let mut list: Vec<_> = self
            .board_sessions(at)
            .into_iter()
            .filter(|(col, _)| *col == c)
            .map(|(_, s)| s)
            .collect();
        self.sort_column(c, &mut list);
        list
    }
    fn sort_column(&self, c: Column, list: &mut [&AgentSession]) {
        // An Agent holding a live terminal for you comes first, then what has waited
        // longest; elsewhere the latest comes first.
        if c == Column::Needs {
            list.sort_by_key(|s| {
                (
                    !needs_attention(&self.workspace, s),
                    self.session_updated(s),
                )
            });
        } else {
            list.sort_by_key(|s| std::cmp::Reverse(self.session_updated(s)));
        }
    }
    pub(super) fn board_view(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let at = now();
        let sessions = self.board_sessions(at);
        let todos = self.board_todos();
        let remote = self.remote_cards();
        let columns_of = sessions
            .iter()
            .map(|(c, _)| *c)
            .chain(remote.iter().map(|r| r.column));
        let open = columns_of.clone().filter(|c| *c != Column::Done).count();
        let stale = columns_of.filter(|c| *c == Column::Stale).count();
        let empty = sessions.is_empty() && remote.is_empty() && todos.is_empty();
        let mut grouped: HashMap<Column, Vec<&AgentSession>> = HashMap::new();
        for (column, session) in sessions {
            grouped.entry(column).or_default().push(session);
        }
        let todo_ids: Vec<_> = todos.iter().map(|t| t.id.clone()).collect();
        let mut columns = vec![
            self.column(
                Column::Todo,
                todo_ids.len(),
                self.card_list(Column::Todo, todo_ids, cx),
                window.is_window_active(),
                cx,
            )
            .into_any_element(),
        ];
        let mut shown = vec![Column::Running, Column::Needs, Column::Review, Column::Done];
        if self.overview.show_stale {
            shown.push(Column::Stale);
        }
        for c in shown {
            let mut list = grouped.remove(&c).unwrap_or_default();
            self.sort_column(c, &mut list);
            if c == Column::Running && !list.is_empty() {
                self.tick_turn(cx);
            }
            let ids: Vec<_> = self.mixed_ids(c, &list, &remote);
            let column = self.column(
                c,
                ids.len(),
                self.card_list(c, ids, cx),
                window.is_window_active(),
                cx,
            );
            columns.push(self.shaking(c, column));
        }
        div()
            .id("overview-scroll")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .font_features(super::diagnostics::tabular())
            .children(self.board_top(open, stale, empty, cx))
            .when_some(self.error.clone(), |d, error| {
                d.child(
                    div()
                        .px(px(T::OVERVIEW_X))
                        .pb(px(T::S8))
                        .text_color(Tone::Danger.ink())
                        .child(error),
                )
            })
            .child({
                let spot = self.overview.spots.board.clone();
                gpui::canvas(move |bounds, _, _| spot.set(bounds), |_, _, _, _| ())
                    .absolute()
                    .size_full()
            })
            .child(
                div()
                    .id("board")
                    .on_drop(
                        cx.listener(|this, _: &super::board_drawer::DraggedCard, _, cx| {
                            this.drop_off_board(cx)
                        }),
                    )
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(T::S14))
                    .px(px(T::OVERVIEW_X))
                    .pb(px(T::S16))
                    .overflow_x_scroll()
                    .children(columns),
            )
            .children(self.lift_view(cx))
            .children(self.drawer_view(cx))
            .children(self.toast_view(cx))
    }
}

/// One turn of the running column's arc, and one pass of a working card's comet.
const SPIN: Duration = Duration::from_millis(1600);
const SWEEP: Duration = Duration::from_millis(2400);
/// One pulse of 要你处理's halo.
const HALO: Duration = Duration::from_millis(2000);
/// The board's motion advances 20 times a second, not on every display frame: a frame
/// redraws the whole board, and at 120 Hz the arc and comets alone held a quarter core.
/// At 20 steps the arc turns 4.5° and a comet moves about 7 px a step.
const MOTION_STEP: Duration = Duration::from_millis(50);
static MOTION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
mod tests;

/// The terminal style draws the board on the window's gray, the standard one on white.
fn app_style(cx: &gpui::App) -> bool {
    crate::ui::style::current(cx) == crate::ui::style::Style::Terminal
}
fn board_surface(terminal: bool) -> fn() -> u32 {
    if terminal { T::app } else { T::pane }
}
