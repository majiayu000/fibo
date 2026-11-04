//! The face the warm styles draw, and the small shapes around it. What a face looks like
//! is a pure function of its mood, so `-_-` and the smile are one drawing with two values
//! rather than two pictures, and every frame between them exists.
use super::*;

/// Under this radius the features would be a smudge: only the round face is drawn.
const FEATURES_MIN_R: f32 = 8.;
/// Points along a curve; enough that an arc at the largest size shows no corners.
const CURVE_STEPS: usize = 12;
/// Points around a squashed face or a heart; few enough to stay cheap, no visible corners.
const OVAL_POINTS: usize = 56;

/// The expression: every value is 0 for `-_-`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Mood {
    /// Each eye from a flat dash to a happy arch.
    pub(super) happy: [f32; 2],
    /// Each eye from what `happy` says to wide open and round.
    pub(super) open: [f32; 2],
    /// The mouth from a flat `_` to a full smile; overshoots a little as it springs.
    pub(super) smile: f32,
    /// A smile's mouth opening into a laugh.
    pub(super) laugh: f32,
    /// A flat `_` turned into a cat's `ω`.
    pub(super) omega: f32,
    /// The cheeks coming up.
    pub(super) blush: f32,
}

/// A curve in the face's own units: the face's radius is 1, y grows down.
pub(super) type Curve = Vec<(f32, f32)>;

fn along(step: impl Fn(f32) -> (f32, f32)) -> Curve {
    (0..=CURVE_STEPS)
        .map(|i| step(i as f32 / CURVE_STEPS as f32))
        .collect()
}
/// 0 at both ends of `u`, 1 in the middle.
fn dome(u: f32) -> f32 {
    1. - (2. * u - 1.).powi(2)
}

impl Mood {
    /// Eye `side` (0 left, 1 right) as a line; an open eye's dot is in `eye_dot`.
    pub(super) fn eye(&self, side: usize) -> Curve {
        let x = if side == 0 { -0.38 } else { 0.38 };
        let width = 0.34 * (1. - self.open[side]);
        let rise = 0.24 * self.happy[side].clamp(0., 1.2);
        along(|u| (x + (u - 0.5) * width, -0.1 - rise * dome(u)))
    }
    /// The open eye's dot, as its center and radius.
    pub(super) fn eye_dot(&self, side: usize) -> Option<((f32, f32), f32)> {
        let x = if side == 0 { -0.38 } else { 0.38 };
        (self.open[side] > 0.02).then_some(((x, -0.14), 0.12 * self.open[side]))
    }
    /// The mouth's lower edge.
    pub(super) fn mouth(&self) -> Curve {
        let width = lerp(0.22, 0.6, self.smile) + 0.1 * self.laugh;
        let width = lerp(width, 0.44, self.omega);
        let sag = 0.24 * self.smile * (1. + 0.35 * self.laugh);
        let y = lerp(0.34, 0.26, self.smile.clamp(0., 1.));
        along(|u| {
            // A laugh's mouth is a round bowl; a smile's is a thin curve.
            let bowl = 1. - (2. * u - 1.).abs().powf(lerp(2., 3.4, self.laugh));
            let cat = 0.13 * (std::f32::consts::TAU * u).sin().abs();
            (
                (u - 0.5) * width,
                lerp(y + sag * bowl, 0.22 + cat, self.omega),
            )
        })
    }
}

/// A face being drawn at one moment.
pub(super) struct Face {
    pub(super) at: Point<Pixels>,
    pub(super) r: f32,
    /// Squash and stretch: the round shape's width and height against its radius.
    pub(super) squash: (f32, f32),
    /// Head tilt in radians.
    pub(super) tilt: f32,
    pub(super) mood: Mood,
    /// The round shape; None when the features sit on something else, like a cup.
    pub(super) skin: Option<Hsla>,
    pub(super) ink: Hsla,
    pub(super) alpha: f32,
}
impl Face {
    /// A point in the face's units on screen.
    pub(super) fn point(&self, x: f32, y: f32) -> Point<Pixels> {
        let (x, y) = (x * self.squash.0, y * self.squash.1);
        let (sin, cos) = self.tilt.sin_cos();
        point(
            self.at.x + px((x * cos - y * sin) * self.r),
            self.at.y + px((x * sin + y * cos) * self.r),
        )
    }
}

impl Painter<'_> {
    /// A filled shape.
    pub(super) fn shape(&mut self, points: &[Point<Pixels>], color: Hsla) {
        if points.len() < 3 || color.a <= 0.003 {
            return;
        }
        let mut path = PathBuilder::fill();
        path.move_to(points[0]);
        for &p in &points[1..] {
            path.line_to(p);
        }
        path.close();
        if let Ok(path) = path.build() {
            self.paths += 1;
            self.window.paint_path(path, color);
        }
    }
    /// A rounded box around `at`: cheeks, a saucer, a shadow.
    pub(super) fn pill(&mut self, at: Point<Pixels>, w: f32, h: f32, color: Hsla) {
        if w <= 0. || h <= 0. || color.a <= 0.003 {
            return;
        }
        self.quads += 1;
        self.window.paint_quad(gpui::quad(
            Bounds::new(
                point(at.x - px(w / 2.), at.y - px(h / 2.)),
                size(px(w), px(h)),
            ),
            px(w.min(h) / 2.),
            color,
            px(0.),
            gpui::transparent_black(),
            Default::default(),
        ));
    }
    /// A line with round ends, in the face's units.
    fn curve(&mut self, f: &Face, curve: &[(f32, f32)], width: f32, color: Hsla) {
        let points: Vec<_> = curve.iter().map(|&(x, y)| f.point(x, y)).collect();
        self.stroke(&points, width, color);
        if let (Some(&a), Some(&b)) = (points.first(), points.last()) {
            self.disc(a, width / 2., color);
            self.disc(b, width / 2., color);
        }
    }
    pub(super) fn face(&mut self, f: &Face) {
        if let Some(skin) = f.skin {
            if f.squash == (1., 1.) {
                self.disc(f.at, f.r, skin.opacity(f.alpha));
            } else {
                // A quad's corners would make a squashed face a capsule.
                let oval: Vec<_> = (0..OVAL_POINTS)
                    .map(|i| {
                        let a = i as f32 / OVAL_POINTS as f32 * std::f32::consts::TAU;
                        f.point(a.cos(), a.sin())
                    })
                    .collect();
                self.shape(&oval, skin.opacity(f.alpha));
            }
        }
        if f.r < FEATURES_MIN_R {
            return;
        }
        let ink = f.ink.opacity(f.alpha);
        let width = (f.r * 0.085).max(1.2);
        let cheeks = self
            .rose
            .opacity(0.42 * f.mood.blush.clamp(0., 1.) * f.alpha);
        for x in [-0.62, 0.62] {
            self.pill(f.point(x, 0.18), f.r * 0.36, f.r * 0.22, cheeks);
        }
        for side in 0..2 {
            let eye = f.mood.eye(side);
            if f.mood.open[side] < 0.92 {
                self.curve(f, &eye, width, ink);
            }
            if let Some(((x, y), r)) = f.mood.eye_dot(side) {
                self.disc(f.point(x, y), r * f.r, ink);
            }
        }
        let mouth = f.mood.mouth();
        if f.mood.laugh > 0.02 {
            let points: Vec<_> = mouth.iter().map(|&(x, y)| f.point(x, y)).collect();
            self.shape(&points, ink);
        }
        self.curve(f, &mouth, width, ink);
    }
    /// A heart `size` wide around `at`.
    pub(super) fn heart(&mut self, at: Point<Pixels>, size: f32, color: Hsla) {
        if size <= 0.5 {
            return;
        }
        let points: Vec<_> = (0..OVAL_POINTS)
            .map(|i| {
                let a = i as f32 / OVAL_POINTS as f32 * std::f32::consts::TAU;
                let x = 16. * a.sin().powi(3);
                let y = 13. * a.cos() - 5. * (2. * a).cos() - 2. * (3. * a).cos() - (4. * a).cos();
                point(at.x + px(x / 32. * size), at.y - px(y / 32. * size))
            })
            .collect();
        self.shape(&points, color);
    }
    /// A four-pointed twinkle.
    pub(super) fn sparkle(&mut self, at: Point<Pixels>, size: f32, color: Hsla) {
        if size <= 0.5 {
            return;
        }
        let points: Vec<_> = (0..8)
            .map(|i| {
                let a = i as f32 / 8. * std::f32::consts::TAU;
                let r = if i % 2 == 0 { size } else { size * 0.22 };
                point(at.x + px(r * a.sin()), at.y - px(r * a.cos()))
            })
            .collect();
        self.shape(&points, color);
    }
    /// A sweat drop hanging from `at`.
    pub(super) fn drop_of_sweat(&mut self, at: Point<Pixels>, size: f32, color: Hsla) {
        if size <= 0.5 {
            return;
        }
        let points: Vec<_> = (0..16)
            .map(|i| {
                let a = i as f32 / 16. * std::f32::consts::TAU;
                let narrow = 0.5 * (1. - a.cos()) * size * 0.5;
                point(
                    at.x + px(narrow * a.sin()),
                    at.y + px(size * (0.5 - 0.5 * a.cos())),
                )
            })
            .collect();
        self.shape(&points, color);
    }
    /// A sleeper's Z.
    pub(super) fn z(&mut self, at: Point<Pixels>, size: f32, width: f32, color: Hsla) {
        let half = size / 2.;
        let corner = |x: f32, y: f32| point(at.x + px(x * half), at.y + px(y * half));
        self.stroke(
            &[
                corner(-1., -1.),
                corner(1., -1.),
                corner(-1., 1.),
                corner(1., 1.),
            ],
            width,
            color,
        );
    }
    /// A paw print with its pad at `at`.
    pub(super) fn paw(&mut self, at: Point<Pixels>, size: f32, color: Hsla) {
        self.pill(at, size, size * 0.82, color);
        for (x, y) in [(-0.56, -0.5), (0., -0.78), (0.56, -0.5)] {
            self.disc(
                point(at.x + px(x * size), at.y + px(y * size)),
                size * 0.2,
                color,
            );
        }
    }
    /// Twinkles popping one after another around `at` as `k` goes from 0 to 1.
    pub(super) fn twinkles(&mut self, at: Point<Pixels>, r: f32, k: f32, seed: u32, color: Hsla) {
        if k <= 0. || k >= 1. {
            return;
        }
        let mut rand = rng(seed);
        for i in 0..6 {
            // Only over the face and beside it: below it are the words.
            let angle = -std::f32::consts::PI * (i as f32 + rand()) / 5.2 + 0.25;
            let (far, big) = (1.35 + rand() * 0.55, 0.14 + rand() * 0.14);
            let own = sl(k, i as f32 * 0.08, 0.6 + i as f32 * 0.08);
            let spot = point(
                at.x + px(angle.cos() * far * r),
                at.y + px(angle.sin() * far * r),
            );
            let grow = (std::f32::consts::PI * own).sin();
            self.sparkle(spot, big * r * grow, color.opacity(grow));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(curve: &[(f32, f32)]) -> bool {
        curve.iter().all(|p| (p.1 - curve[0].1).abs() < 1e-5)
    }

    #[test]
    fn a_face_with_no_mood_is_the_tired_one() {
        let tired = Mood::default();
        assert!(flat(&tired.eye(0)) && flat(&tired.eye(1)), "- -");
        assert!(flat(&tired.mouth()), "_");
        assert!(tired.eye_dot(0).is_none());
    }

    #[test]
    fn a_happy_face_arches_its_eyes_and_lifts_its_mouth_corners() {
        let happy = Mood {
            happy: [1., 1.],
            smile: 1.,
            ..Mood::default()
        };
        let eye = happy.eye(1);
        let top = eye.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        assert!(
            top < eye[0].1 - 0.2,
            "the eye's middle rises above its ends"
        );
        let mouth = happy.mouth();
        let low = mouth.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        assert!(
            low > mouth[0].1 + 0.2,
            "the mouth's middle drops below its corners"
        );
    }
}
