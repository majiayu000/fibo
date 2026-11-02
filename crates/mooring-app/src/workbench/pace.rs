//! Repeating motion drawn on a timer instead of on every display frame.
//!
//! A GPUI `with_animation(...repeat())` asks for the next frame on every vsync, and a frame
//! re-renders the whole workbench (about 1 ms on the panes, 2 ms on the board with 40
//! sessions), so a spinner left on screen kept the window drawing at 120 Hz: a quarter
//! of a core for an icon. Paced motion reads its phase from one shared clock and asks for
//! one redraw per step; the caller picks a step that the eye does not separate.
use gpui::Context;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Where a repeating motion of `period` is now, from 0 to 1. Every caller shares the
/// clock, so two spinners drawn in different frames stay in step.
pub(super) fn phase(period: Duration) -> f32 {
    let period = period.as_micros().max(1);
    (EPOCH.elapsed().as_micros() % period) as f32 / period as f32
}

/// Redraws the view once `every` has passed. Calls while one is pending are absorbed,
/// so any number of animated elements in one frame cost a single redraw per step; when
/// nothing calls it again (the motion left the screen) the redraws stop.
pub(super) fn wake<V: 'static>(pending: &'static AtomicBool, every: Duration, cx: &Context<V>) {
    if pending.swap(true, Ordering::Relaxed) {
        return;
    }
    cx.spawn(async move |this, cx| {
        cx.background_executor().timer(every).await;
        pending.store(false, Ordering::Relaxed);
        let _ = this.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_stays_within_one_period() {
        for period in [1, 80, 1600, 2400] {
            let t = phase(Duration::from_millis(period));
            assert!((0. ..1.).contains(&t), "{period} ms gave {t}");
        }
    }
}
