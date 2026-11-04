//! The warm styles: a tired `- _ -` typed in the terminal's font that ends as `^ — ^`.
//! Nothing here is a drawing: the face is a string that changes one character at a time,
//! and what happens around it is characters too. Text sizes stay few on purpose, since
//! every new size is a new shape and new glyphs in the atlas.
use super::*;
use std::f32::consts::{PI, TAU};

/// The face's font size in a pane big enough for it, and for what is around it.
const FACE_SIZE: f32 = 56.;
const SIDE_SIZE: f32 = 22.;
/// The pane's short side at which the sizes are full, and the least they scale to.
const FULL_SIZE_SIDE: f32 = 420.;
const MIN_SCALE: f32 = 0.55;
/// How far above the pane's center the face sits, leaving the words below it.
const FACE_LIFT: f32 = 40.;
/// The sizes a face passes through on its way to the sidebar.
const SHRINK_STEPS: [f32; 5] = [1., 0.76, 0.56, 0.4, 0.28];
/// A row of the sunrise's horizon, cut to the pane.
const HORIZON: &str = "________________________________________________________________";
/// Characters a twinkle goes through.
const TWINKLE: [&str; 5] = [".", "+", "*", "+", "."];

/// When each face comes on, in order. The first is the tired one; the first with both
/// eyes up is the moment it turns happy.
fn beats(style: Style) -> &'static [(f32, &'static str)] {
    match style {
        Style::Relief => &[
            (0., " - _ - "),
            (0.14, " - _ -;"),
            (0.26, " - . -;"),
            (0.34, " - o - "),
            (0.44, " - _ - "),
            (0.48, " ^ _ - "),
            (0.52, " ^ — ^ "),
        ],
        Style::Wake => &[
            (0., "- _ -"),
            (0.4, "- _ o"),
            (0.48, "o _ o"),
            (0.54, "- _ -"),
            (0.57, "o _ o"),
            (0.62, "^ — ^"),
        ],
        Style::Sunrise => &[(0., "- _ -"), (0.42, "^ _ -"), (0.47, "^ — ^")],
        Style::Heart => &[(0., "- _ -"), (0.36, "o _ o"), (0.42, "^ — ^")],
        Style::Cat => &[
            (0., "=- . -="),
            (0.26, "=^ . -="),
            (0.33, "=- . ^="),
            (0.4, "=- . -="),
            (0.46, "=^ . ^="),
        ],
        Style::Tea => &[(0., "c[- _ -]"), (0.46, "c[^ _ -]"), (0.5, "c[^ — ^]")],
        _ => &[(0., "- _ -"), (0.14, "^ _ -"), (0.18, "^ — ^")],
    }
}
/// The face showing at `t`.
fn face_at(style: Style, t: f32) -> &'static str {
    beats(style)
        .iter()
        .rev()
        .find(|(from, _)| *from <= t)
        .map_or("", |(_, face)| face)
}
/// When the face has both eyes up.
fn turn(style: Style) -> f32 {
    beats(style)
        .iter()
        .find(|(_, face)| face.matches('^').count() == 2)
        .map_or(0., |(from, _)| *from)
}

/// One frame of a scene.
struct Scene {
    style: Style,
    t: f32,
    center: Point<Pixels>,
    /// The face's size and the size of what is around it.
    size: f32,
    side: f32,
    body: Bounds<Pixels>,
    target: Option<Point<Pixels>>,
}
fn mix(a: Hsla, b: Hsla, k: f32) -> Hsla {
    Hsla {
        h: lerp(a.h, b.h, k),
        s: lerp(a.s, b.s, k),
        l: lerp(a.l, b.l, k),
        a: lerp(a.a, b.a, k),
    }
}
/// A bounce that dies away over `x`.
fn bounce(x: f32, times: f32) -> f32 {
    (PI * times * x).sin().abs() * (1. - x)
}
/// The nearest of the few sizes on the way from `size` down, `k` of the way there.
fn shrunk(size: f32, k: f32) -> f32 {
    let last = SHRINK_STEPS.len() - 1;
    let step = (k.clamp(0., 1.) * last as f32).round() as usize;
    (size * SHRINK_STEPS[step]).round()
}

impl Painter<'_> {
    /// `text` in the terminal's font, centered on `at`.
    pub(super) fn text(
        &mut self,
        at: Point<Pixels>,
        text: impl Into<gpui::SharedString>,
        size: f32,
        color: Hsla,
    ) {
        if color.a <= 0.003 {
            return;
        }
        let text: gpui::SharedString = text.into();
        let run = gpui::TextRun {
            len: text.len(),
            font: self.mono.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = self
            .window
            .text_system()
            .shape_line(text, px(size), &[run], None);
        let height = px(size * 1.2);
        let origin = point(at.x - line.width / 2., at.y - height / 2.);
        let cx = &mut *self.cx;
        let mut paint = |window: &mut Window| {
            let _ = line.paint(origin, height, gpui::TextAlign::Left, None, window, cx);
        };
        match self.clip {
            Some(bounds) => self
                .window
                .with_content_mask(Some(gpui::ContentMask { bounds }), paint),
            None => paint(self.window),
        }
        self.quads += 1;
    }
    pub(super) fn cozy(
        &mut self,
        style: Style,
        t: f32,
        body: Bounds<Pixels>,
        target: Option<Point<Pixels>>,
    ) {
        let short = f32::from(body.size.width.min(body.size.height));
        let s = (short / FULL_SIZE_SIDE).clamp(MIN_SCALE, 1.);
        let scene = Scene {
            style,
            t,
            center: point(body.center().x, body.center().y - px(FACE_LIFT * s)),
            size: (FACE_SIZE * s).round(),
            side: (SIDE_SIZE * s).round(),
            body,
            target,
        };
        match style {
            Style::Relief => self.relief(&scene),
            Style::Wake => self.wake(&scene),
            Style::Sunrise => self.sunrise(&scene),
            Style::Heart => self.hearts(&scene),
            Style::Cat => self.cat(&scene),
            Style::Tea => self.tea(&scene),
            _ => self.hop(&scene),
        }
    }
    /// The face's color: the plain text color, warm from the moment it smiles.
    fn tint(&self, sc: &Scene) -> Hsla {
        if sc.t >= turn(sc.style) {
            self.warm
        } else {
            self.ink
        }
    }
    /// The face in its place, or on its way to the sidebar row, where it shrinks by steps
    /// and turns green; with no row in sight it stays and fades with the pane. `lift`
    /// moves it up, only while it has not left.
    fn face(&mut self, sc: &Scene, text: &'static str, lift: f32) {
        let tm = timing(sc.style);
        let k = sl(sc.t, tm.fade.0, tm.land);
        let shown = sl(sc.t, 0.05, 0.09);
        let tint = self.tint(sc);
        match sc.target {
            Some(_) if k >= 1. => {}
            Some(_) => {
                let (_, at) = self.flight_at(eio(k), sc.center, sc.target, (-120., 0.));
                let color = mix(tint, self.green, k);
                let at = point(at.x, at.y - px(lift * (1. - k)));
                self.text(at, text, shrunk(sc.size, eo(k)), color.opacity(shown));
            }
            None => {
                let fade = Workbench::completing_opacity(sc.style, sc.t);
                let at = point(sc.center.x, sc.center.y - px(lift));
                self.text(at, text, sc.size, tint.opacity(shown * fade));
            }
        }
    }
    /// What is around the face goes before it leaves.
    fn around(&self, sc: &Scene) -> f32 {
        let leaves = timing(sc.style).fade.0;
        1. - sl(sc.t, leaves - 0.03, leaves + 0.01)
    }
    /// A jump when the smile arrives.
    fn joy(&self, sc: &Scene) -> f32 {
        let from = turn(sc.style);
        bounce(sl(sc.t, from, from + 0.2), 2.) * sc.size * 0.6
    }
    /// Stars twinkling over and beside the face as `k` goes from 0 to 1; below it are the
    /// words.
    fn twinkles(&mut self, sc: &Scene, k: f32, seed: u32) {
        if k <= 0. || k >= 1. {
            return;
        }
        let mut rand = rng(seed);
        for i in 0..6 {
            let angle = -PI * (i as f32 + rand()) / 5.2 + 0.25;
            let far = 1.5 + rand() * 0.7;
            let own = sl(k, i as f32 * 0.08, 0.6 + i as f32 * 0.08);
            if own <= 0. || own >= 1. {
                continue;
            }
            let at = point(
                sc.center.x + px(angle.cos() * far * sc.size * 1.5),
                sc.center.y + px(angle.sin() * far * sc.size),
            );
            let star = TWINKLE[((own * TWINKLE.len() as f32) as usize).min(TWINKLE.len() - 1)];
            self.text(at, star, sc.side, self.warm.opacity((PI * own).sin()));
        }
    }

    /// A deep breath, the sigh going out as `~`, and the smile.
    fn relief(&mut self, sc: &Scene) {
        let t = sc.t;
        for i in 0..4 {
            let k = sl(t, 0.34 + i as f32 * 0.025, 0.54 + i as f32 * 0.025);
            if k > 0. && k < 1. {
                let at = point(
                    sc.center.x + px((1.7 + k * 1.6) * sc.size),
                    sc.center.y + px((0.2 - k * 0.5) * sc.size),
                );
                self.text(at, "~", sc.side, self.soft.opacity(1. - k));
            }
        }
        let lift = self.joy(sc);
        self.face(sc, face_at(sc.style, t), lift);
        let from = turn(sc.style);
        self.twinkles(sc, sl(t, from + 0.02, from + 0.26), 11);
    }

    /// Dozes under its z's, opens one eye and the other, blinks, and smiles.
    fn wake(&mut self, sc: &Scene) {
        let t = sc.t;
        for (i, z) in ["z", "z", "Z"].into_iter().enumerate() {
            let k = sl(t, 0.08 + i as f32 * 0.1, 0.34 + i as f32 * 0.1);
            if k > 0. && k < 1. {
                let at = point(
                    sc.center.x + px((1.5 + 1.1 * k) * sc.size),
                    sc.center.y - px((0.3 + 1.2 * k) * sc.size),
                );
                let a = (PI * k).sin() * (1. - sl(t, 0.38, 0.42));
                self.text(at, z, sc.side, self.soft.opacity(a));
            }
        }
        let startled = bounce(sl(t, 0.4, 0.48), 1.) * sc.size * 0.3;
        let lift = startled + self.joy(sc);
        self.face(sc, face_at(sc.style, t), lift);
        let from = turn(sc.style);
        self.twinkles(sc, sl(t, from + 0.02, from + 0.22), 23);
    }

    /// Comes up over a horizon of underscores; the sky warms and the rays open.
    fn sunrise(&mut self, sc: &Scene) {
        let t = sc.t;
        let tm = timing(sc.style);
        let shown = self.around(sc);
        let sky = eo(sl(t, 0.08, 0.5)) * shown;
        let horizon = sc.center.y + px(sc.size * 0.8);
        let top = sc.body.top();
        let above = Bounds::new(
            point(sc.body.left(), top),
            size(sc.body.size.width, horizon - top),
        );
        if sky > 0. {
            self.rect(
                above,
                gpui::linear_gradient(
                    0.,
                    gpui::linear_color_stop(self.warm.opacity(0.2 * sky), 0.),
                    gpui::linear_color_stop(self.warm.opacity(0.), 1.),
                ),
            );
            let cell = sc.side * 0.6;
            let room = (f32::from(sc.body.size.width) * 0.72 / cell) as usize;
            let row = &HORIZON[..room.min(HORIZON.len())];
            self.text(
                point(sc.center.x, horizon - px(sc.side * 0.3)),
                row,
                sc.side,
                self.warm.opacity(sky),
            );
        }
        let rise = eio(sl(t, 0.08, 0.46));
        let leaving = t >= tm.fade.0;
        self.clip = (!leaving).then_some(above);
        let rays = eo(sl(t, turn(sc.style), turn(sc.style) + 0.12)) * shown;
        if rays > 0. {
            for (i, ray) in ["-", "\\", "|", "/", "-", "\\", "|", "/"]
                .iter()
                .enumerate()
            {
                let a = i as f32 / 8. * TAU;
                let at = point(
                    sc.center.x + px(a.cos() * sc.size * 2.3),
                    sc.center.y + px(a.sin() * sc.size * 1.25),
                );
                let pulse = 0.65 + 0.35 * (t * 30. + i as f32).sin();
                self.text(at, *ray, sc.side, self.warm.opacity(rays * pulse));
            }
        }
        let below = lerp(sc.size * 2., 0., rise);
        self.face(sc, face_at(sc.style, t), -below);
        self.clip = None;
    }

    /// A `<3` floats up and touches the face; it smiles, and more of them rise around it.
    fn hearts(&mut self, sc: &Scene) {
        let t = sc.t;
        let k = sl(t, 0.14, 0.36);
        if k > 0. && k < 1. {
            let at = point(
                sc.center.x + px((k * 13.).sin() * 0.1 * sc.size),
                sc.center.y + px(lerp(2.2, 0.8, eio(k)) * sc.size),
            );
            self.text(at, "<3", sc.side, self.rose.opacity(sl(k, 0., 0.15)));
        }
        let mut rand = rng(5);
        for _ in 0..10 {
            let (x, sway, start) = (rand() * 2. - 1., rand() * TAU, 0.42 + rand() * 0.34);
            // Either side of the face, never over it.
            let side = x.signum() * (2.2 + x.abs() * 1.2);
            let k = sl(t, start, start + 0.28);
            if k > 0. && k < 1. {
                let at = point(
                    sc.center.x + px((side + (k * 6. + sway).sin() * 0.2) * sc.size),
                    sc.center.y + px((0.5 - 2.4 * k) * sc.size),
                );
                let a = (PI * k).sin().min(0.5) * 2.;
                self.text(at, "<3", sc.side, self.rose.opacity(a));
            }
        }
        let lift = self.joy(sc);
        self.face(sc, face_at(sc.style, t), lift);
    }

    /// `=- . -=` opens one eye and the other, smiles, waves, and leaves prints on its way.
    fn cat(&mut self, sc: &Scene) {
        let t = sc.t;
        let tm = timing(sc.style);
        if let Some(target) = sc.target {
            let n = 7;
            for i in 0..n {
                let along = (i as f32 + 1.) / (n as f32 + 1.);
                let start = lerp(tm.fade.0, tm.land, along);
                let shown = sl(t, start, start + 0.02) * (1. - sl(t, start + 0.03, start + 0.08));
                if shown > 0. {
                    let (_, spot) = self.flight_at(along, sc.center, Some(target), (-120., 0.));
                    let side = if i % 2 == 0 { -0.4 } else { 0.4 } * sc.side;
                    let at = point(spot.x, spot.y + px(side));
                    self.text(at, ":.", sc.side, self.warm.opacity(shown));
                }
            }
        }
        let wave = sl(t, turn(sc.style) + 0.06, turn(sc.style) + 0.1) * self.around(sc);
        if wave > 0. {
            let paw = if ((t * 16.) as u32).is_multiple_of(2) {
                "/"
            } else {
                "\\"
            };
            let at = point(
                sc.center.x + px(sc.size * 2.3),
                sc.center.y - px(sc.size * 0.1),
            );
            self.text(at, paw, sc.size, self.warm.opacity(wave));
        }
        let lift = self.joy(sc);
        self.face(sc, face_at(sc.style, t), lift);
    }

    /// `c[- _ -]` on its saucer with `~` rising; it warms up and a `<3` comes over the steam.
    fn tea(&mut self, sc: &Scene) {
        let t = sc.t;
        let shown = sl(t, 0.05, 0.09) * self.around(sc);
        let steam = sl(t, 0.12, 0.24) * shown;
        let rows = [("~ ~ ~", " ~ ~ ~"), (" ~ ~ ~", "~ ~ ~")];
        for (i, (even, odd)) in rows.into_iter().enumerate() {
            let at = point(
                sc.center.x,
                sc.center.y - px(sc.size * 0.85 + sc.side * 1.1 * i as f32),
            );
            let row = if ((t * 10.) as u32).is_multiple_of(2) {
                even
            } else {
                odd
            };
            self.text(at, row, sc.side, self.soft.opacity(steam));
        }
        let heart = back_out(sl(t, turn(sc.style) + 0.04, turn(sc.style) + 0.12)).clamp(0., 1.);
        self.text(
            point(
                sc.center.x,
                sc.center.y - px(sc.size * 0.85 + sc.side * 2.6),
            ),
            "<3",
            sc.side,
            self.rose.opacity(heart * shown),
        );
        self.text(
            point(sc.center.x, sc.center.y + px(sc.size * 0.72)),
            "'-------'",
            sc.side,
            self.soft.opacity(shown),
        );
        self.face(sc, face_at(sc.style, t), 0.);
    }

    /// Cheers up and hops all the way to its sidebar row, kicking up dots.
    fn hop(&mut self, sc: &Scene) {
        const HOPS: f32 = 5.;
        let t = sc.t;
        let tm = timing(sc.style);
        if t >= tm.land && sc.target.is_some() {
            return;
        }
        let end = sc
            .target
            .unwrap_or(point(sc.body.right() - px(sc.size), sc.center.y));
        let going = sl(t, 0.34, tm.land) * HOPS;
        let i = going.floor().min(HOPS - 1.);
        let f = (going - i).min(1.);
        let along = (i + eio(f)) / HOPS;
        let air = (PI * f).sin();
        let ground = point(
            px(lerp(f32::from(sc.center.x), f32::from(end.x), along)),
            px(lerp(f32::from(sc.center.y), f32::from(end.y), along)),
        );
        let fade = if sc.target.is_some() {
            1.
        } else {
            1. - sl(t, 0.76, 0.88)
        };
        if going > 0. && f < 0.45 {
            let k = f / 0.45;
            for side in [-1f32, 1.] {
                let at = point(
                    ground.x + px(side * sc.size * (0.8 + 0.9 * k)),
                    ground.y + px(sc.size * 0.4),
                );
                self.text(at, ".", sc.side, self.soft.opacity((1. - k) * fade));
            }
        }
        let high = air * sc.size * 1.5 * (1. - 0.13 * i);
        let shown = sl(t, 0.05, 0.09) * fade;
        let color = mix(self.tint(sc), self.green, sl(along, 0.7, 1.));
        let at = point(ground.x, ground.y - px(high + self.joy(sc)));
        self.text(
            at,
            face_at(sc.style, t),
            shrunk(sc.size, along),
            color.opacity(shown),
        );
        self.twinkles(sc, sl(t, 0.18, 0.36), 31);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FACES: [Style; 7] = [
        Style::Relief,
        Style::Wake,
        Style::Sunrise,
        Style::Heart,
        Style::Cat,
        Style::Tea,
        Style::Hop,
    ];

    #[test]
    fn every_face_starts_tired_and_is_smiling_before_the_words_leave() {
        for style in FACES {
            let start = face_at(style, 0.);
            assert_eq!(
                start.matches('-').count(),
                2,
                "{style:?} starts as - -: {start}"
            );
            assert!(!start.contains('^'), "{style:?} starts tired: {start}");
            let tm = timing(style);
            let later = face_at(style, tm.fade.0);
            assert_eq!(later.matches('^').count(), 2, "{style:?} ends ^ ^: {later}");
            assert!(turn(style) <= tm.words.1, "{style:?} smiles by the words");
            assert!(
                tm.fade.1 <= tm.land + 1e-5,
                "{style:?} lands after the fade"
            );
            assert!(style.greeting().is_some(), "{style:?} says something warm");
        }
    }

    #[test]
    fn a_face_keeps_its_width_so_it_does_not_jitter() {
        for style in FACES {
            let width = beats(style)[0].1.chars().count();
            for (from, face) in beats(style) {
                assert_eq!(face.chars().count(), width, "{style:?} at {from}: {face}");
            }
            assert!(
                beats(style).windows(2).all(|b| b[0].0 < b[1].0),
                "{style:?} in order"
            );
        }
    }

    #[test]
    fn waking_opens_one_eye_before_the_other() {
        assert_eq!(face_at(Style::Wake, 0.44), "- _ o");
        assert_eq!(face_at(Style::Wake, 0.5), "o _ o");
    }

    #[test]
    fn shrinking_uses_only_a_few_sizes() {
        let sizes: std::collections::BTreeSet<u32> = (0..=100)
            .map(|i| shrunk(FACE_SIZE, i as f32 / 100.) as u32)
            .collect();
        assert_eq!(sizes.len(), SHRINK_STEPS.len());
    }
}
