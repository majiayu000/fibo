//! The tab strip as a cached view. Terminal output draws the workbench on every frame for
//! the title row and pane headers, and building the strip each time cost about 0.24 M
//! instructions a tab (fifteen tabs: half the frame). The strip reads no pane's own state,
//! so like the sidebar it draws again when the workbench notifies, and a catch-up covers
//! what changes by the clock.
use super::*;

pub(in crate::workbench) struct TabStripView {
    workbench: gpui::WeakEntity<Workbench>,
    drawn: Cell<Instant>,
    /// The workbench notified since the strip last drew, so it draws this frame.
    asked: Cell<bool>,
    _watch: gpui::Subscription,
}

impl TabStripView {
    pub(super) fn new(workbench: &Entity<Workbench>, cx: &mut Context<Workbench>) -> Entity<Self> {
        let weak = workbench.downgrade();
        cx.new(|cx| Self {
            workbench: weak,
            drawn: Cell::new(Instant::now()),
            asked: Cell::new(true),
            _watch: cx.observe(workbench, |view: &mut Self, _, cx| {
                view.asked.set(true);
                cx.notify();
            }),
        })
    }
    /// When it last drew, if this frame reuses that draw.
    pub(super) fn waits(&self) -> Option<Instant> {
        (!self.asked.get()).then(|| self.drawn.get())
    }
}

impl Render for TabStripView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drawn.set(Instant::now());
        self.asked.set(false);
        #[cfg(feature = "visual-test")]
        DRAWS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.workbench
            .update(cx, |w, cx| {
                if let Some(r) = &w.redraw {
                    r.marks.clear_tabs();
                }
                w.strip_row(cx)
            })
            .unwrap_or_else(|_| gpui::Empty.into_any_element())
    }
}

/// Draws the strip next frame, as a catch-up.
pub(super) fn ask(view: &Entity<TabStripView>, cx: &mut gpui::App) {
    view.update(cx, |v, cx| {
        v.asked.set(true);
        cx.notify();
    });
}

impl Workbench {
    /// The strip for this frame: its last draw unless the workbench notified. On Windows the
    /// system asks where the title bar's drag area is from the frame's hitboxes, which a
    /// reused draw does not bring back, so there it draws every frame.
    pub(in crate::workbench) fn cached_strip(
        &self,
        terminal_style: bool,
    ) -> Option<gpui::AnyElement> {
        let view = self.redraw.as_ref()?.strip.clone();
        fn place<S: Styled>(s: S, terminal_style: bool) -> S {
            match terminal_style {
                true => s.h(px(T::FOOTER)).flex_shrink_0(),
                false => s.flex_1().min_w_0().h_full(),
            }
        }
        Some(if fibo_platform::desktop::DRAWS_WINDOW_BUTTONS {
            place(div(), terminal_style).child(view).into_any_element()
        } else {
            view.cached(place(gpui::StyleRefinement::default(), terminal_style))
                .into_any_element()
        })
    }
    /// The window's own tabs are not drawn while another machine's main area stands here.
    pub(in crate::workbench) fn forget_tab_marks(&self) {
        if let Some(r) = &self.redraw {
            r.marks.clear_tabs();
        }
    }
}

#[cfg(feature = "visual-test")]
static DRAWS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(feature = "visual-test")]
impl Workbench {
    /// How many times the tab strip has drawn, to check that terminal output leaves it alone.
    pub fn visual_strip_draws() -> usize {
        DRAWS.load(std::sync::atomic::Ordering::Relaxed)
    }
}
