//! The left column's motion: a group folding, the list arriving when the machine it shows
//! changes, the rail's mark sliding to that machine, a machine breathing while it connects.
//!
//! What it may cost is the rule here. Every frame of it draws the sidebar again, so:
//! - it is short and it ends: the last frame draws what the column draws at rest and asks
//!   for nothing, so an idle window pays nothing and no row keeps a wrapper;
//! - nothing right of the column moves, and terminal frames still skip the sidebar;
//! - only the breathing repeats, on `pace`'s timer rather than on every display frame.
//!
//! The curves take their time as an argument, so tests need no clock.
use super::sidebar_list::SidebarPrefs;
use super::*;
use crate::machines::Link;
use crate::ui::theme::Flat as F;
use mooring_core::sidebar::Machines;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

mod flight;
pub(in crate::workbench) use flight::{block_key, mix};

const ROW_IN: f32 = 180.;
const STAGGER: f32 = 16.;
/// Rows past this many start together, so a long group takes no longer than a short one.
const STAGGERED: usize = 10;
const CLOSE: f32 = 130.;
const ARRIVE: f32 = 170.;
const ARRIVE_STAGGER: f32 = 12.;
const RAIL: f32 = 220.;
/// How far the list comes from: enough to say which way, not enough to travel.
const SLIDE: f32 = 8.;
const BREATH: Duration = Duration::from_millis(1600);
/// Five draws a second: a slow pulse needs no more, and it lasts only while connecting.
const BREATH_STEP: Duration = Duration::from_millis(200);
fn ease_out(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}
fn ease_in(t: f32) -> f32 {
    t * t
}
/// Overshoots a little and settles, as a mark that was thrown there.
fn settle_spring(t: f32) -> f32 {
    let (c1, c3) = (1.2_f32, 2.2_f32);
    1. + c3 * (t - 1.).powi(3) + c1 * (t - 1.).powi(2)
}
fn part(elapsed: f32, length: f32) -> f32 {
    (elapsed / length).clamp(0., 1.)
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Way {
    opening: bool,
}
impl Way {
    fn total(self) -> f32 {
        if self.opening {
            ROW_IN + STAGGER * STAGGERED as f32
        } else {
            CLOSE
        }
    }
    /// How shown row `i` is, 0 to 1, at `done` of the whole fold.
    fn row(self, i: usize, done: f32) -> f32 {
        if self.opening {
            let at = done * self.total() - STAGGER * i.min(STAGGERED) as f32;
            ease_out(part(at, ROW_IN))
        } else {
            1. - ease_in(done)
        }
    }
    /// The chevron: 0 pointing right, 1 pointing down.
    fn chevron(self, done: f32) -> f32 {
        if self.opening {
            ease_out(part(done * self.total(), ROW_IN))
        } else {
            1. - ease_out(done)
        }
    }
}

struct Fold {
    key: String,
    way: Way,
    started: Instant,
    /// What moves here, as tall as it was drawn, by its place in the fold.
    heights: Rc<RefCell<HashMap<usize, f32>>>,
    /// How many rows start late, once drawn: frames past the last row's end would redraw
    /// the column to show nothing moving.
    late: Cell<usize>,
}
impl Fold {
    fn done(&self, now: Instant) -> f32 {
        part(
            now.saturating_duration_since(self.started).as_secs_f32() * 1000.,
            self.way.total(),
        )
    }
    /// `rows` shown `t` of their height. The first frame measures them and later ones use
    /// that, so nothing is estimated and nothing jumps at the end: a wrapper of unknown
    /// height has taffy measure its text again each frame (111 M instructions a frame for
    /// a group and 127 M, 8 ms, for a whole section, against 55 M and 58 M).
    fn clipped(&self, place: usize, t: f32, mut rows: Vec<gpui::AnyElement>) -> gpui::AnyElement {
        let height = self.heights.borrow().get(&place).copied();
        let outer = div().flex_shrink_0().overflow_hidden().opacity(t);
        let Some(height) = height else {
            let heights = self.heights.clone();
            let measure =
                move |bounds: gpui::Bounds<gpui::Pixels>, _: &mut Window, _: &mut gpui::App| {
                    heights
                        .borrow_mut()
                        .insert(place, f32::from(bounds.size.height));
                };
            // Its first frame, where opening shows nothing and closing all.
            return outer
                .when(self.way.opening, |d| d.h(px(0.)))
                .child(
                    div()
                        .relative()
                        .w_full()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .children(rows)
                        .child(
                            gpui::canvas(measure, |_, _, _, _| ())
                                .absolute()
                                .size_full(),
                        ),
                )
                .into_any_element();
        };
        // Measured: nothing but the clip, each frame of a long list adds a node per row.
        let outer = outer.h(px(height * t));
        if rows.len() == 1 {
            return outer.children(rows.pop()).into_any_element();
        }
        outer
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .children(rows),
            )
            .into_any_element()
    }
    fn ended(&self, now: Instant) -> bool {
        let last = if self.way.opening {
            (ROW_IN + STAGGER * self.late.get() as f32) / self.way.total()
        } else {
            1.
        };
        self.done(now) >= last
    }
}

struct Switch {
    started: Instant,
    from: (f32, f32),
    /// The rail's mark: the slot it left, and the slot it goes to.
    rail: Option<(f32, f32)>,
}
impl Switch {
    fn elapsed(&self, now: Instant) -> f32 {
        now.saturating_duration_since(self.started).as_secs_f32() * 1000.
    }
    fn total() -> f32 {
        (ARRIVE + ARRIVE_STAGGER * STAGGERED as f32).max(RAIL)
    }
    fn row(elapsed: f32, i: usize) -> f32 {
        ease_out(part(
            elapsed - ARRIVE_STAGGER * i.min(STAGGERED) as f32,
            ARRIVE,
        ))
    }
}

/// What is moving in the column now. Empty almost always.
#[derive(Default)]
pub(in crate::workbench) struct SidebarMotion {
    folds: Vec<Fold>,
    switch: Option<Switch>,
    flight: Option<flight::Flight>,
    seats: flight::Seats,
}
impl SidebarMotion {
    /// Turning back midway starts from where it is, not from the far end.
    fn fold(&mut self, key: &str, opening: bool, now: Instant) {
        if mooring_core::effect::reduce_motion() {
            return;
        }
        let way = Way { opening };
        // A share of the old way done leaves that share of the new one ahead.
        let behind = match self.folds.iter().position(|f| f.key == key) {
            Some(at) => 1. - self.folds.remove(at).done(now),
            None => 0.,
        };
        let ago = Duration::from_secs_f32(behind * way.total() / 1000.);
        let started = now.checked_sub(ago).unwrap_or(now);
        self.folds.push(Fold {
            key: key.to_owned(),
            way,
            started,
            heights: Rc::default(),
            late: Cell::new(STAGGERED),
        });
    }
    fn switched(&mut self, from: (f32, f32), rail: Option<(f32, f32)>, now: Instant) {
        if mooring_core::effect::reduce_motion() {
            return;
        }
        self.switch = Some(Switch {
            started: now,
            from,
            rail,
        });
    }
    /// The last frame's turn: what ended leaves, so this draw is the resting column.
    pub(super) fn settle(&mut self) {
        let now = clock();
        self.folds.retain(|f| !f.ended(now));
        if self
            .switch
            .as_ref()
            .is_some_and(|s| s.elapsed(now) >= Switch::total())
        {
            self.switch = None;
        }
        if self.flight.as_ref().is_some_and(|f| f.ended(now)) {
            self.flight = None;
        }
    }
    pub(super) fn moving(&self) -> bool {
        !self.folds.is_empty() || self.switch.is_some() || self.flight.is_some()
    }
    fn folding(&self, key: &str) -> Option<&Fold> {
        self.folds.iter().find(|f| f.key == key)
    }
    /// Where the rail's mark is drawn now, in slots, while it moves.
    pub(super) fn rail_at(&self) -> Option<f32> {
        let s = self.switch.as_ref()?;
        let (from, to) = s.rail?;
        let t = part(s.elapsed(clock()), RAIL);
        (t < 1.).then(|| from + (to - from) * settle_spring(t))
    }
    pub(super) fn clear_folds(&mut self) {
        self.folds.clear();
    }
    pub(super) fn arrive_plain(&mut self) {
        self.switched((0., 0.), None, clock());
    }
}

impl SidebarPrefs {
    /// Whether a group is open: its default, flipped once per click.
    pub fn open(&self, key: &str, default: bool) -> bool {
        self.toggled.get(key).copied().unwrap_or(default)
    }
    /// Whether a group's rows are drawn: open, or still closing.
    pub(super) fn showing(&self, key: &str, default: bool) -> bool {
        self.open(key, default) || self.motion.folding(key).is_some()
    }
    /// A heading's fold mark, turning while the group moves.
    pub(super) fn fold_mark(&self, key: &str, open: bool) -> gpui::Svg {
        let mark = super::sidebar_flat::fold_mark(open);
        match self.motion.folding(key) {
            None => mark,
            Some(f) => {
                let turn = f.way.chevron(f.done(clock()));
                // A quarter turn back from pointing down, at any point of it.
                super::sidebar_flat::fold_mark(true).with_transformation(
                    gpui::Transformation::rotate(gpui::radians(
                        (turn - 1.) * std::f32::consts::FRAC_PI_2,
                    )),
                )
            }
        }
    }
    /// The rows from `start` on, clipped and faded while group `key` moves, top first.
    /// `keep` are the positions in `out` that stay when it closes (the session you are in).
    pub(super) fn reveal(
        &self,
        key: &str,
        start: usize,
        keep: &[usize],
        out: &mut Vec<gpui::AnyElement>,
    ) {
        let Some(fold) = self.motion.folding(key) else {
            return;
        };
        let done = fold.done(clock());
        let mut moved = 0;
        let rows = out.split_off(start.min(out.len()));
        for (i, row) in rows.into_iter().enumerate() {
            if keep.contains(&(start + i)) {
                out.push(row);
                continue;
            }
            let t = fold.way.row(moved, done);
            out.push(fold.clipped(moved, t, vec![row]));
            moved += 1;
        }
        fold.late.set(moved.saturating_sub(1).min(STAGGERED));
    }
    /// A whole section moving as one block, wiped from the top. `part` tells two blocks of
    /// one section apart.
    pub(super) fn reveal_block(
        &self,
        key: &str,
        part: usize,
        start: usize,
        out: &mut Vec<gpui::AnyElement>,
    ) {
        let Some(fold) = self.motion.folding(key) else {
            return;
        };
        let rows = out.split_off(start.min(out.len()));
        fold.late.set(0);
        let t = fold.way.row(0, fold.done(clock()));
        out.push(fold.clipped(usize::MAX - part, t, rows));
    }
    /// The list arriving after the machine it shows changed.
    fn arrive(&self, rows: Vec<gpui::AnyElement>) -> Vec<gpui::AnyElement> {
        let Some(s) = self.motion.switch.as_ref() else {
            return rows;
        };
        let elapsed = s.elapsed(clock());
        rows.into_iter()
            .enumerate()
            .map(|(i, row)| {
                let t = Switch::row(elapsed, i);
                let rest = SLIDE * (1. - t);
                div()
                    .relative()
                    .flex_shrink_0()
                    .left(px(s.from.0 * rest))
                    .top(px(s.from.1 * rest))
                    .opacity(t)
                    .child(row)
                    .into_any_element()
            })
            .collect()
    }
}

/// The time motion is drawn at; a test window holds it still to look at one moment.
#[cfg(not(feature = "visual-test"))]
fn clock() -> Instant {
    Instant::now()
}
#[cfg(feature = "visual-test")]
static HELD: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
#[cfg(feature = "visual-test")]
fn clock() -> Instant {
    HELD.lock().unwrap().unwrap_or_else(Instant::now)
}

impl Workbench {
    /// Opens a group shown closed, or closes one shown open, and keeps it that way.
    pub(super) fn toggle_sidebar_group(&mut self, key: &str, open: bool, cx: &mut Context<Self>) {
        self.sidebar.toggled.insert(key.to_owned(), !open);
        self.sidebar.motion.fold(key, !open, clock());
        cx.notify();
    }
    /// Opens a group by itself (the next-waiting key landing in it).
    pub(super) fn open_sidebar_group(&mut self, key: String) {
        if !self.sidebar.open(&key, true) {
            self.sidebar.motion.fold(&key, true, clock());
        }
        self.sidebar.toggled.insert(key, true);
    }
    /// A 会话优先 section's heading that folds its rows away and opens them again.
    pub(super) fn flat_fold(
        &self,
        key: &str,
        title: &str,
        count: usize,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let open = self.sidebar.open(key, true);
        let key = key.to_owned();
        Self::flat_group(title, count)
            .id(gpui::SharedString::from(format!("sidebar-group-{key}")))
            .cursor_pointer()
            // At the line's right end, level with the rows' times below it.
            .child(div().ml_auto().child(self.sidebar.fold_mark(&key, open)))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_sidebar_group(&key, open, cx)))
            .into_any_element()
    }
    /// Lists one of the shown machine's spaces alone, or all of them again.
    pub(super) fn toggle_machine_space(&mut self, id: &str, cx: &mut Context<Self>) {
        let again = self.sidebar.machine_space.as_deref() == Some(id);
        self.sidebar.machine_space = (!again).then(|| id.to_owned());
        self.sidebar.motion.arrive_plain();
        cx.notify();
    }
    /// The machine the column shows. The list comes from the side you went: down the rail
    /// from below, along the chips from the right.
    pub(super) fn set_machine_scope(&mut self, host: Option<String>) {
        if self.sidebar.machine_scope == host {
            return;
        }
        let slot = |w: &Self, h: &Option<String>| -> f32 {
            h.as_ref()
                .and_then(|h| {
                    // Its place in the rail, which leaves out quiet machines.
                    (0..w.machines.len())
                        .filter(|i| !w.machine_quiet(*i))
                        .position(|i| w.machine_host(i).as_deref() == Some(h))
                })
                .map_or(0., |i| i as f32 + 1.)
        };
        let (from, to) = (slot(self, &self.sidebar.machine_scope), slot(self, &host));
        let way = (to - from).signum();
        let (side, rail) = match self.machine_layout() {
            Machines::Rail => ((0., way), Some((from, to))),
            Machines::Mixed => ((way, 0.), None),
            _ => ((0., 0.), None),
        };
        self.sidebar.motion.switched(side, rail, clock());
        self.sidebar.machine_scope = host;
        self.sidebar.machine_space = None;
    }
    /// The local rows for the list: the local section's fold and the list's arrival.
    pub(super) fn moving_rows(&self, mut rows: Vec<gpui::AnyElement>) -> Vec<gpui::AnyElement> {
        self.sidebar.reveal_block("machine:local", 1, 0, &mut rows);
        self.sidebar.arrive(rows)
    }
    /// How lit a machine's mark is while it connects, slowly; `None` once it has settled.
    pub(super) fn machine_breath(&self, index: usize, cx: &Context<Self>) -> Option<f32> {
        static BREATHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let connecting = self.machines.get(index)?.view.lock().unwrap().link == Link::Connecting;
        if !connecting || mooring_core::effect::reduce_motion() {
            return None;
        }
        super::pace::wake(&BREATHING, BREATH_STEP, cx);
        let wave = (super::pace::phase(BREATH) * std::f32::consts::TAU).cos();
        Some(0.65 - 0.3 * wave)
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// Holds the column's clock at `ms` past the moment it was first held.
    pub fn visual_hold_motion(&self, ms: Option<u64>) {
        static FROM: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
        let mut from = FROM.lock().unwrap();
        let at = ms.map(|ms| *from.get_or_insert_with(Instant::now) + Duration::from_millis(ms));
        if ms.is_none() {
            *from = None;
        }
        *HELD.lock().unwrap() = at;
    }
    pub fn visual_toggle_group(&mut self, key: &str, cx: &mut Context<Self>) {
        let open = self.sidebar.open(key, true);
        self.toggle_sidebar_group(key, open, cx);
    }
    pub fn visual_machine_space(&mut self, id: &str, cx: &mut Context<Self>) {
        self.toggle_machine_space(id, cx);
    }
    pub fn visual_arrive(&mut self, cx: &mut Context<Self>) {
        self.sidebar.motion.arrive_plain();
        cx.notify();
    }
    pub fn visual_motion_moving(&self) -> bool {
        self.sidebar.motion.moving()
    }
    /// The longest any motion here runs, in ms.
    pub fn visual_motion_longest() -> u64 {
        Way { opening: true }.total().max(Switch::total()) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fold_ends_at_rest_both_ways() {
        let (open, close) = (Way { opening: true }, Way { opening: false });
        for i in [0, 1, 9, 10, 500] {
            assert_eq!(open.row(i, 0.), 0., "row {i} starts hidden");
            assert_eq!(open.row(i, 1.), 1., "row {i} ends shown");
            assert_eq!(close.row(i, 0.), 1., "row {i} starts shown");
            assert_eq!(close.row(i, 1.), 0., "row {i} ends hidden");
        }
        assert_eq!((open.chevron(0.), open.chevron(1.)), (0., 1.));
        assert_eq!((close.chevron(0.), close.chevron(1.)), (1., 0.));
    }

    /// The cost of a click is its length times a frame: a long group must not stretch it.
    #[test]
    fn no_motion_runs_past_a_third_of_a_second() {
        assert!(Way { opening: true }.total() <= 340.);
        assert!(Way { opening: false }.total() <= 340.);
        assert!(Switch::total() <= 340.);
        let open = Way { opening: true };
        assert_eq!(
            open.row(5000, 1.),
            1.,
            "the last of thousands is done by the end"
        );
    }

    #[test]
    fn rows_open_top_down() {
        let open = Way { opening: true };
        let done = 0.4;
        assert!(open.row(0, done) > open.row(1, done));
        assert!(open.row(1, done) > open.row(5, done));
        assert_eq!(
            open.row(10, done),
            open.row(5000, done),
            "past the stagger they go together"
        );
    }

    #[test]
    fn turning_back_keeps_going_from_where_it_was() {
        let now = Instant::now();
        let mut m = SidebarMotion::default();
        m.folds.push(Fold {
            key: "k".into(),
            way: Way { opening: false },
            started: now - Duration::from_millis(65),
            heights: Rc::default(),
            late: Cell::new(STAGGERED),
        });
        m.fold("k", true, now);
        assert_eq!(m.folds.len(), 1, "one group, one fold");
        let f = &m.folds[0];
        assert!(f.way.opening);
        let done = f.done(now);
        assert!(
            (done - 0.5).abs() < 0.02,
            "half closed turns back half open, got {done}"
        );
    }

    #[test]
    fn the_rail_mark_lands_on_its_slot() {
        let now = Instant::now();
        let mut m = SidebarMotion {
            switch: Some(Switch {
                started: now,
                from: (0., 1.),
                rail: Some((0., 2.)),
            }),
            ..Default::default()
        };
        assert!(m.moving());
        assert_eq!(settle_spring(0.), 0.);
        assert_eq!(settle_spring(1.), 1.);
        m.switch.as_mut().unwrap().started = now - Duration::from_secs(1);
        assert_eq!(m.rail_at(), None, "at rest the button itself shows it");
        m.settle();
        assert!(!m.moving(), "nothing left to draw: no frame is asked for");
    }

    /// A short group asks for no frames past its last row, or a 200 ms motion draws 340.
    #[test]
    fn a_short_group_ends_with_its_last_row() {
        let now = Instant::now();
        let fold = Fold {
            key: "k".into(),
            way: Way { opening: true },
            started: now - Duration::from_millis(200),
            heights: Rc::default(),
            late: Cell::new(1),
        };
        assert!(fold.ended(now), "two rows are done by 196 ms");
        fold.late.set(STAGGERED);
        assert!(!fold.ended(now), "eleven rows are not");
    }

    #[test]
    fn settling_keeps_only_what_still_moves() {
        let now = clock();
        let mut m = SidebarMotion::default();
        m.fold("old", false, now - Duration::from_secs(5));
        m.fold("new", true, now + Duration::from_secs(5));
        m.settle();
        assert!(m.folding("old").is_none());
        assert!(m.folding("new").is_some());
    }
}
