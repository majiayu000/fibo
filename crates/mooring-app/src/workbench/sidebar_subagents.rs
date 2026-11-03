//! A session row's subagents in 会话优先 (docs/subagent-tree/index.html, option B): a
//! count line (a dot each, a bar past [`PIPS`]) and, once opened, a branch per subagent
//! with its clock, [`SHOWN`] at most before the rest fold into one line that opens them
//! all. Open by default on the session you are in. The marks stay still: the clocks
//! already tick each second, and spinning rings would redraw the column every frame.
use super::layout::turn_clock;
use super::*;
use crate::ui::theme::Flat as F;
use mooring_core::content::Subagent;

fn ink(v: u32) -> gpui::Hsla {
    T::color(v)
}
const SHOWN: usize = 4;
const PIPS: usize = 8;
const LEAF: f32 = F::SUB_LEAF;
const HAIR: f32 = T::BORDER_WIDTH;

/// Running only while its session runs: one that stopped leaves its unfinished
/// subagents on disk, and those read as ended.
fn runs(s: &Subagent, live: bool) -> bool {
    live && s.running()
}
/// "3 在跑 · 1 完成", or with `each` = "个" and `done` = "已完成" for the fold line.
fn tally(running: usize, ended: usize, each: &str, done: &str) -> String {
    let run = (running > 0).then(|| format!("{running} {each}{}", Status::Running.label()));
    let end = (ended > 0).then(|| format!("{ended} {each}{done}"));
    run.into_iter().chain(end).collect::<Vec<_>>().join(" · ")
}
fn flip(set: &mut HashSet<String>, id: &str) {
    if !set.remove(id) {
        set.insert(id.to_owned());
    }
}
/// Your last click on a row, else its default: open on the session you are in. Storing
/// only "clicked" would turn a list you closed back open once you went elsewhere.
fn opened(chosen: &HashMap<String, bool>, session: &str, current: bool) -> bool {
    chosen.get(session).copied().unwrap_or(current)
}

/// The branches listed, in start order, and how many running and ended fold away. All
/// of them when asked or when only one would fold; otherwise the running ones that have
/// run longest, then those that ended last.
struct Shown<'a> {
    list: Vec<&'a Subagent>,
    running: usize,
    ended: usize,
}
fn shown(subs: &[Subagent], live: bool, all: bool) -> Shown<'_> {
    let mut list: Vec<&Subagent> = subs.iter().collect();
    if all || list.len() <= SHOWN + 1 {
        return Shown {
            list,
            running: 0,
            ended: 0,
        };
    }
    list.sort_by_key(|s| match runs(s, live) {
        true => (0, s.started),
        false => (1, u64::MAX - s.ended.unwrap_or(s.started)),
    });
    let rest = list.split_off(SHOWN);
    list.sort_by_key(|s| s.started);
    let running = rest.iter().filter(|s| runs(s, live)).count();
    Shown {
        list,
        running,
        ended: rest.len() - running,
    }
}

impl Workbench {
    pub(super) fn subagents_open(&self, session: &str, current: bool) -> bool {
        opened(&self.layout.subagents_toggled, session, current)
    }
    /// The row's second line: 子代理, the dots or bar, the counts and the fold chevron. A
    /// click opens or closes the list without opening the session.
    pub(super) fn subagent_line(
        &self,
        session: &str,
        subs: &[Subagent],
        live: bool,
        open: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let (run, quiet) = (Status::Running.colors().0, ink(F::third()));
        let running = subs.iter().filter(|s| runs(s, live)).count();
        let ended = subs.len() - running;
        let marks = if subs.len() <= PIPS {
            div()
                .flex()
                .gap(px(F::SUB_PIP_GAP))
                .children(subs.iter().map(|s| {
                    let pip = div().size(px(F::SUB_PIP)).rounded_full();
                    match runs(s, live) {
                        true => pip.bg(run),
                        false => pip.border(px(HAIR)).border_color(quiet),
                    }
                }))
        } else {
            // Ended on the left, running on the right, in proportion.
            let share = |n: usize| px(F::SUB_BAR * n as f32 / subs.len() as f32);
            div()
                .w(px(F::SUB_BAR))
                .h(px(F::SUB_BAR_HEIGHT))
                .flex()
                .rounded_full()
                .overflow_hidden()
                .child(div().w(share(ended)).bg(quiet))
                .child(div().w(share(running)).bg(run))
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
            .child(marks.flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_color(if running > 0 { run } else { quiet })
                    .child(tally(running, ended, "", "完成")),
            )
            .child(super::sidebar_flat::fold_mark(open))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.layout.subagents_toggled.insert(id.clone(), !open);
                cx.notify();
            }))
            .into_any_element()
    }
    /// The opened list: a branch per subagent hung from under the row's state mark, a
    /// gap ring and its time so far while it runs, a ✓ and how long it took once ended;
    /// then what folds away, or 收起 once the whole list is open.
    pub(super) fn subagent_tree(
        &self,
        session: &str,
        subs: &[Subagent],
        live: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let whole = self.layout.subagents_all.contains(session);
        let shown = shown(subs, live, whole);
        let fold = if shown.running + shown.ended > 0 {
            Some(format!(
                "还有 {}",
                tally(shown.running, shown.ended, "个", "已完成")
            ))
        } else {
            (whole && subs.len() > SHOWN + 1).then(|| "收起".to_owned())
        };
        let (now, run, line) = (
            mooring_core::content::now(),
            Status::Running.colors().0,
            ink(F::third()).opacity(0.45),
        );
        let last = shown.list.len() - usize::from(fold.is_none());
        // A trunk down the left, cut at the last branch, and a twig to each.
        let leaf = move |i: usize| {
            div()
                .h(px(LEAF))
                .flex()
                .items_center()
                .gap(px(T::S6))
                .text_size(px(F::SUB))
                .child(
                    div()
                        .relative()
                        .w(px(F::SUB_BRANCH))
                        .h_full()
                        .flex_shrink_0()
                        .child(
                            div()
                                .absolute()
                                .left(px(F::SUB_TRUNK))
                                .w(px(HAIR))
                                .h(px(if i == last { LEAF / 2. } else { LEAF }))
                                .bg(line),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(F::SUB_TRUNK))
                                .top(px(LEAF / 2.))
                                .w(px(F::SUB_BRANCH - F::SUB_TRUNK))
                                .h(px(HAIR))
                                .bg(line),
                        ),
                )
        };
        let mut running = false;
        let rows = shown
            .list
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let on = runs(s, live);
                running |= on;
                let (path, color, clock) = match (on, s.ended) {
                    (true, _) => (
                        "icons/state-run.svg",
                        run,
                        turn_clock(now.saturating_sub(s.started)),
                    ),
                    (false, Some(end)) => (
                        "icons/state-done.svg",
                        ink(F::done()),
                        turn_clock(end.saturating_sub(s.started)),
                    ),
                    (false, None) => ("icons/state-done.svg", ink(F::done()), String::new()),
                };
                leaf(i)
                    .child(
                        gpui::svg()
                            .path(path)
                            .size(px(F::STATE + 1.))
                            .flex_shrink_0()
                            .text_color(color),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_color(ink(if on { F::second() } else { F::third() }))
                            .child(s.name.clone()),
                    )
                    .child(
                        div()
                            .min_w(px(F::TIME_WIDTH))
                            .flex()
                            .justify_end()
                            .font_family(T::mono(cx).family)
                            .text_size(px(F::TIME))
                            .text_color(ink(F::third()))
                            .child(clock),
                    )
            })
            .collect::<Vec<_>>();
        if running {
            self.tick_turn(cx);
        }
        let id = session.to_owned();
        div()
            .mt(px(T::S2))
            // From the state column: the branches hang from under the row's state mark.
            .ml(px(F::STATE / 2. - F::ROW_GAP - F::STATE - F::SUB_TRUNK))
            .flex()
            .flex_col()
            .children(rows)
            .when_some(fold, |d, text| {
                d.child(
                    leaf(last)
                        .id(gpui::SharedString::from(format!("subagents-all-{session}")))
                        .cursor_pointer()
                        .text_color(ink(F::third()))
                        .hover(|s| s.text_color(ink(F::second())))
                        .child(div().flex_1().child(text))
                        .child(super::sidebar_flat::fold_mark(whole))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            flip(&mut this.layout.subagents_all, &id);
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
    #[test]
    fn a_list_you_closed_stays_closed_when_you_go_elsewhere() {
        let mut chosen = HashMap::new();
        assert!(opened(&chosen, "s", true));
        assert!(!opened(&chosen, "s", false));
        // Closed while in it, then another session is selected.
        chosen.insert("s".to_owned(), false);
        assert!(!opened(&chosen, "s", true));
        assert!(!opened(&chosen, "s", false));
        // Opened elsewhere stays open when you come back.
        chosen.insert("o".to_owned(), true);
        assert!(opened(&chosen, "o", false));
        assert!(opened(&chosen, "o", true));
    }
    fn sub(started: u64, ended: Option<u64>) -> Subagent {
        Subagent {
            name: format!("a{started}"),
            started,
            ended,
        }
    }
    fn names<'a>(s: &Shown<'a>) -> Vec<&'a str> {
        s.list.iter().map(|s| s.name.as_str()).collect()
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
        assert_eq!(shown(&subs, true, true).list.len(), 6);
        // One more than fits shows whole.
        assert_eq!(shown(&subs[..5], true, false).list.len(), 5);
        // Twenty running keep the four that ran longest.
        let many: Vec<_> = (1..=20).map(|i| sub(i, None)).collect();
        let s = shown(&many, true, false);
        assert_eq!(names(&s), ["a1", "a2", "a3", "a4"]);
        assert_eq!((s.running, s.ended), (16, 0));
        // Once the session stopped, unfinished ones count as ended.
        assert_eq!(shown(&many, false, false).ended, 16);
        assert_eq!(tally(2, 1, "", "完成"), "2 在跑 · 1 完成");
    }
}
