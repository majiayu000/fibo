//! A session row's subagents in 会话优先 (docs/subagent-tree/index.html, option B): a
//! second line that counts them with a dot each, and under it, once opened, one branch
//! per subagent with its state and clock. Open by default on the session you are in.
use super::layout::turn_clock;
use super::*;
use crate::ui::theme::Flat as F;
use mooring_core::content::Subagent;

fn ink(v: u32) -> gpui::Hsla {
    T::color(v)
}
/// Branches listed before the rest fold into 还有 N 个已完成.
const SHOWN: usize = 4;
const BRANCH: f32 = F::SUB_BRANCH;
const LEAF: f32 = F::SUB_LEAF;
const PIP: f32 = F::SUB_PIP;
const HAIR: f32 = T::BORDER_WIDTH;

/// What one branch shows: running (only while its session still runs), ended, or left
/// unfinished by a session that no longer runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leaf {
    Running,
    Ended,
    Unknown,
}
fn leaf(s: &Subagent, live: bool) -> Leaf {
    match (s.running(), live) {
        (false, _) => Leaf::Ended,
        (true, true) => Leaf::Running,
        (true, false) => Leaf::Unknown,
    }
}
/// The branches to list, oldest first: every running one, then the newest ended ones up
/// to [`SHOWN`]; and how many ended ones fold away.
fn shown(subs: &[Subagent], live: bool) -> (Vec<(&Subagent, Leaf)>, usize) {
    let all: Vec<_> = subs.iter().map(|s| (s, leaf(s, live))).collect();
    let running = all.iter().filter(|(_, l)| *l == Leaf::Running).count();
    let room = SHOWN.saturating_sub(running);
    let ended: Vec<usize> = (0..all.len())
        .filter(|i| all[*i].1 != Leaf::Running)
        .collect();
    let keep_from = ended.len().saturating_sub(room);
    let dropped: std::collections::HashSet<usize> = ended[..keep_from].iter().copied().collect();
    let list = all
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !dropped.contains(i))
        .map(|(_, x)| x)
        .collect();
    (list, dropped.len())
}

impl Workbench {
    /// Whether `session`'s subagent list is open: on the session you are in unless you
    /// closed it, elsewhere only once you opened it.
    pub(super) fn subagents_open(&self, session: &str, current: bool) -> bool {
        current != self.layout.subagents_toggled.contains(session)
    }
    /// The row's second line: 子代理, a dot per subagent, and the counts, with the fold
    /// chevron at its end. A click opens or closes the list without opening the session.
    pub(super) fn subagent_line(
        &self,
        session: &str,
        subs: &[Subagent],
        live: bool,
        open: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let run = Status::Running.colors().0;
        let leaves: Vec<Leaf> = subs.iter().map(|s| leaf(s, live)).collect();
        let running = leaves.iter().filter(|l| **l == Leaf::Running).count();
        let ended = leaves.len() - running;
        let mut counts = vec![];
        if running > 0 {
            counts.push(format!("{running} 在跑"));
        }
        if ended > 0 {
            counts.push(format!("{ended} 完成"));
        }
        let pips = div()
            .flex()
            .items_center()
            .gap(px(F::SUB_PIP_GAP))
            .children(leaves.iter().take(8).map(|l| {
                let pip = div().size(px(PIP)).rounded_full().flex_shrink_0();
                match l {
                    Leaf::Running => pip.bg(run),
                    _ => pip.border(px(HAIR)).border_color(ink(F::third())),
                }
            }));
        let id = session.to_owned();
        div()
            .id(gpui::SharedString::from(format!("subagents-{session}")))
            .mt(px(T::S2))
            .flex()
            .items_center()
            .gap(px(T::S6))
            .text_size(px(F::SUB))
            .line_height(px(F::SUB_LINE))
            .text_color(ink(F::second()))
            .cursor_pointer()
            .child("子代理")
            .child(pips)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(if running > 0 { run } else { ink(F::third()) })
                    .child(counts.join(" · ")),
            )
            .child(super::sidebar_flat::fold_mark(open))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if !this.layout.subagents_toggled.remove(&id) {
                    this.layout.subagents_toggled.insert(id.clone());
                }
                cx.notify();
            }))
            .into_any_element()
    }
    /// The opened list: a branch per subagent drawn from the row's state column, with a
    /// gap ring while it runs, a ✓ once it ended, and its clock: running time so far, or
    /// how long it took.
    pub(super) fn subagent_tree(
        &self,
        subs: &[Subagent],
        live: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let (list, folded) = shown(subs, live);
        let now = mooring_core::content::now();
        let line = ink(F::third()).opacity(0.45);
        let run = Status::Running.colors().0;
        if list.iter().any(|(_, l)| *l == Leaf::Running) {
            self.tick_turn(cx);
        }
        let last = list.len() - 1 + usize::from(folded > 0);
        let branch = |i: usize| {
            div()
                .relative()
                .w(px(BRANCH))
                .h(px(LEAF))
                .flex_shrink_0()
                .child(
                    div()
                        .absolute()
                        .left(px(F::SUB_TRUNK))
                        .top_0()
                        .w(px(HAIR))
                        .h(if i == last { px(LEAF / 2.) } else { px(LEAF) })
                        .bg(line),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(F::SUB_TRUNK))
                        .top(px(LEAF / 2.))
                        .w(px(BRANCH - F::SUB_TRUNK - 2. * HAIR))
                        .h(px(HAIR))
                        .bg(line),
                )
        };
        let icon = |path: &'static str, color: gpui::Hsla| {
            gpui::svg()
                .path(path)
                .size(px(F::STATE + 1.))
                .flex_shrink_0()
                .text_color(color)
        };
        let rows = list.iter().enumerate().map(|(i, (s, l))| {
            let (mark, name_ink, clock) = match l {
                Leaf::Running => (
                    icon("icons/state-run.svg", run),
                    F::second(),
                    turn_clock(now.saturating_sub(s.started)),
                ),
                Leaf::Ended => (
                    icon("icons/state-done.svg", ink(F::done())),
                    F::third(),
                    turn_clock(s.ended.unwrap_or(s.started).saturating_sub(s.started)),
                ),
                Leaf::Unknown => (
                    icon("icons/state-ring.svg", ink(F::idle())),
                    F::third(),
                    String::new(),
                ),
            };
            div()
                .h(px(LEAF))
                .flex()
                .items_center()
                .gap(px(T::S6))
                .child(branch(i))
                .child(mark)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(F::SUB))
                        .text_color(ink(name_ink))
                        .child(s.name.clone()),
                )
                .when(s.foreground && *l == Leaf::Running, |d| {
                    d.child(
                        div()
                            .px(px(F::SUB_MARK_PAD))
                            .rounded(px(F::SUB_MARK_PAD))
                            .border(px(HAIR))
                            .border_color(run.opacity(0.45))
                            .text_size(px(F::TAG - 1.))
                            .line_height(px(F::SUB_MARK_LINE))
                            .text_color(run)
                            .child("前台"),
                    )
                })
                .child(
                    div()
                        .min_w(px(F::TIME_WIDTH))
                        .flex_shrink_0()
                        .flex()
                        .justify_end()
                        .font_family(T::mono(cx).family)
                        .text_size(px(F::TIME))
                        .text_color(ink(F::third()))
                        .child(clock),
                )
        });
        div()
            .mt(px(T::S2))
            // From the state column: the row's state dot sits STATE wide, ROW_GAP before
            // the text, so the branches hang from under the title's first letter.
            .ml(px(F::STATE / 2. - F::ROW_GAP - F::STATE - F::SUB_TRUNK))
            .flex()
            .flex_col()
            .children(rows)
            .when(folded > 0, |d| {
                d.child(
                    div()
                        .h(px(LEAF))
                        .flex()
                        .items_center()
                        .gap(px(T::S6))
                        .child(branch(last))
                        .child(
                            div()
                                .text_size(px(F::TAG))
                                .text_color(ink(F::third()))
                                .child(format!("还有 {folded} 个已完成")),
                        ),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sub(started: u64, ended: Option<u64>) -> Subagent {
        Subagent {
            name: format!("a{started}"),
            started,
            ended,
            ..Default::default()
        }
    }
    #[test]
    fn running_ones_stay_and_the_oldest_ended_fold_away() {
        let subs = [
            sub(1, Some(2)),
            sub(2, Some(3)),
            sub(3, None),
            sub(4, Some(5)),
            sub(5, None),
            sub(6, Some(7)),
        ];
        let (list, folded) = shown(&subs, true);
        let names: Vec<_> = list.iter().map(|(s, _)| s.name.as_str()).collect();
        assert_eq!(names, ["a3", "a4", "a5", "a6"]);
        assert_eq!(folded, 2);
        // Once the session stopped, unfinished ones are not shown as running.
        let (list, _) = shown(&subs, false);
        assert!(list.iter().all(|(_, l)| *l != Leaf::Running));
    }
}
