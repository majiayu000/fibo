//! Easing, and which slice of the run each part of a style plays in.
use super::*;

/// `t`'s progress through `from..to` of the whole run, clamped to 0..=1.
pub(super) fn sl(t: f32, from: f32, to: f32) -> f32 {
    ((t - from) / (to - from)).clamp(0., 1.)
}
pub(super) fn eo(x: f32) -> f32 {
    1. - (1. - x).powi(4)
}
pub(super) fn eio(x: f32) -> f32 {
    if x < 0.5 {
        4. * x * x * x
    } else {
        1. - (-2. * x + 2.).powi(3) / 2.
    }
}
/// Overshoots a little before settling, like a stamp pressed down.
pub(super) fn back_out(x: f32) -> f32 {
    let (c1, c3) = (1.70158, 2.70158);
    1. + c3 * (x - 1.).powi(3) + c1 * (x - 1.).powi(2)
}
pub(super) fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}
pub(super) fn bez(a: Point<Pixels>, c: Point<Pixels>, b: Point<Pixels>, k: f32) -> Point<Pixels> {
    let m = 1. - k;
    point(
        a.x * (m * m) + c.x * (2. * m * k) + b.x * (k * k),
        a.y * (m * m) + c.y * (2. * m * k) + b.y * (k * k),
    )
}
/// A small deterministic random sequence, so the same `t` is always the same frame.
pub(super) fn rng(mut seed: u32) -> impl FnMut() -> f32 {
    move || {
        seed = seed.wrapping_add(0x6D2B_79F5);
        let mut t = (seed ^ (seed >> 15)).wrapping_mul(1 | seed);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f32) / 4_294_967_296.
    }
}

/// Per style, the slices of the run each part plays in.
pub(super) struct Timing {
    /// Where the button has filled and turned into ✓ 做完了 (`Status::Closed.label()`).
    pub(super) button: f32,
    /// The words in, and out (0 when they stay).
    pub(super) words: (f32, f32, f32, f32),
    /// Where the pane starts and finishes fading out.
    pub(super) fade: (f32, f32),
    /// Where the finished session reaches its sidebar row.
    pub(super) land: f32,
}
pub(super) fn timing(style: Style) -> Timing {
    match style {
        Style::Seal => Timing {
            button: 0.25,
            words: (0.3, 0.5, 0., 0.),
            fade: (0.85, 1.),
            land: 1.,
        },
        Style::Stamp => Timing {
            button: 0.14,
            words: (0.36, 0.52, 0.7, 0.78),
            fade: (0.74, 0.86),
            land: 0.88,
        },
        Style::Sweep => Timing {
            button: 0.12,
            words: (0.52, 0.64, 0.62, 0.7),
            fade: (0.88, 0.98),
            land: 0.9,
        },
        Style::Dissolve => Timing {
            button: 0.12,
            words: (0.3, 0.42, 0.66, 0.74),
            fade: (0.76, 0.9),
            land: 0.88,
        },
        Style::Fade => Timing {
            button: 0.12,
            words: (2., 2., 0., 0.),
            fade: (0.15, 1.),
            land: 2.,
        },
        Style::Moor => Timing {
            button: 0.1,
            words: (0.16, 0.24, 0.4, 0.44),
            fade: (0.42, 0.48),
            land: 0.87,
        },
        Style::Gravity => Timing {
            button: 0.12,
            words: (0.46, 0.54, 0.68, 0.74),
            fade: (0.78, 0.9),
            land: 0.86,
        },
        Style::Swirl => Timing {
            button: 0.12,
            words: (0.58, 0.66, 0.72, 0.78),
            fade: (0.76, 0.88),
            land: 0.88,
        },
        Style::Backspace => Timing {
            button: 0.12,
            words: (0.6, 0.66, 0.76, 0.8),
            fade: (0.8, 0.92),
            land: 0.9,
        },
        Style::Crt => Timing {
            button: 0.12,
            words: (0.42, 0.5, 0.6, 0.66),
            fade: (0.62, 0.74),
            land: 0.76,
        },
        Style::Confetti => Timing {
            button: 0.12,
            words: (0.3, 0.44, 0.72, 0.78),
            fade: (0.74, 0.86),
            land: 0.88,
        },
        Style::Relief => face_timing((0.54, 0.62), 0.82, 0.92),
        Style::Wake => face_timing((0.66, 0.72), 0.84, 0.93),
        Style::Sunrise => face_timing((0.54, 0.62), 0.82, 0.92),
        Style::Heart => face_timing((0.5, 0.58), 0.82, 0.92),
        Style::Cat => face_timing((0.54, 0.62), 0.82, 0.92),
        Style::Tea => face_timing((0.58, 0.66), 0.84, 0.93),
        Style::Hop => Timing {
            button: 0.08,
            words: (0.2, 0.3, 0.68, 0.74),
            fade: (0.72, 0.86),
            land: 0.88,
        },
    }
}
/// The face styles: the words come once the face is smiling and stay until it leaves.
fn face_timing(words: (f32, f32), leaves: f32, land: f32) -> Timing {
    Timing {
        button: 0.08,
        words: (words.0, words.1, leaves - 0.02, leaves + 0.02),
        fade: (leaves, land),
        land,
    }
}
