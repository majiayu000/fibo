//! Where the column's list sits as it grows past the window. It has one scroll, the
//! column's, and two rules: opening a session brings its row into sight, and rows changing
//! elsewhere leave what you are reading where it is. Nothing is measured: once per draw of
//! the column this reads the bounds its layout already has.
use super::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug, PartialEq)]
struct Placed {
    id: String,
    group: Bucket,
    /// From the top of the list's content, so scrolling does not change them.
    top: f32,
    bottom: f32,
}

#[derive(Default)]
struct Seen {
    scope: String,
    rows: Vec<Placed>,
    /// The session whose row was last brought into sight.
    shown: Option<String>,
}

#[derive(Default)]
pub(in crate::workbench) struct ListScroll {
    pub(in crate::workbench) handle: gpui::ScrollHandle,
    /// This draw's session rows: their place in the list, session and group.
    marks: RefCell<Vec<(usize, String, Bucket)>>,
    seen: Rc<RefCell<Seen>>,
}
impl ListScroll {
    /// Rows just moved on purpose (a fold): nothing to hold still against.
    pub(in crate::workbench) fn forget(&self) {
        self.seen.borrow_mut().rows.clear();
    }
}

/// The scroll that shows `row` whole, or `None` if it already is. A row taller than the
/// list starts at the top.
fn to_show(row: &Placed, scroll: f32, height: f32) -> Option<f32> {
    if row.top < scroll || row.bottom - row.top > height {
        return (row.top != scroll).then_some(row.top);
    }
    (row.bottom > scroll + height).then_some(row.bottom - height)
}

/// The scroll that leaves the first row you can see, still in its group, where it was. At
/// the very top there is nothing to hold: what arrives above should be seen.
fn to_hold(was: &[Placed], now: &[Placed], scroll: f32, height: f32) -> Option<f32> {
    if scroll <= 0. {
        return None;
    }
    let moved = was
        .iter()
        .filter(|r| r.bottom > scroll && r.top < scroll + height)
        .find_map(|r| {
            let same = now.iter().find(|n| n.id == r.id && n.group == r.group)?;
            Some(same.top - r.top)
        })?;
    (moved.abs() >= 0.5).then_some((scroll + moved).max(0.))
}

/// Where the list should scroll to now, if anywhere else. A new session wins over holding.
fn next_scroll(
    seen: &Seen,
    now: &[Placed],
    current: Option<&str>,
    scope: &str,
    scroll: f32,
    height: f32,
) -> Option<f32> {
    if seen.shown.as_deref() != current
        && let Some(row) = now.iter().find(|r| Some(r.id.as_str()) == current)
    {
        return to_show(row, scroll, height);
    }
    (seen.scope == scope)
        .then(|| to_hold(&seen.rows, now, scroll, height))
        .flatten()
}

impl Workbench {
    /// Remembers that the next row pushed at `at` is this session's.
    pub(in crate::workbench) fn note_row(&self, at: usize, l: &Listed) {
        let group = self.row_bucket(l);
        (self.sidebar.scroll.marks.borrow_mut()).push((at, l.0.id.clone(), group));
    }
    /// Goes right before the list's rows, which stay the column's own children: a box around
    /// them would have taffy measure every row's text again (73 M to 123 M a redraw). The
    /// column's handle already knows where its children are; this marks where the rows
    /// start among them and runs once they are placed.
    pub(in crate::workbench) fn list_watch(&self, current: Option<String>) -> gpui::AnyElement {
        let marks = self.sidebar.scroll.marks.take();
        let moving = self.sidebar.motion.moving();
        let scope = format!(
            "{}|{:?}|{:?}|{:?}",
            self.sidebar.preset,
            self.sidebar.space_filter,
            self.sidebar.machine_scope,
            self.sidebar.machine_space
        );
        let handle = self.sidebar.scroll.handle.clone();
        let seen = self.sidebar.scroll.seen.clone();
        let sidebar = self.redraw.as_ref().map(|r| r.sidebar.clone());
        let watch = move |_, window: &mut Window, _: &mut gpui::App| {
            let view = handle.bounds();
            let height = f32::from(view.size.height);
            let mut seen = seen.borrow_mut();
            // Mid-fold the rows are wrapped and moving: start again once they settle.
            if moving || height <= 0. {
                seen.rows.clear();
                return;
            }
            // Itself among the column's children: the last empty box at the very top.
            let Some(start) = (0..handle.children_count()).rev().find(|i| {
                handle.bounds_for_item(*i).is_some_and(|b| {
                    f32::from(b.size.height) == 0. && f32::from(b.top() - view.top()).abs() < 0.5
                })
            }) else {
                return;
            };
            // Before the column's own scroll, so these are places in its content.
            let now: Vec<Placed> = marks
                .iter()
                .filter_map(|(at, id, group)| {
                    let row = handle.bounds_for_item(start + 1 + at)?;
                    let top = f32::from(row.top() - view.top());
                    Some(Placed {
                        id: id.clone(),
                        group: *group,
                        top,
                        bottom: top + f32::from(row.size.height),
                    })
                })
                .collect();
            let scroll = -f32::from(handle.offset().y);
            let to = next_scroll(&seen, &now, current.as_deref(), &scope, scroll, height);
            // A session with no row yet is shown once it has one.
            if current.is_none() || now.iter().any(|r| Some(&r.id) == current.as_ref()) {
                seen.shown = current.clone();
            }
            seen.rows = now;
            seen.scope = scope.clone();
            let Some(to) = to else {
                return;
            };
            handle.set_offset(gpui::point(handle.offset().x, px(-to)));
            // This draw is already placed, and the column draws only when asked.
            if let Some(view) = sidebar.clone() {
                window.on_next_frame(move |_, cx| super::super::redraw::ask(&view, cx));
            }
        };
        gpui::canvas(watch, |_, _, _, _| ())
            .absolute()
            .top_0()
            .left_0()
            .size_0()
            .into_any_element()
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// How far the list is scrolled, how tall it shows and how far it can go.
    pub fn visual_list_scroll(&self) -> (f32, f32, f32) {
        let handle = &self.sidebar.scroll.handle;
        (
            -f32::from(handle.offset().y),
            f32::from(handle.bounds().size.height),
            f32::from(handle.max_offset().y),
        )
    }
    pub fn visual_set_list_scroll(&mut self, to: f32, cx: &mut Context<Self>) {
        let handle = &self.sidebar.scroll.handle;
        handle.set_offset(gpui::point(handle.offset().x, px(-to)));
        cx.notify();
    }
    /// Listed sessions, top to bottom: id, its top and bottom in the content, its group.
    pub fn visual_list_rows(&self) -> Vec<(String, f32, f32, Bucket)> {
        let seen = self.sidebar.scroll.seen.borrow();
        seen.rows
            .iter()
            .map(|r| (r.id.clone(), r.top, r.bottom, r.group))
            .collect()
    }
    /// Where the row of the session last brought into sight is.
    pub fn visual_shown_row(&self) -> Option<(f32, f32)> {
        let seen = self.sidebar.scroll.seen.borrow();
        let row = seen
            .rows
            .iter()
            .find(|r| Some(&r.id) == seen.shown.as_ref())?;
        Some((row.top, row.bottom))
    }
    pub fn visual_focus_session(&mut self, terminal: &str, cx: &mut Context<Self>) {
        self.nav.focused = Some(terminal.to_owned());
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEIGHT: f32 = 100.;

    fn row(id: &str, group: Bucket, top: f32) -> Placed {
        Placed {
            id: id.into(),
            group,
            top,
            bottom: top + 26.,
        }
    }
    fn list(rows: &[(&str, f32)]) -> Vec<Placed> {
        rows.iter()
            .map(|(id, top)| row(id, Bucket::Running, *top))
            .collect()
    }
    fn seen(rows: Vec<Placed>, shown: Option<&str>) -> Seen {
        Seen {
            scope: "s".into(),
            rows,
            shown: shown.map(str::to_owned),
        }
    }

    #[test]
    fn a_session_opened_out_of_sight_comes_into_sight_and_no_further() {
        let row = row("a", Bucket::Running, 200.);
        assert_eq!(
            to_show(&row, 0., HEIGHT),
            Some(126.),
            "below: its bottom at the edge"
        );
        assert_eq!(
            to_show(&row, 300., HEIGHT),
            Some(200.),
            "above: its top at the edge"
        );
        assert_eq!(
            to_show(&row, 150., HEIGHT),
            None,
            "in sight: the list stays"
        );
    }

    #[test]
    fn opening_scrolls_once_so_you_can_scroll_away_from_it() {
        let now = list(&[("a", 200.)]);
        let opened = seen(now.clone(), Some("b"));
        assert_eq!(
            next_scroll(&opened, &now, Some("a"), "s", 0., HEIGHT),
            Some(126.)
        );
        let same = seen(now.clone(), Some("a"));
        assert_eq!(next_scroll(&same, &now, Some("a"), "s", 0., HEIGHT), None);
    }

    #[test]
    fn rows_above_changing_leave_what_you_read_where_it_is() {
        let was = list(&[("gone", 0.), ("read", 100.), ("next", 126.)]);
        let now = list(&[("read", 74.), ("next", 100.)]);
        assert_eq!(to_hold(&was, &now, 90., HEIGHT), Some(64.));
    }

    #[test]
    fn nothing_is_held_at_the_top_or_when_nothing_moved() {
        let was = list(&[("a", 0.)]);
        let now = list(&[("new", 0.), ("a", 26.)]);
        assert_eq!(
            to_hold(&was, &now, 0., HEIGHT),
            None,
            "the new row should be seen"
        );
        assert_eq!(to_hold(&was, &was, 10., HEIGHT), None);
    }

    #[test]
    fn a_row_changing_group_does_not_drag_the_list_after_it() {
        let was = vec![
            row("left", Bucket::Running, 100.),
            row("stays", Bucket::Running, 126.),
        ];
        let now = vec![
            row("left", Bucket::Attention, 0.),
            row("stays", Bucket::Running, 100.),
        ];
        assert_eq!(to_hold(&was, &now, 90., HEIGHT), Some(64.));
    }

    #[test]
    fn nothing_is_held_across_another_layout_or_filter() {
        let was = seen(list(&[("a", 100.)]), None);
        let now = list(&[("a", 300.)]);
        assert_eq!(next_scroll(&was, &now, None, "s", 90., HEIGHT), Some(290.));
        assert_eq!(next_scroll(&was, &now, None, "other", 90., HEIGHT), None);
    }

    #[test]
    fn the_list_counts_no_rows_of_its_own() {
        let source = include_str!("session_list.rs");
        assert!(!source.contains("preset.limit"), "use Preset::row_limit");
    }
}
