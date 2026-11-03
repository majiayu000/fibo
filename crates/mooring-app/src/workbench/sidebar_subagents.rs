//! A session row's subagents in 会话优先 (docs/subagent-tree/index.html, option B): a
//! second line that counts them, a dot each or past [`PIPS`] a bar, and under it, once
//! opened, a branch per subagent with its state and clock, at most [`SHOWN`] of them
//! before the rest fold into one line that opens the whole list. Open by default on the
//! session you are in. The marks stay still: the clocks already tick each second, and a
//! dozen spinning rings would redraw the column every frame.
use super::layout::turn_clock;
use super::*;
use crate::ui::theme::Flat as F;
use mooring_core::content::Subagent;

fn ink(v: u32) -> gpui::Hsla {
    T::color(v)
}
/// Branches listed before the rest fold into 还有 N 个在跑 · M 个已完成; a list one longer
/// shows whole, a fold line in place of its last branch would hide nothing.
const SHOWN: usize = 4;
/// Subagents counted with a dot each; more take a bar.
const PIPS: usize = 8;
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
/// What the opened list shows: its branches in start order, and how many running and
/// ended ones fold away. Asked for whole (`all`), or short enough, it lists every one;
/// otherwise [`SHOWN`]: the running ones that have run longest, then the ones that ended
/// last.
struct Shown<'a> {
    list: Vec<(&'a Subagent, Leaf)>,
    running: usize,
    ended: usize,
}
fn shown(subs: &[Subagent], live: bool, all: bool) -> Shown<'_> {
    let mut leaves: Vec<_> = subs.iter().map(|s| (s, leaf(s, live))).collect();
    if all || leaves.len() <= SHOWN + 1 {
        return Shown {
            list: leaves,
            running: 0,
            ended: 0,
        };
    }
    let rank = |(s, l): &(&Subagent, Leaf)| match l {
        Leaf::Running => (0, s.started),
        _ => (1, u64::MAX - s.ended.unwrap_or(s.started)),
    };
    leaves.sort_by_key(rank);
    let rest = leaves.split_off(SHOWN);
    leaves.sort_by_key(|(s, _)| s.started);
    let running = rest.iter().filter(|(_, l)| *l == Leaf::Running).count();
    Shown {
        list: leaves,
        running,
        ended: rest.len() - running,
    }
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
            counts.push(format!("{running} {}", Status::Running.label()));
        }
        if ended > 0 {
            counts.push(format!("{ended} 完成"));
        }
        let pips = if leaves.len() <= PIPS {
            div()
                .flex()
                .items_center()
                .gap(px(F::SUB_PIP_GAP))
                .children(leaves.iter().map(|l| {
                    let pip = div().size(px(PIP)).rounded_full().flex_shrink_0();
                    match l {
                        Leaf::Running => pip.bg(run),
                        _ => pip.border(px(HAIR)).border_color(ink(F::third())),
                    }
                }))
        } else {
            // Ended on the left, still running on the right, in proportion.
            let share = |n: usize| F::SUB_BAR * n as f32 / leaves.len() as f32;
            div()
                .w(px(F::SUB_BAR))
                .h(px(F::SUB_BAR_HEIGHT))
                .flex_shrink_0()
                .flex()
                .rounded_full()
                .overflow_hidden()
                .bg(ink(F::third()).opacity(0.3))
                .child(div().w(px(share(ended))).h_full().bg(ink(F::third())))
                .child(div().w(px(share(running))).h_full().bg(run))
        };
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
        session: &str,
        subs: &[Subagent],
        live: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let whole = self.layout.subagents_all.contains(session);
        let Shown {
            list,
            running: more_running,
            ended: more_ended,
        } = shown(subs, live, whole);
        let folded = more_running + more_ended;
        // The last line: what folds away, or a way back to the short list.
        let fold = if folded > 0 {
            let mut parts = vec![];
            if more_running > 0 {
                parts.push(format!("{more_running} 个{}", Status::Running.label()));
            }
            if more_ended > 0 {
                parts.push(format!("{more_ended} 个已完成"));
            }
            Some(format!("还有 {}", parts.join(" · ")))
        } else {
            (whole && subs.len() > SHOWN + 1).then(|| "收起".to_owned())
        };
        let now = mooring_core::content::now();
        let line = ink(F::third()).opacity(0.45);
        let run = Status::Running.colors().0;
        if list.iter().any(|(_, l)| *l == Leaf::Running) {
            self.tick_turn(cx);
        }
        let last = list.len() - 1 + usize::from(fold.is_some());
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
            .when_some(fold, |d, text| {
                let id = session.to_owned();
                d.child(
                    div()
                        .id(gpui::SharedString::from(format!("subagents-all-{session}")))
                        .h(px(LEAF))
                        .flex()
                        .items_center()
                        .gap(px(T::S6))
                        .cursor_pointer()
                        .text_color(ink(F::third()))
                        .hover(|s| s.text_color(ink(F::second())))
                        .child(branch(last))
                        .child(div().flex_1().text_size(px(F::TAG)).child(text))
                        .child(super::sidebar_flat::fold_mark(whole))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if !this.layout.subagents_all.remove(&id) {
                                this.layout.subagents_all.insert(id.clone());
                            }
                            cx.notify();
                        })),
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
    fn names<'a>(s: &Shown<'a>) -> Vec<&'a str> {
        s.list.iter().map(|(s, _)| s.name.as_str()).collect()
    }
    #[test]
    fn the_longest_running_and_the_last_ended_stay_the_rest_fold() {
        let subs = [
            sub(1, Some(2)),
            sub(2, Some(30)),
            sub(3, None),
            sub(4, Some(5)),
            sub(5, None),
            sub(6, Some(7)),
        ];
        let s = shown(&subs, true, false);
        assert_eq!(names(&s), ["a2", "a3", "a5", "a6"]);
        assert_eq!((s.running, s.ended), (0, 2));
        // Asked for whole, every one in start order.
        assert_eq!(shown(&subs, true, true).list.len(), 6);
        // One more than fits shows whole.
        assert_eq!(shown(&subs[..5], true, false).list.len(), 5);
        // Once the session stopped, unfinished ones are not shown as running.
        assert!(
            shown(&subs, false, true)
                .list
                .iter()
                .all(|(_, l)| *l != Leaf::Running)
        );
    }
    #[test]
    fn twenty_running_keep_the_four_that_ran_longest() {
        let subs: Vec<_> = (1..=20).map(|i| sub(i, None)).collect();
        let s = shown(&subs, true, false);
        assert_eq!(names(&s), ["a1", "a2", "a3", "a4"]);
        assert_eq!((s.running, s.ended), (16, 0));
    }
}
