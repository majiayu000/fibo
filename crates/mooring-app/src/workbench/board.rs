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

mod composer;
pub(super) use composer::{BoardQuick, FreshTodo};
mod card;
mod column;
mod pill;
pub(super) mod remote;
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
            // A list at its top stays there: a splice moves the top along with its first
            // item, which would leave a card that just arrived above the view.
            let top = self.state.logical_scroll_top();
            let at_top = top.item_ix == 0 && top.offset_in_item == px(0.);
            self.state
                .splice(prefix..self.ids.len() - suffix, ids.len() - prefix - suffix);
            if at_top {
                self.state.scroll_to(gpui::ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.),
                });
            }
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

/// How long it has waited, as four rising bars that fill and warm up with the wait.
fn wait_bars(waited: u64, hue: Hsla) -> gpui::Div {
    let filled = match waited {
        0..300 => 1,
        300..1800 => 2,
        1800..3600 => 3,
        _ => 4,
    };
    let lit = if filled >= 3 {
        hue
    } else {
        T::color(T::secondary())
    };
    div()
        .size(px(PILL))
        .flex_shrink_0()
        .flex()
        .items_end()
        .justify_center()
        .gap(px(T::BOARD_BAR_GAP))
        .pb(px(T::BOARD_BAR_INSET))
        .rounded(px(T::BOARD_BAR_FRAME_RADIUS))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::card_border()))
        .children((0..4).map(|i| {
            div()
                .w(px(T::BOARD_BAR_WIDTH))
                .h(px(T::BOARD_BAR_BASE + T::BOARD_BAR_STEP * i as f32))
                .rounded(px(T::BOARD_BAR_RADIUS))
                .bg(if i < filled {
                    lit
                } else {
                    T::color(T::border())
                })
        }))
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
        // A todo belongs to no Agent, so filtering by one leaves it in place.
        (mooring_core::status::unfinished_todos(&self.workspace, now()).into_iter())
            .filter(|t| {
                (self.overview.filter_space.as_ref()).is_none_or(|id| id == &t.space)
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
        let empty = sessions.is_empty()
            && remote.is_empty()
            && todos.is_empty()
            && self.workspace.quiet_spaces.is_empty();
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
                false,
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
            let mut ids: Vec<_> = self.mixed_ids(c, &list, &remote);
            if c == Column::Done {
                let mut done: Vec<&Task> = (self.workspace.tasks.iter().rev())
                    .filter(|t| {
                        self.workspace.task_outcomes.get(&t.id) == Some(&Outcome::Completed)
                            && (self.overview.filter_space.as_ref()).is_none_or(|id| id == &t.space)
                    })
                    .collect();
                done.sort_by_key(|t| {
                    let log = self.workspace.task_logs.get(&t.id);
                    std::cmp::Reverse(log.and_then(|l| l.closed).unwrap_or(0))
                });
                ids = (done.into_iter().map(|t| format!("todo:{}", t.id)))
                    .chain(ids)
                    .collect();
            }
            // Only this column's own sessions are read, on frames that already draw them.
            let reviewable = c == Column::Needs
                && (list.iter()).any(|s| review_queue::reviewable(status_of(&self.workspace, s)));
            let column = self.column(
                c,
                ids.len(),
                self.card_list(c, ids, cx),
                reviewable,
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
    fn board_header(
        &self,
        open: usize,
        stale: usize,
        empty: bool,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let mut agents: Vec<(Agent, usize)> = vec![];
        for s in self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| self.session_hidden(s).is_none() && s.outcome.needs_review(now()))
        {
            match agents.iter_mut().find(|(a, _)| *a == s.runtime) {
                Some((_, n)) => *n += 1,
                None => agents.push((s.runtime, 1)),
            }
        }
        agents.sort_by_key(|(a, _)| a.name());
        let chips = agents.into_iter().map(|(agent, n)| {
            let on = self.overview.agent == Some(agent);
            let dim = self.overview.agent.is_some() && !on;
            div()
                .id(gpui::SharedString::from(format!(
                    "agent-filter-{}",
                    agent.key()
                )))
                .role(gpui::accesskit::Role::Button)
                .aria_label(gpui::SharedString::from(format!("只看 {}", agent.name())))
                .h(px(T::SEGMENT_HEIGHT))
                .pl(px(T::S6))
                .pr(px(T::S8))
                .flex()
                .items_center()
                .gap(px(T::S6))
                .rounded(px(T::SEGMENT_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(if on { T::control() } else { T::card_border() }))
                .when(on, |d| d.bg(T::color(T::raised())))
                .when(dim, |d| d.opacity(0.5))
                .text_size(px(T::SMALL))
                .text_color(T::color(if on { T::primary() } else { T::body() }))
                .cursor_pointer()
                .hover(|s| s.border_color(T::color(T::control())))
                .child(agent_mark(agent, T::color(T::secondary())))
                .child(agent.name())
                .child(div().text_color(T::color(T::muted())).child(n.to_string()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.overview.agent = if this.overview.agent == Some(agent) {
                        None
                    } else {
                        Some(agent)
                    };
                    cx.notify();
                }))
        });
        let link = |id: &'static str, text: String| {
            button(id, text)
                .h(px(T::SEGMENT_HEIGHT))
                .px(px(T::S8))
                .text_color(T::color(T::secondary()))
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(T::S10))
            .px(px(T::OVERVIEW_X))
            .pt(px(T::S20))
            .pb(px(T::S14))
            .child(
                div()
                    .text_size(px(T::TITLE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(T::primary()))
                    .child(if self.overview.welcome {
                        "欢迎回来"
                    } else {
                        "鸟瞰全部"
                    }),
            )
            .child(
                // An empty board says what lands in these columns, not just 「0 个」.
                label(if empty {
                    "还没有会话：在终端里运行 claude 或 codex，它们会按进度出现在下面几列 · Esc 回到工作区".to_owned()
                } else {
                    format!("{open} 个没收尾 · Esc 回到工作区")
                })
                .text_color(T::color(T::muted())),
            )
            .child(div().w(px(T::S8)))
            .children(chips)
            .child(div().flex_1())
            .when(stale > 0 || self.overview.show_stale, |d| {
                d.child(
                    link(
                        "board-stale",
                        if self.overview.show_stale {
                            format!("收起很久没动 {stale}")
                        } else {
                            format!("很久没动 {stale}")
                        },
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.overview.show_stale = !this.overview.show_stale;
                        cx.notify();
                    })),
                )
            })
            .child(
                link("board-closed", "稍后再看与已结束".into()).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.overview.show_closed = true;
                        cx.notify();
                    },
                )),
            )
            .child(self.navigation_filters(cx))
    }
}

fn card_frame(id: gpui::SharedString) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::accesskit::Role::Button)
        .relative()
        .flex_shrink_0()
        .px(px(T::S12))
        .py(px(T::S10))
        .rounded(px(T::CARD_RADIUS))
        .bg(T::color(T::popover()))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::card_border()))
        .flex()
        .flex_col()
        .gap(px(T::BOARD_CARD_GAP))
        .cursor_pointer()
        .hover(|s| s.border_color(T::color(T::control())))
}
fn card_title(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .text_size(px(T::CARD_TITLE))
        .line_height(relative(1.4))
        .text_color(T::color(T::primary()))
        .line_clamp(2)
        .text_ellipsis()
        .child(text.into())
}
/// A card's one line of what it waits on.
fn card_quote(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .text_size(px(T::SMALL))
        .text_color(T::color(T::secondary()))
        .line_clamp(1)
        .text_ellipsis()
        .child(text.into())
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
/// A comet of the running hue that runs along a working card's bottom edge.
fn sweep() -> gpui::Div {
    let hue = Status::Running.colors().0;
    div()
        .absolute()
        .bottom_0()
        .left(px(T::S10))
        .right(px(T::S10))
        .h(px(T::BORDER_WIDTH))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(0.4))
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(hue.opacity(0.), 0.),
                    linear_color_stop(hue, 1.),
                ))
                .left(relative(-0.4 + 1.4 * super::pace::phase(SWEEP))),
        )
}

#[cfg(test)]
mod tests;

/// The terminal style draws the board on the window's gray, the standard one on white.
fn app_style(cx: &gpui::App) -> bool {
    crate::ui::style::current(cx) == crate::ui::style::Style::Terminal
}
fn board_surface(terminal: bool) -> fn() -> u32 {
    if terminal { T::app } else { T::pane }
}
