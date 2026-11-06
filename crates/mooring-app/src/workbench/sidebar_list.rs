//! The sessions part of the left column, arranged by the chosen preset: 会话优先, 目录
//! or a plugin's (`mooring_core::sidebar`). Every preset reads the same `session_status`,
//! so what needs you is the same row in all of them.
use super::*;
use mooring_core::sidebar::{self as presets, Bucket, Detail, Grouping, Preset};

mod dense_row;
mod prefs;
mod session_list;
mod session_menu;
mod session_row;
mod unlinked;

pub(super) use prefs::SidebarPrefs;
pub(super) use unlinked::Unlinked;

/// How a row's second line opens when the session asks you something.
pub(super) const ASKS: &str = "问你：";

/// A session the column lists: its shown state and whether it still asks for a look.
pub(super) type Listed<'a> = (&'a AgentSession, Status, bool);
/// What a session row ends with: its state (or age, once seen), a text, or nothing.
pub(super) enum End {
    State,
    Text(String),
    Nothing,
}

/// 会话优先's sections: 要你处理 (asking, failed, or a finished turn to check), 在跑,
/// 停着 (stopped short, put off or just opened), and what has gone quiet, each newest
/// first. `when` is the turn's start for a running row, so two running rows never trade
/// places while they print, and its last update otherwise.
pub(super) struct RecentOrder<R> {
    pub need: Vec<R>,
    pub running: Vec<R>,
    pub stopped: Vec<R>,
    pub quiet: Vec<R>,
}
pub(super) fn recent_order<R: Copy>(rows: &[(R, Bucket, u64)]) -> RecentOrder<R> {
    let pick = |keep: &dyn Fn(Bucket) -> bool| {
        let mut picked: Vec<(R, u64)> = rows
            .iter()
            .filter(|(_, b, _)| keep(*b))
            .map(|(r, _, when)| (*r, *when))
            .collect();
        picked.sort_by_key(|(_, when)| std::cmp::Reverse(*when));
        picked.into_iter().map(|(r, _)| r).collect::<Vec<_>>()
    };
    RecentOrder {
        // What asks you or failed before the finished turns waiting for your check.
        need: [
            pick(&|b| b == Bucket::Attention),
            pick(&|b| b == Bucket::Done),
        ]
        .concat(),
        running: pick(&|b| b == Bucket::Running),
        stopped: pick(&|b| matches!(b, Bucket::Stopped | Bucket::Fresh)),
        quiet: pick(&|b| b == Bucket::Stale),
    }
}

/// Rows no longer moving, newest first, split at 24 hours: 今天 and 更早.
pub(super) fn split_today<R>(mut rows: Vec<(R, u64)>, now: u64) -> (Vec<R>, Vec<R>) {
    rows.sort_by_key(|(_, when)| std::cmp::Reverse(*when));
    let (today, older): (Vec<_>, Vec<_>) = rows
        .into_iter()
        .partition(|(_, when)| now.saturating_sub(*when) < 86_400);
    (
        today.into_iter().map(|(r, _)| r).collect(),
        older.into_iter().map(|(r, _)| r).collect(),
    )
}

impl Workbench {
    /// The group a listed row sits in: its state's bucket, except that 要你处理 holds only
    /// what `status::needs_you` counts, so the sidebar, the chip and the board agree. A
    /// row whose state would ask but is not the terminal's conversation any more (a
    /// resume not taken yet) waits with the stopped ones, its dot still saying why.
    pub(super) fn row_bucket(&self, l: &Listed) -> Bucket {
        match bucket(l.1) {
            Bucket::Attention if !self.workspace.needs_you(l.0, crate::model::time::now()) => {
                Bucket::Stopped
            }
            b => b,
        }
    }
}
pub(super) use mooring_core::status_group::bucket;
/// The state a bucket's summary shows its dot in.
fn bucket_group(b: Bucket) -> Status {
    match b {
        Bucket::Attention => Status::Waiting,
        Bucket::Running => Status::Running,
        Bucket::Done => Status::Review,
        Bucket::Stopped => Status::Paused,
        Bucket::Fresh => Status::Fresh,
        Bucket::Stale => Status::Stale,
    }
}

/// What a session row's right-click menu does.
#[derive(Clone)]
pub(super) enum SessionAction {
    Open,
    /// Pins or unpins this terminal.
    Pin(String),
    Rename,
    /// Closes this terminal's pane, asking first while its Agent runs.
    Close(String),
    /// 标记完成, 放弃 or 撤销结局: the same endings the session's card offers.
    Settle(Outcome),
}

/// The last path segment, as a folder is named.
fn folder_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| cwd.to_owned())
}

impl Workbench {
    pub(super) fn choose_sidebar_preset(&mut self, id: &str, cx: &mut Context<Self>) {
        self.sidebar.preset = id.to_owned();
        self.sidebar.toggled.clear();
        self.sidebar.motion.clear_folds();
        self.sidebar.motion.arrive_plain();
        self.sidebar.save();
        cx.notify();
    }
    /// The session menu's 置顶: pins its terminal to 置顶, or takes it off.
    pub(super) fn toggle_pin(&mut self, terminal: &str, cx: &mut Context<Self>) {
        let pins = &mut self.sidebar.pinned;
        match pins.iter().position(|t| t == terminal) {
            Some(i) => {
                pins.remove(i);
            }
            None => pins.push(terminal.to_owned()),
        }
        self.sidebar.save();
        cx.notify();
    }
    /// A pin lasts as long as its terminal runs: one that ended or was closed comes off.
    /// Only against a workspace the host has sent, never the empty one before it.
    pub(super) fn prune_pins(&mut self) {
        if self.sidebar.pinned.is_empty() || self.workspace.revision == 0 || self.store.cut_off() {
            return;
        }
        let before = self.sidebar.pinned.len();
        let w = &self.workspace;
        self.sidebar.pinned.retain(|id| {
            w.terminals
                .iter()
                .any(|t| &t.id == id && t.state == ProcessState::Running)
        });
        if self.sidebar.pinned.len() != before {
            self.sidebar.save();
        }
    }
    /// ⌘B: the column folds to a rail and back, as Claude's desktop app does.
    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar.collapsed = !self.sidebar.collapsed;
        self.sidebar.save();
        cx.notify();
    }
    /// A group's heading in the 状态 / 目录 presets: small, its count in monospace, no rule.
    pub(super) fn group_title(
        title: String,
        count: Option<usize>,
        cx: &Context<Self>,
    ) -> gpui::Div {
        Self::tinted_title(title, count, T::color(T::secondary()), cx)
    }
    /// A group's heading whose name is in `ink`: 要你处理 and 在跑 in their states' ink.
    pub(super) fn tinted_title(
        title: String,
        count: Option<usize>,
        ink: gpui::Hsla,
        cx: &Context<Self>,
    ) -> gpui::Div {
        div()
            .flex()
            .items_baseline()
            .gap(px(T::S6))
            .px(px(T::S8))
            .mt(px(T::S14))
            .mb(px(T::S2))
            .text_size(px(T::CHIP_SIZE))
            .child(
                div()
                    .font_family(T::UI_FONT)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(ink)
                    .child(title),
            )
            .when_some(count, |d, n| {
                d.child(
                    div()
                        .font_family(T::mono(cx).family)
                        .text_color(T::color(T::muted()))
                        .child(n.to_string()),
                )
            })
    }
    pub(super) fn section_title(title: String, count: usize) -> gpui::Div {
        div()
            .flex()
            .px(px(T::S8))
            .mt(px(T::S12))
            .mb(px(T::S2))
            .child(label(title).flex_1().text_size(px(T::CHIP_SIZE)))
            .child(label(count.to_string()).text_size(px(T::CHIP_SIZE)))
    }
    /// A group's heading in the chosen preset's look.
    pub(super) fn heading(&self, title: String, count: usize, cx: &Context<Self>) -> gpui::Div {
        if self.sidebar.dense() {
            Self::group_title(title, Some(count), cx)
        } else {
            Self::section_title(title, count)
        }
    }
    /// A folder's heading: its name, `1 需要你` in the error ink when something in it
    /// needs you, and its session count. A folder with nothing running or waiting is
    /// folded and dimmed; its path shows only while the pointer is on it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn folder_row(
        &self,
        cwd: &str,
        key: &str,
        count: usize,
        needs: usize,
        open: bool,
        active: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let key = key.to_owned();
        let quiet = !active && needs == 0;
        let row_group = gpui::SharedString::from(format!("sidebar-{key}"));
        let mono = T::mono(cx).family;
        div()
            .id(row_group.clone())
            .group(row_group.clone())
            .h(px(T::ROW))
            .px(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S6))
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .hover(|s| s.bg(T::color(T::raised())))
            .font_family(T::UI_FONT)
            .text_size(px(T::SMALL))
            .child(
                div()
                    .w(px(T::S12))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .child(if open { "▾" } else { "▸" }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(px(T::SIDEBAR * 0.6))
                    .truncate()
                    .when(!quiet, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                    .text_color(T::color(if quiet { T::muted() } else { T::primary() }))
                    .child(folder_name(cwd)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .opacity(0.)
                    .group_hover(row_group, |s| s.opacity(1.))
                    .child(home_relative(cwd)),
            )
            .when(needs > 0, |d| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(T::CHIP_SIZE))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(Tone::Danger.ink())
                        .child(format!("{needs} {}", mooring_core::status::NEEDS_YOU)),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(mono)
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_sidebar_group(&key, open, cx)))
            .into_any_element()
    }
    /// The switch over the list: one segment per preset.
    /// 会话 and its count, heading the list in every preset. The ≑ for choosing the
    /// view shows while the pointer is over the row, or while its menu is open, like the
    /// filter button in Claude's desktop sidebar; it sits on this one sidebar-wide row so
    /// it never reads as belonging to a single group.
    pub(super) fn sessions_header(&self, count: usize, cx: &Context<Self>) -> gpui::Div {
        let open = self.sidebar.view_menu.is_some();
        // Inside the heading's own padding, so its right edge stays in line with the
        // rows' 在跑 / 几分钟前 and no extra column opens on the right.
        let icon = div()
            .id("sidebar-view")
            .role(gpui::accesskit::Role::Button)
            .aria_label("选择视图")
            .my(px(-T::S4))
            .size(px(T::ICON_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .opacity(if open { 1. } else { T::VIEW_IDLE_OPACITY })
            .group_hover("sessions-header", |s| s.opacity(1.))
            .when(open, |d| d.bg(T::color(T::button_hover())))
            .hover(|s| s.bg(T::color(T::button_hover())))
            .child(
                gpui::svg()
                    .path("icons/sliders.svg")
                    .size(px(T::VIEW_GLYPH))
                    .text_color(T::color(T::secondary())),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    this.sidebar.view_menu = match this.sidebar.view_menu {
                        Some(_) => None,
                        None => Some(gpui::point(event.position.x, event.position.y + px(T::S12))),
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        let head = if self.sidebar.preset().group == Grouping::Recent {
            // 会话优先 has no heading: its sections name themselves, and the view menu
            // sits in the column's foot.
            div()
        } else if self.sidebar.dense() {
            // 会话 3 on the left as the dense presets write it; the ≑ at the right end.
            Self::group_title("会话".into(), Some(count), cx)
                .child(div().flex_1())
                .child(icon)
        } else {
            // 会话 … ≑ 8: the count keeps its column, the ≑ sits just before it.
            div()
                .flex()
                .items_center()
                .px(px(T::S8))
                .mt(px(T::S12))
                .mb(px(T::S2))
                .child(label("会话").flex_1().text_size(px(T::CHIP_SIZE)))
                .child(icon.mr(px(T::S4)))
                .child(label(count.to_string()).text_size(px(T::CHIP_SIZE)))
        };
        head.group("sessions-header")
    }
    /// The views to pick from, the current one ticked; picking one closes the menu.
    pub(super) fn view_menu_view(
        &self,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let chosen = self.sidebar.preset().id.clone();
        let entries = self
            .sidebar_presets()
            .map(|p| {
                let id = p.id.clone();
                let on = p.id == chosen;
                div()
                    .id(gpui::SharedString::from(format!("sidebar-preset-{}", p.id)))
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(p.name.clone())
                    .h(px(T::ROW))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .rounded(px(T::CONTROL_RADIUS))
                    .text_size(px(T::SMALL))
                    .cursor_pointer()
                    .hover(|s| s.bg(T::color(T::button_hover())))
                    .child(div().flex_1().child(p.name.clone()))
                    .when(on, |d| {
                        d.child(div().text_color(T::color(T::link())).child("✓"))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sidebar.view_menu = None;
                        this.choose_sidebar_preset(&id, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        self.popup(position, entries, window, cx, |this| {
            this.sidebar.view_menu = None
        })
    }
}

#[cfg(test)]
mod recent_tests;
