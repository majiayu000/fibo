//! The running mark turning, when 设置 › 外观 › 动效 asks for it (`ui::running_spin`).
//!
//! The sidebar is a cached view and costs about 105 M instructions a draw on a large
//! workspace, so the turning part lives outside it: a row leaves an empty box where its
//! mark goes and notes where that box landed, and a small view over the sidebar paints the
//! marks there, turned to the shared clock. A step draws the workbench (about 8 M) and
//! reuses the sidebar. With the setting off none of this exists in a frame.
use super::*;
use gpui::{Bounds, ContentMask, Pixels, TransformationMatrix};
use std::cell::RefCell;
use std::sync::atomic::AtomicBool;

/// One turn, in steps of about 10 degrees. At 100 ms a step, 30 degrees each, people saw it
/// stutter; whoever turns this on wants it smooth, and the setting says what that costs.
const TURN: Duration = Duration::from_millis(1200);
const STEP: Duration = Duration::from_millis(33);

struct Mark {
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    icon: &'static str,
    hue: gpui::Hsla,
}
impl Mark {
    /// Scrolled out of the list, or under a fold, it shows nothing.
    fn shown(&self) -> bool {
        !self.bounds.intersect(&self.mask.bounds).is_empty()
    }
}

/// Where the sidebar's last draw put its turning marks.
#[derive(Default)]
pub(super) struct Marks {
    rows: RefCell<Vec<Mark>>,
    /// The last paint showed marks: the view keeps stepping only while this holds, so a
    /// running row scrolled away costs nothing. Noted at paint, because a frame that draws
    /// the sidebar has no marks yet when views render.
    live: Cell<bool>,
}
impl Marks {
    pub(super) fn clear(&self) {
        self.rows.borrow_mut().clear();
    }
}

pub(super) struct Turning {
    pub(super) marks: Rc<Marks>,
}
impl Render for Turning {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        static PENDING: AtomicBool = AtomicBool::new(false);
        // A frame not drawn (a covered window) renders nothing, so this asks for no more.
        if self.marks.live.get() {
            #[cfg(feature = "visual-test")]
            ASKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            crate::workbench::pace::wake(&PENDING, STEP, cx);
        }
        let marks = self.marks.clone();
        let angle = crate::workbench::pace::phase(TURN) * std::f32::consts::TAU;
        gpui::canvas(
            |_, _, _| (),
            move |_, (), window, cx| {
                let rows = marks.rows.borrow();
                marks.live.set(rows.iter().any(Mark::shown));
                let scale = window.scale_factor();
                for mark in rows.iter().filter(|m| m.shown()) {
                    let center = mark.bounds.center();
                    let turned = TransformationMatrix::unit()
                        .translate(center.scale(scale))
                        .rotate(gpui::radians(angle))
                        .translate(center.scale(-scale));
                    window.with_content_mask(Some(mark.mask), |window| {
                        // An icon that fails to draw is no reason to stop the frame.
                        window
                            .paint_svg(mark.bounds, mark.icon.into(), None, turned, mark.hue, cx)
                            .ok();
                    });
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

fn turns(cx: &gpui::App) -> bool {
    crate::ui::running_spin::current(cx) && !mooring_core::effect::reduce_motion()
}

impl Workbench {
    /// An empty box for a running session's mark, which `Turning` paints. `None` when it
    /// stands still. Rows that turn also keep the clock's once-a-second redraw going, which
    /// does the catch-up's work while `turning_alone` holds it back.
    pub(in crate::workbench) fn turning_mark(
        &self,
        g: Status,
        icon: &'static str,
        size: f32,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if g != Status::Running || !turns(cx) {
            return None;
        }
        let marks = self.redraw.as_ref()?.marks.clone();
        self.tick_turn(cx);
        let hue = g.colors().0;
        let note = move |bounds, window: &mut Window, _: &mut gpui::App| {
            let mask = window.content_mask();
            marks.rows.borrow_mut().push(Mark {
                bounds,
                mask,
                icon,
                hue,
            });
        };
        Some(
            gpui::canvas(note, |_, (), _, _| {})
                .size(px(size))
                .flex_shrink_0()
                .into_any_element(),
        )
    }
    /// The list layouts' mark: its pale ring stands still and the arc on it turns.
    pub(in crate::workbench) fn session_mark(
        &self,
        g: Status,
        unseen: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        const SIZE: f32 = 10.;
        let Some(arc) = self.turning_mark(g, "icons/arc.svg", SIZE, cx) else {
            return crate::ui::widgets::status_mark_seen(g, unseen);
        };
        let ring = gpui::svg()
            .path("icons/ring.svg")
            .size(px(SIZE))
            .text_color(g.colors().0.opacity(0.25));
        div()
            .relative()
            .size(px(SIZE))
            .flex_shrink_0()
            .child(ring)
            .child(div().absolute().top_0().left_0().child(arc))
            .into_any_element()
    }
    /// The view that paints them, over the sidebar and under menus and the 标记完成 run.
    pub(in crate::workbench) fn turning_view(
        &self,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let on = turns(cx) && !self.sidebar.collapsed;
        self.redraw
            .as_ref()
            .filter(|_| on)
            .map(|r| r.turning.clone().into_any_element())
    }
    /// Marks are turning: every step is a frame beside the sidebar, and each would set a
    /// catch-up on top of the clock's redraw, so the sidebar drew up to twice a second.
    pub(super) fn turning_alone(&self, cx: &Context<Self>) -> bool {
        turns(cx) && self.redraw.as_ref().is_some_and(|r| r.marks.live.get())
    }
}

#[cfg(feature = "visual-test")]
static ASKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(feature = "visual-test")]
impl Workbench {
    /// One step of the marks, as the timer asks for it.
    pub fn visual_turning_step(&self, cx: &mut gpui::App) {
        if let Some(r) = &self.redraw {
            r.turning.update(cx, |_, cx| cx.notify());
        }
    }
    /// How many marks the last sidebar draw left to turn, on screen.
    pub fn visual_turning_rows(&self) -> usize {
        self.redraw.as_ref().map_or(0, |r| {
            r.marks.rows.borrow().iter().filter(|m| m.shown()).count()
        })
    }
    /// How many times the view has asked for its next step, to see it stop.
    pub fn visual_turning_asks() -> usize {
        ASKS.load(std::sync::atomic::Ordering::Relaxed)
    }
}
