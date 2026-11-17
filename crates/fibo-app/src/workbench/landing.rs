//! Where a jump landed. Among several panes the focused one's edge is only a step lighter
//! than the others', so after a sidebar row, ⌘J, a board card or search moved the keyboard
//! nobody could tell which pane it went to. The pane it landed on rings once in the accent
//! color and fades.
use super::*;
use std::time::Instant;

/// How long the ring shows, and the share of it at full strength before it fades.
const RING: Duration = Duration::from_millis(800);
const HOLD: f32 = 0.25;
/// The glow inside the ring and the wash over the pane, at full strength.
const GLOW: f32 = 0.4;
const WASH: f32 = 0.05;

/// The pane a jump last landed on, and when.
pub(super) struct Landing {
    pub(super) terminal: String,
    pub(super) at: Instant,
}

/// How strong the ring is `since` the jump: full, then easing out to nothing at `RING`;
/// `None` once it is over. Reduced motion keeps it full and takes it away in one step.
pub(super) fn strength(since: Duration, still: bool) -> Option<f32> {
    if since >= RING {
        return None;
    }
    if still {
        return Some(1.);
    }
    let t = since.as_secs_f32() / RING.as_secs_f32();
    let fade = ((t - HOLD) / (1. - HOLD)).clamp(0., 1.);
    Some(1. - fade * fade * (3. - 2. * fade))
}

/// Whether landing on one of `shown` panes rings it: alone, the pane is the whole main
/// area, and there is nothing to tell it from.
pub(super) fn rings(shown: usize) -> bool {
    shown > 1
}

/// How long ago every ring is held to have started, in visual tests; `None` runs the clock.
#[cfg(feature = "visual-test")]
fn held() -> Option<Duration> {
    *probe::HELD.lock().unwrap()
}
#[cfg(not(feature = "visual-test"))]
fn held() -> Option<Duration> {
    None
}

#[cfg(feature = "visual-test")]
pub(in crate::workbench) mod probe {
    use std::sync::Mutex;
    use std::time::Duration;
    /// The ring's age every frame shows while held, so two frames drawn apart match.
    pub(in crate::workbench) static HELD: Mutex<Option<Duration>> = Mutex::new(None);
}

impl Workbench {
    /// The panes shown now: the shown tab's, or the zoomed one alone.
    fn shown_leaves(&self) -> Vec<String> {
        if self.nav.overview {
            return Vec::new();
        }
        if let Some(zoomed) = &self.nav.zoomed {
            return vec![zoomed.clone()];
        }
        self.nav
            .tab
            .as_ref()
            .map(|t| self.workspace.tab_terminals(t))
            .unwrap_or_default()
    }
    /// A jump focused `terminal`: it rings if it shares the view with other panes.
    pub(super) fn ring_landing(&mut self, terminal: &str, cx: &mut Context<Self>) {
        let shown = self.shown_leaves();
        if !rings(shown.len()) || !shown.iter().any(|t| t == terminal) {
            self.layout.landing = None;
            return;
        }
        self.layout.landing = Some(Landing {
            terminal: terminal.to_owned(),
            at: Instant::now(),
        });
        // Held still, it asks for no frames while it shows; one when it goes.
        if fibo_core::effect::reduce_motion() {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(RING).await;
                let _ = this.update(cx, |this, cx| this.pane_changed(cx));
            })
            .detach();
        }
    }
    /// A launch replaced the pane a jump landed on (a lost terminal reopened in place):
    /// the new one rings in its place.
    pub(super) fn ring_landing_again(&mut self, old: &str, new: &str, cx: &mut Context<Self>) {
        if self
            .layout
            .landing
            .as_ref()
            .is_some_and(|l| l.terminal == old)
        {
            self.ring_landing(new, cx);
        }
    }
    /// The ring over pane `id` this frame. While it fades it asks for the next frame the
    /// way a pane's change does, so the sidebar keeps its draw.
    pub(super) fn landing_ring(
        &self,
        id: &str,
        terminal_style: bool,
        window: &Window,
    ) -> Option<gpui::Div> {
        let landing = self.layout.landing.as_ref().filter(|l| l.terminal == id)?;
        // 标记完成 plays over a pane it went to; that is the pane's whole show.
        if self.completing(id).is_some() {
            return None;
        }
        let still = fibo_core::effect::reduce_motion();
        let held = held();
        let s = strength(held.unwrap_or_else(|| landing.at.elapsed()), still);
        if held.is_none() && !still {
            self.next_pane_frame(window);
        }
        let s = s?;
        // The one solid control's color: the focus token is a mid gray on dark, and beside
        // a pane ringing in its state's color it did not read as where you went.
        let ink = T::color(T::accent());
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .top(px(if terminal_style {
                    T::PANE_HEADER / 2.
                } else {
                    0.
                }))
                .when(!terminal_style, |d| d.rounded(px(T::PANE_RADIUS)))
                .bg(ink.opacity(WASH * s))
                .border(px(T::FOCUS_RING))
                .border_color(ink.opacity(s))
                .shadow(vec![gpui::BoxShadow {
                    color: ink.opacity(GLOW * s),
                    offset: gpui::point(px(0.), px(0.)),
                    blur_radius: px(T::RING_GLOW),
                    spread_radius: px(0.),
                    inset: true,
                }]),
        )
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// Holds every ring at `ms` past its jump; `None` lets it run.
    pub fn visual_hold_landing(ms: Option<u64>) {
        *probe::HELD.lock().unwrap() = ms.map(Duration::from_millis);
    }
    /// The pane the last jump rang, while its ring shows.
    pub fn visual_landing(&self) -> Option<String> {
        let l = self.layout.landing.as_ref()?;
        strength(held().unwrap_or_else(|| l.at.elapsed()), false).map(|_| l.terminal.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_landing_rings_at_full_strength_then_fades_out_and_ends() {
        let at = |ms| strength(Duration::from_millis(ms), false);
        assert_eq!(at(0), Some(1.));
        assert_eq!(at(150), Some(1.), "held a beat before fading");
        let (a, b) = (at(400).unwrap(), at(700).unwrap());
        assert!(1. > a && a > b && b > 0., "fades: {a} then {b}");
        assert_eq!(at(800), None);
        assert_eq!(at(5000), None);
    }

    #[test]
    fn with_reduced_motion_it_holds_still_and_still_ends() {
        let at = |ms| strength(Duration::from_millis(ms), true);
        assert_eq!(at(0), Some(1.));
        assert_eq!(at(700), Some(1.));
        assert_eq!(at(800), None);
    }

    #[test]
    fn only_a_pane_among_others_rings() {
        assert!(!rings(0));
        assert!(!rings(1), "a pane alone is the whole main area");
        assert!(rings(2));
        assert!(rings(3));
    }
}
