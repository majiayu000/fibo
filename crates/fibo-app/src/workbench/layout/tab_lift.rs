//! A tab carried along the strip (direction C, 11-17): in the strip it is the tab
//! itself, kept to its row while its neighbours make room; pulled out of the strip it
//! grows a sketch of its panes underneath, so you see which group you are carrying.
//! Let go anywhere over the tabs or panes and it lands in the gap.
use super::*;
use gpui::{App, Bounds, DragMoveEvent, Pixels, Point, Stateful, relative};
use std::{cell::RefCell, rc::Rc};

/// How far past the strip's edge (towards the panes) a tab counts as pulled out.
const PULL: f32 = T::S16;
/// The sketch of a pulled-out tab's panes.
const CARD_W: f32 = 260.;
const CARD_H: f32 = 156.;

/// What the carried tab is drawn from, shared between the workbench (which sees the
/// strip and the panes) and the drawing under the pointer (which sees only the pointer).
#[derive(Default)]
pub(in crate::workbench) struct Lift {
    /// The gap the tab left in the strip, where it is drawn while in the strip.
    gap: Option<Bounds<Pixels>>,
    /// The strip's visible part: the tab does not slide past its ends.
    strip: Option<Bounds<Pixels>>,
    /// Pulled out of the strip: drawn as the tab over a sketch of its panes.
    out: bool,
    sketch: Option<Rc<Sketch>>,
}

/// A tab's panes as boxes: each with its CLI's mark, its name and its state.
pub(in crate::workbench) enum Sketch {
    Pane {
        agent: Option<Agent>,
        title: String,
        status: Option<(Status, bool)>,
    },
    Split {
        axis: Axis,
        share: f32,
        first: Box<Sketch>,
        second: Box<Sketch>,
    },
}

/// A tab being dragged; GPUI draws it at the pointer less where it was picked up.
#[derive(Clone)]
pub(in crate::workbench) struct DraggedTab {
    pub(in crate::workbench) id: String,
    title: String,
    agent: Option<Agent>,
    terminal_style: bool,
    lift: Rc<RefCell<Lift>>,
    grab: Point<Pixels>,
}
impl gpui::Render for DraggedTab {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let lift = self.lift.borrow();
        let tab = tab_face(&self.title, self.agent, self.terminal_style);
        if lift.out {
            return div().child(card(tab, lift.sketch.as_deref()));
        }
        // In the strip: on the gap's row, sliding sideways with the pointer.
        let drawn = window.mouse_position() - self.grab;
        let Some(gap) = lift.gap else {
            return div().child(lifted(tab));
        };
        let width = gap.size.width;
        let left = lift.strip.map_or(drawn.x, |s| {
            let end = s.right() - width;
            let x = if drawn.x > end { end } else { drawn.x };
            if x < s.left() { s.left() } else { x }
        });
        div().child(
            lifted(tab)
                .w(width)
                .relative()
                .left(left - drawn.x)
                .top(gap.top() - drawn.y),
        )
    }
}

/// The tab's own face: its CLI's mark and its name.
fn tab_face(title: &str, agent: Option<Agent>, terminal_style: bool) -> gpui::Div {
    div()
        .h(px(if terminal_style { T::FOOTER } else { T::ROW }))
        .max_w(px(T::TAB_SHORT))
        .pl(px(T::S10))
        .pr(px(T::S12))
        .flex()
        .items_center()
        .gap(px(T::S6))
        .bg(T::color(T::raised()))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::border()))
        .text_size(px(T::MONO_SIZE))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(T::color(T::primary()))
        .when_some(agent.filter(|_| !terminal_style), |d, a| {
            d.child(super::super::board::agent_mark(a, T::color(T::secondary())))
        })
        .child(div().min_w_0().truncate().child(title.to_owned()))
}
fn shadow(blur: f32, drop: f32) -> Vec<gpui::BoxShadow> {
    vec![gpui::BoxShadow {
        color: gpui::black().opacity(0.24),
        offset: gpui::point(px(0.), px(drop)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }]
}
/// The tab raised off the strip.
fn lifted(tab: gpui::Div) -> gpui::Div {
    tab.rounded(px(T::CONTROL_RADIUS))
        .shadow(shadow(T::S16, T::S4))
}
/// The tab with its panes hanging under it, joined like a browser tab to its page.
fn card(tab: gpui::Div, sketch: Option<&Sketch>) -> gpui::Div {
    let window = div()
        .w(px(CARD_W))
        .h(px(CARD_H))
        .p(px(T::S6))
        .flex()
        .bg(T::surface(T::app()))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::border()))
        .rounded_tr(px(T::PANE_RADIUS))
        .rounded_b(px(T::PANE_RADIUS))
        .shadow(shadow(T::S20 + T::S12, T::S12))
        .children(sketch.map(sketch_view));
    div()
        .flex()
        .flex_col()
        .items_start()
        .child(
            tab.border_b_0()
                .rounded_t(px(T::CONTROL_RADIUS))
                .max_w(px(CARD_W))
                // Over the window's top line, so the two read as one piece.
                .mb(-px(T::BORDER_WIDTH))
                .relative(),
        )
        .child(window)
}
fn sketch_view(sketch: &Sketch) -> gpui::AnyElement {
    match sketch {
        Sketch::Pane {
            agent,
            title,
            status,
        } => {
            let line = |share: f32| {
                div()
                    .h(px(T::S4))
                    .w(relative(share))
                    .rounded_full()
                    .bg(T::color(T::border()))
            };
            div()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .p(px(T::S6))
                .flex()
                .flex_col()
                .gap(px(T::S4))
                .bg(T::surface(T::pane()))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .rounded(px(T::S4))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(T::S4))
                        .text_size(px(T::META))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(T::color(T::primary()))
                        .child(super::drag::mark(*agent, T::color(T::secondary())))
                        .child(div().flex_1().min_w_0().truncate().child(title.clone()))
                        .children(
                            status
                                .map(|(g, unseen)| crate::ui::widgets::status_mark_seen(g, unseen)),
                        ),
                )
                .child(line(0.8))
                .child(line(0.55))
                .child(line(0.68))
                .into_any_element()
        }
        Sketch::Split {
            axis,
            share,
            first,
            second,
        } => {
            let row = *axis == Axis::Right;
            let first = div()
                .flex_shrink_0()
                .min_w_0()
                .min_h_0()
                .map(|d| {
                    if row {
                        d.h_full().w(relative(*share))
                    } else {
                        d.w_full().h(relative(*share))
                    }
                })
                .child(sketch_view(first));
            let second = div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .child(sketch_view(second));
            div()
                .size_full()
                .min_w_0()
                .min_h_0()
                .flex()
                .when(!row, |d| d.flex_col())
                .gap(px(T::S4))
                .child(first)
                .child(second)
                .into_any_element()
        }
    }
}

/// A tab's mark, name or tail, kept in place but not drawn while the tab is carried.
pub(in crate::workbench) fn veil(carried: bool, element: impl IntoElement) -> gpui::AnyElement {
    if !carried {
        return element.into_any_element();
    }
    div()
        .flex()
        .flex_shrink_0()
        .invisible()
        .child(element)
        .into_any_element()
}

/// Where the gap goes as the pointer crosses `over`, a tab `left` of the gap or not:
/// past the middle of a tab towards the far side. Inside the half nearer the gap nothing
/// changes, so a tab narrower than the carried one does not flip back and forth.
pub(super) fn slot(over: Bounds<Pixels>, left: bool, x: Pixels) -> Option<bool> {
    let middle = over.center().x;
    match left {
        true if x < middle => Some(false),
        false if x > middle => Some(true),
        _ => None,
    }
}
/// The strip as it would be after the drop: `moving` put before or `after` `target`.
pub(super) fn previewed<'a>(
    ids: Vec<&'a str>,
    moving: &str,
    target: &str,
    after: bool,
) -> Vec<&'a str> {
    let Some(carried) = ids.iter().copied().find(|t| *t == moving) else {
        return ids;
    };
    let mut rest: Vec<&str> = ids.into_iter().filter(|t| *t != moving).collect();
    let Some(at) = rest.iter().position(|t| *t == target) else {
        rest.push(carried);
        return rest;
    };
    rest.insert(at + usize::from(after), carried);
    rest
}

impl Workbench {
    /// What a tab carries when dragged; its sketch is drawn once it starts to move.
    pub(in crate::workbench) fn dragged_tab(
        &self,
        id: &str,
        title: &str,
        agent: Option<Agent>,
        terminal_style: bool,
    ) -> DraggedTab {
        DraggedTab {
            id: id.to_owned(),
            title: title.to_owned(),
            agent,
            terminal_style,
            lift: Rc::default(),
            grab: Point::default(),
        }
    }
    /// This space's tabs in strip order, with a tab being dragged already in its gap.
    pub(in crate::workbench) fn strip_tabs(&self, cx: &App) -> Vec<&Tab> {
        let tabs: Vec<&Tab> = (self.workspace.tabs.iter())
            .filter(|t| t.space == self.nav.space)
            .collect();
        let drop = &self.layout.drop;
        let (Some(moving), Some((target, after)), true) =
            (&drop.lifted, &drop.strip, cx.has_active_drag())
        else {
            return tabs;
        };
        let ids = tabs.iter().map(|t| t.id.as_str()).collect();
        previewed(ids, moving, target, *after)
            .into_iter()
            .filter_map(|id| tabs.iter().copied().find(|t| t.id == id))
            .collect()
    }
    fn sketch(&self, layout: &Layout, cx: &App) -> Sketch {
        match layout {
            Layout::Leaf(id) => {
                let session = self.pane_session(id);
                let title = (session.map(|s| self.session_title(s)))
                    .or_else(|| self.layout.panes.get(id)?.read(cx).program_title())
                    .unwrap_or_else(|| t!("终端").into());
                Sketch::Pane {
                    agent: self.pane_agent(id),
                    title,
                    status: self.pane_status(id),
                }
            }
            Layout::Split {
                axis,
                ratio,
                first,
                second,
            } => Sketch::Split {
                axis: *axis,
                share: (f32::from(*ratio) / 10_000.).clamp(0.1, 0.9),
                first: Box::new(self.sketch(first, cx)),
                second: Box::new(self.sketch(second, cx)),
            },
        }
    }
    /// The visible strip: it tells when a tab is pulled out, and keeps one inside it.
    pub(in crate::workbench) fn strip_lift(
        &self,
        strip: gpui::Div,
        terminal_style: bool,
        cx: &Context<Self>,
    ) -> gpui::Div {
        strip.on_drag_move(
            cx.listener(move |this, e: &DragMoveEvent<DraggedTab>, _, cx| {
                let at = e.event.position.y;
                // The panes are under the standard strip and over the terminal style's.
                let out = if terminal_style {
                    at < e.bounds.top() - px(PULL)
                } else {
                    at > e.bounds.bottom() + px(PULL)
                };
                let mut lift = e.drag(cx).lift.borrow_mut();
                lift.strip = Some(e.bounds);
                lift.out = out;
                std::mem::drop(lift);
                if this.layout.drop.pulled != out {
                    this.layout.drop.pulled = out;
                    cx.notify();
                }
            }),
        )
    }
    /// The tab under a dragged tab: the carried one marks its gap, any other gives way.
    pub(super) fn tab_lift_move(
        &mut self,
        on_tab: &str,
        e: &DragMoveEvent<DraggedTab>,
        cx: &mut Context<Self>,
    ) {
        let dragged = e.drag(cx);
        let (moving, lift) = (dragged.id.clone(), dragged.lift.clone());
        let drop = &mut self.layout.drop;
        if *on_tab == moving {
            // Picked up: a fresh drag (its gap not yet seen), nothing left of the last
            // one, which may have been let go where nothing takes a tab.
            if lift.borrow().gap.is_none() {
                drop.lifted = Some(moving.clone());
                drop.strip = None;
                drop.pulled = false;
                cx.notify();
            }
            let mut lift = lift.borrow_mut();
            lift.gap = Some(e.bounds);
            if lift.sketch.is_none() {
                let tab = self.workspace.tabs.iter().find(|t| t.id == moving);
                lift.sketch =
                    (tab.and_then(|t| t.layout.as_ref())).map(|l| Rc::new(self.sketch(l, cx)));
            }
            return;
        }
        let x = e.event.position.x;
        if x < e.bounds.left() || x >= e.bounds.right() {
            return;
        }
        let gap = lift.borrow().gap;
        let left = gap.is_none_or(|g| e.bounds.left() < g.left());
        if let Some(after) = slot(e.bounds, left, x) {
            let now = Some((on_tab.to_owned(), after));
            if drop.strip != now {
                drop.strip = now;
                cx.notify();
            }
        }
    }
    /// Whether this tab is the one being carried: its place in the strip is the gap.
    pub(in crate::workbench) fn tab_carried(&self, id: &str, cx: &App) -> bool {
        cx.has_active_drag() && self.layout.drop.lifted.as_deref() == Some(id)
    }
    /// The carried tab's own place: kept (a hidden element would stop hearing the drag)
    /// but bare, its contents veiled; outlined once the tab is pulled out.
    pub(super) fn tab_gap(
        &self,
        tab: Stateful<gpui::Div>,
        id: &str,
        cx: &App,
    ) -> Stateful<gpui::Div> {
        if !self.tab_carried(id, cx) {
            return tab;
        }
        let none = gpui::transparent_black();
        let tab = tab.bg(none).border_color(none);
        if !self.layout.drop.pulled {
            return tab;
        }
        tab.border(px(T::BORDER_WIDTH * 1.5))
            .border_dashed()
            .border_color(T::color(T::accent()))
            .bg(T::color(T::accent()).opacity(0.1))
    }
    /// The tabs and panes take a dragged tab: it lands in its gap. Let go elsewhere
    /// (the sidebar, another window), it stays where it was.
    pub(in crate::workbench) fn tab_drop_area(
        &self,
        area: gpui::Div,
        cx: &Context<Self>,
    ) -> gpui::Div {
        area.on_drop(cx.listener(|this, t: &DraggedTab, _, cx| {
            let drop = &mut this.layout.drop;
            drop.lifted = None;
            drop.pulled = false;
            if let Some((target, after)) = drop.strip.take() {
                this.drop_tab(&t.id, &target, after, cx);
            }
            cx.notify();
        }))
    }
    /// Starts the drag with where the tab was picked up, so it is drawn where it was.
    pub(super) fn lift_tab(tab: &DraggedTab, grab: Point<Pixels>) -> DraggedTab {
        DraggedTab {
            grab,
            ..tab.clone()
        }
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// The carried tab's title, the strip's titles as shown, and whether it is pulled out.
    pub fn visual_tab_lift(&self, cx: &App) -> (Option<String>, Vec<String>, bool) {
        let drop = &self.layout.drop;
        let title =
            |id: &str| (self.workspace.tabs.iter().find(|t| t.id == id)).map(|t| t.title.clone());
        let lifted = drop.lifted.as_deref().and_then(title);
        let shown = self
            .strip_tabs(cx)
            .iter()
            .map(|t| t.title.clone())
            .collect();
        (lifted, shown, drop.pulled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, size};

    const P: fn(f32) -> Pixels = Pixels::from;

    fn tab(left: f32, width: f32) -> Bounds<Pixels> {
        Bounds::new(point(P(left), P(0.)), size(P(width), P(30.)))
    }

    #[test]
    fn the_gap_moves_only_past_the_middle_of_the_tab_beyond_it() {
        let wide = tab(100., 160.);
        // A tab right of the gap: its far half takes the gap after it.
        assert_eq!(slot(wide, false, P(150.)), None);
        assert_eq!(slot(wide, false, P(200.)), Some(true));
        // Left of the gap: its far (left) half takes the gap before it.
        assert_eq!(slot(wide, true, P(230.)), None);
        assert_eq!(slot(wide, true, P(150.)), Some(false));
    }

    #[test]
    fn a_narrow_tab_passed_by_a_wide_one_does_not_flip_back() {
        // Gap (wide, 160) at 0, narrow tab (60) at 160. Past its middle (190) the gap
        // goes after it: now the narrow tab sits at 0..60 and the gap at 60..220, so the
        // pointer at 190 is over the gap and the narrow tab says nothing.
        assert_eq!(slot(tab(160., 60.), false, P(191.)), Some(true));
        let moved = tab(0., 60.);
        assert_eq!(slot(moved, true, P(191.)), None);
    }

    #[test]
    fn the_strip_shows_the_carried_tab_in_its_gap() {
        let ids = vec!["a", "b", "c", "d"];
        assert_eq!(previewed(ids.clone(), "a", "c", true), ["b", "c", "a", "d"]);
        assert_eq!(
            previewed(ids.clone(), "d", "b", false),
            ["a", "d", "b", "c"]
        );
        // A target gone since (closed mid-drag): the tab goes last, nothing is lost.
        assert_eq!(
            previewed(ids.clone(), "a", "x", false),
            ["b", "c", "d", "a"]
        );
        assert_eq!(previewed(ids.clone(), "x", "a", false), ids);
    }
}
