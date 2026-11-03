//! Moving panes and tabs by dragging. A pane picked up by its title bar lands on
//! another pane's edge (half of it, on that side), its middle (the two trade places),
//! a tab (joins it; resting there opens it first), or 「＋」 (a tab of its own). A tab dragged along the strip lands before or after the tab it is over.
//!
//! Drag moves arrive for every listening pane and tab on every mouse move; each only
//! notes where the drop would land and redraws when that changes, so a drag costs a
//! few frames, not one per move. The terminals themselves are cached views and are
//! not redrawn for it.
use super::*;
use gpui::{Bounds, DragMoveEvent, Pixels, Point, Stateful, relative};
use std::time::Duration;

/// How long a dragged pane rests on a tab before that tab opens under it.
const SPRING: Duration = Duration::from_millis(450);
/// Half the width and height of a pane's middle, where a drop trades places.
const MIDDLE: f32 = 0.2;

/// A pane being dragged by its title bar, drawn beside the pointer by its name.
#[derive(Clone)]
pub(in crate::workbench) struct DraggedPane {
    pub(in crate::workbench) id: String,
    pub(in crate::workbench) title: String,
    /// Where in the title bar it was picked up. GPUI draws the chip that far up and
    /// left of the pointer, as if the whole bar were carried; a wide bar would leave
    /// the chip far from the pointer, so the chip is pushed back next to it.
    grab: Point<Pixels>,
}
impl gpui::Render for DraggedPane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.grab.x + px(T::S12))
            .pt(self.grab.y + px(T::S12))
            .child(super::super::tabs::drag_chip(self.title.clone()))
    }
}

/// Where on the pane under it a dragged pane would land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::workbench) enum Zone {
    /// Half of the pane, on the side `before` (left or top) or after it.
    Side(Axis, bool),
    /// The two trade places.
    Middle,
}

/// What the drag in progress would do if let go now; stale once no drag is active.
#[derive(Default)]
pub(in crate::workbench) struct DropState {
    /// The pane a dragged pane is over and where on it it would land.
    pane: Option<(String, Zone)>,
    /// The tab a dragged pane rests on; it opens after `SPRING`.
    tab: Option<String>,
    /// The tab a dragged tab is over, and whether it would go after it.
    strip: Option<(String, bool)>,
    /// The tab being dragged along the strip, drawn faded in its old place.
    lifted: Option<String>,
    /// A dragged pane that could leave its tab is over 「＋」.
    new_tab: bool,
}

/// The zone of `bounds` that `at` falls in: the middle, else the side of the nearest edge.
pub(in crate::workbench) fn zone(bounds: Bounds<Pixels>, at: Point<Pixels>) -> Zone {
    let x = ((at.x - bounds.left()) / bounds.size.width).clamp(0., 1.);
    let y = ((at.y - bounds.top()) / bounds.size.height).clamp(0., 1.);
    if (x - 0.5).abs() < MIDDLE && (y - 0.5).abs() < MIDDLE {
        return Zone::Middle;
    }
    let edges = [
        (x, Zone::Side(Axis::Right, true)),
        (1. - x, Zone::Side(Axis::Right, false)),
        (y, Zone::Side(Axis::Down, true)),
        (1. - y, Zone::Side(Axis::Down, false)),
    ];
    edges
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(Zone::Middle, |(_, z)| z)
}

/// The tinted area showing where a dragged pane would land.
fn landing_mark(zone: Zone) -> gpui::Div {
    let half = relative(0.5);
    let mark = div().absolute().rounded(px(T::PANE_RADIUS));
    let mark = match zone {
        Zone::Middle => mark.inset(px(T::S20)),
        Zone::Side(Axis::Right, before) => {
            let d = mark.top_0().bottom_0().w(half);
            if before { d.left_0() } else { d.right_0() }
        }
        Zone::Side(Axis::Down, before) => {
            let d = mark.left_0().right_0().h(half);
            if before { d.top_0() } else { d.bottom_0() }
        }
    };
    mark.bg(T::color(T::accent()).opacity(0.14))
        .border(px(T::BORDER_WIDTH * 2.))
        .border_color(T::color(T::accent()))
}

impl Workbench {
    /// A pane's title bar: dragging it picks the pane up. Hovered it shows the open
    /// hand, so it reads as something to grab; while carried, the closed hand (GPUI
    /// keeps the bar's own cursor, not its hover one, for the whole drag).
    pub(super) fn pane_grip(&self, id: &str, title: &str, bar: gpui::Div) -> Stateful<gpui::Div> {
        let dragged = DraggedPane {
            id: id.to_owned(),
            title: title.to_owned(),
            grab: Point::default(),
        };
        bar.id(gpui::SharedString::from(format!("pane-grip-{id}")))
            .cursor(gpui::CursorStyle::ClosedHand)
            .hover(|s| s.cursor(gpui::CursorStyle::OpenHand))
            .on_drag(dragged, |p, grab, _, cx| {
                cx.new(|_| DraggedPane { grab, ..p.clone() })
            })
    }
    /// A pane takes dragged panes: it shows where one would land and puts it there.
    pub(super) fn pane_landing(
        &self,
        id: &str,
        pane: Stateful<gpui::Div>,
        cx: &Context<Self>,
    ) -> Stateful<gpui::Div> {
        let shown = (self.layout.drop.pane.as_ref())
            .filter(|(p, _)| p == id && cx.has_active_drag())
            .map(|(_, z)| *z);
        let (me, target) = (id.to_owned(), id.to_owned());
        pane.on_drag_move(
            cx.listener(move |this, e: &DragMoveEvent<DraggedPane>, _, cx| {
                let moving = &e.drag(cx).id;
                let at = (e.bounds.contains(&e.event.position) && *moving != me)
                    .then(|| this.landing_zone(moving, &me, zone(e.bounds, e.event.position)));
                let drop = &mut this.layout.drop.pane;
                let mine = drop.as_ref().is_some_and(|(p, _)| *p == me);
                match at {
                    Some(z) if !(mine && drop.as_ref().is_some_and(|(_, was)| *was == z)) => {
                        *drop = Some((me.clone(), z));
                        cx.notify();
                    }
                    None if mine => {
                        *drop = None;
                        cx.notify();
                    }
                    _ => {}
                }
            }),
        )
        .on_drop(cx.listener(move |this, p: &DraggedPane, _, cx| {
            cx.stop_propagation();
            let at = this.layout.drop.pane.take().filter(|(t, _)| *t == target);
            if let Some((_, z)) = at {
                this.drop_pane(&p.id, &target, z, cx);
            }
            cx.notify();
        }))
        .when_some(shown, |d, z| d.child(landing_mark(z)))
    }
    /// The middle of a pane from another tab has no place to trade with; the pane
    /// arrives on its right instead, and the mark says so.
    fn landing_zone(&self, moving: &str, target: &str, zone: Zone) -> Zone {
        let together = (self.workspace.terminal_tab(target))
            .and_then(|t| t.layout.as_ref())
            .is_some_and(|l| l.contains(moving));
        match zone {
            Zone::Middle if !together => Zone::Side(Axis::Right, false),
            z => z,
        }
    }
    fn drop_pane(&mut self, moving: &str, target: &str, zone: Zone, cx: &mut Context<Self>) {
        let Some(tab) = self.workspace.terminal_tab(target).cloned() else {
            return;
        };
        let side = match self.landing_zone(moving, target, zone) {
            Zone::Side(axis, before) => Some((axis, before)),
            Zone::Middle => None,
        };
        if (tab.layout.as_ref())
            .and_then(|l| l.dropped(moving, target, side))
            .is_none()
        {
            return;
        }
        // The host moves it in the layout as it is then; leaving another tab closes it
        // with its last pane.
        self.nav.zoomed = None;
        self.nav.tab = Some(tab.id.clone());
        self.nav.focused = Some(moving.to_owned());
        self.nav.pending_focus = true;
        let (terminal, target) = (moving.to_owned(), target.to_owned());
        self.edit(
            Edit::MovePane {
                terminal,
                target,
                side,
            },
            cx,
        );
    }
    /// 「＋」: a pane dropped on it leaves its tab for one of its own; a tab's only
    /// pane has nothing to leave and is not taken.
    pub(in crate::workbench) fn new_tab_landing(
        &self,
        plus: Stateful<gpui::Div>,
        cx: &Context<Self>,
    ) -> Stateful<gpui::Div> {
        let lit = self.layout.drop.new_tab && cx.has_active_drag();
        plus.when(lit, |d| d.bg(T::color(T::accent()).opacity(0.14)))
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<DraggedPane>, _, cx| {
                let over = e.bounds.contains(&e.event.position) && this.shares_tab(&e.drag(cx).id);
                if over != this.layout.drop.new_tab {
                    this.layout.drop.new_tab = over;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(|this, p: &DraggedPane, _, cx| {
                this.layout.drop.new_tab = false;
                if this.shares_tab(&p.id) {
                    this.move_pane(&p.id, None, cx);
                }
                cx.notify();
            }))
    }
    /// Whether the pane's tab holds other panes besides it.
    fn shares_tab(&self, pane: &str) -> bool {
        (self.workspace.terminal_tab(pane))
            .and_then(|t| t.layout.as_ref())
            .is_some_and(|l| l.ids().len() > 1)
    }
    /// A tab in the strip: dragging it moves it, and it takes dragged tabs (before or
    /// after it, by the half they are over) and dragged panes (they join its panes;
    /// resting on it opens it first, so a pane can be placed among them).
    pub(in crate::workbench) fn tab_landing(
        &self,
        tab: Stateful<gpui::Div>,
        dragged: super::super::tabs::DraggedTab,
        cx: &Context<Self>,
    ) -> Stateful<gpui::Div> {
        let id = dragged.id.clone();
        let drag = cx.has_active_drag();
        let drop = &self.layout.drop;
        let lifted = drag && drop.lifted.as_ref() == Some(&id);
        let mark = (drop.strip.as_ref())
            .filter(|(t, _)| drag && *t == id)
            .map(|(_, after)| *after);
        let resting = drag && drop.tab.as_ref() == Some(&id);
        let (on_tab, on_pane, take_tab, take_pane) = (id.clone(), id.clone(), id.clone(), id);
        tab.on_drag(dragged, |tab, _, _, cx| cx.new(|_| tab.clone()))
            .when(lifted, |d| d.opacity(0.4))
            .when_some(mark, |d, after| {
                let line = px(T::BORDER_WIDTH * 2.);
                (if after {
                    d.border_r(line)
                } else {
                    d.border_l(line)
                })
                .border_color(T::color(T::accent()))
            })
            .when(resting, |d| d.bg(T::color(T::accent()).opacity(0.14)))
            .on_drag_move(cx.listener(
                move |this, e: &DragMoveEvent<super::super::tabs::DraggedTab>, _, cx| {
                    let moving = &e.drag(cx).id;
                    let inside = e.bounds.contains(&e.event.position);
                    let after = e.event.position.x > e.bounds.center().x;
                    let drop = &mut this.layout.drop;
                    let mut changed = drop.lifted.as_ref() != Some(moving);
                    if changed {
                        drop.lifted = Some(moving.clone());
                    }
                    let now = (inside && *moving != on_tab).then(|| (on_tab.clone(), after));
                    let mine = drop.strip.as_ref().is_some_and(|(t, _)| *t == on_tab);
                    if (now.is_some() || mine) && drop.strip != now {
                        drop.strip = now;
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                },
            ))
            .on_drag_move(
                cx.listener(move |this, e: &DragMoveEvent<DraggedPane>, window, cx| {
                    let inside = e.bounds.contains(&e.event.position);
                    let mine = this.layout.drop.tab.as_ref() == Some(&on_pane);
                    if inside && !mine {
                        this.layout.drop.tab = Some(on_pane.clone());
                        this.spring_open(on_pane.clone(), window, cx);
                        cx.notify();
                    } else if !inside && mine {
                        this.layout.drop.tab = None;
                        cx.notify();
                    }
                }),
            )
            .on_drop(
                cx.listener(move |this, t: &super::super::tabs::DraggedTab, _, cx| {
                    let after = this.layout.drop.strip.take().is_some_and(|(_, a)| a);
                    this.layout.drop.lifted = None;
                    this.drop_tab(&t.id, &take_tab, after, cx);
                }),
            )
            .on_drop(cx.listener(move |this, p: &DraggedPane, _, cx| {
                cx.stop_propagation();
                this.layout.drop.tab = None;
                let home = this.workspace.terminal_tab(&p.id).map(|t| t.id.clone());
                if home.as_ref() != Some(&take_pane) {
                    this.move_pane(&p.id, Some(&take_pane), cx);
                }
                cx.notify();
            }))
    }
    /// After a pane has rested on a tab for `SPRING`, opens that tab, if the pane is
    /// still there and still being dragged.
    fn spring_open(&self, tab: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav.tab.as_ref() == Some(&tab) {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SPRING).await;
            this.update_in(cx, |this, window, cx| {
                if cx.has_active_drag() && this.layout.drop.tab.as_ref() == Some(&tab) {
                    this.show_tab(&tab, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }
    /// `moving` dropped on `target`'s left half goes before it, on its right half after it.
    fn drop_tab(&mut self, moving: &str, target: &str, after: bool, cx: &mut Context<Self>) {
        let before = if after {
            let tabs = self.sibling_tabs(target);
            let at = tabs.iter().position(|t| t == target);
            match at.and_then(|i| tabs.get(i + 1)) {
                Some(next) => Some(next.clone()),
                // Last in its space: before whatever follows it in the whole list.
                None => (self.workspace.tabs.iter())
                    .skip_while(|t| t.id != target)
                    .nth(1)
                    .map(|t| t.id.clone()),
            }
        } else {
            Some(target.to_owned())
        };
        if moving == target || before.as_deref() == Some(moving) {
            cx.notify();
            return;
        }
        self.edit(
            Edit::MoveTab {
                id: moving.to_owned(),
                before,
            },
            cx,
        );
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// The shown tab's layout, `a|b` side by side and `a/b` stacked, for evidence.
    pub fn visual_shown_layout(&self) -> String {
        fn text(l: &Layout) -> String {
            match l {
                Layout::Leaf(id) => id.clone(),
                Layout::Split {
                    axis,
                    first,
                    second,
                    ..
                } => {
                    let bar = if *axis == Axis::Right { "|" } else { "/" };
                    format!("({}{bar}{})", text(first), text(second))
                }
            }
        }
        let tab = self.nav.tab.as_ref();
        (self.workspace.tabs.iter().find(|t| Some(&t.id) == tab))
            .and_then(|t| t.layout.as_ref())
            .map_or_else(String::new, text)
    }
    /// The shown tab, the tab a dragged pane rests on, and whether 「＋」 is lit.
    pub fn visual_drop(&self) -> (Option<String>, Option<String>, bool) {
        let d = &self.layout.drop;
        (self.nav.tab.clone(), d.tab.clone(), d.new_tab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, size};

    #[test]
    fn a_pane_is_cut_by_the_nearest_edge_around_a_middle_that_swaps() {
        // A 400 × 200 pane whose corner sits at (100, 50).
        let p = Pixels::from;
        let b = Bounds::new(point(p(100.), p(50.)), size(p(400.), p(200.)));
        let at = |x: f32, y: f32| zone(b, point(p(100. + 400. * x), p(50. + 200. * y)));
        assert_eq!(at(0.5, 0.5), Zone::Middle);
        assert_eq!(at(0.65, 0.4), Zone::Middle);
        assert_eq!(at(0.05, 0.5), Zone::Side(Axis::Right, true));
        assert_eq!(at(0.95, 0.5), Zone::Side(Axis::Right, false));
        assert_eq!(at(0.5, 0.1), Zone::Side(Axis::Down, true));
        assert_eq!(at(0.5, 0.9), Zone::Side(Axis::Down, false));
        // Off-center towards a corner: whichever edge is nearer wins.
        assert_eq!(at(0.15, 0.05), Zone::Side(Axis::Down, true));
        assert_eq!(at(0.05, 0.15), Zone::Side(Axis::Right, true));
        // Past the edge (a move just outside) still names that edge.
        assert_eq!(at(1.2, 0.5), Zone::Side(Axis::Right, false));
    }
}
