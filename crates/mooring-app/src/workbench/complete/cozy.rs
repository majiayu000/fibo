//! The warm styles: a tired `-_-` that ends up smiling. `mood` says what the face feels
//! at `t` for every style; the scenes below say what happens around it.
use super::face::{Face, Mood};
use super::*;
use std::f32::consts::{PI, TAU};

/// The face's radius in a pane big enough for it; smaller panes scale it down.
const FACE_R: f32 = 56.;
/// The pane's short side at which the face reaches `FACE_R`, and the least it shrinks to.
const FULL_SIZE_SIDE: f32 = 420.;
const MIN_SCALE: f32 = 0.55;
/// How far above the pane's center the face sits, leaving the words below it.
const FACE_LIFT: f32 = 52.;
/// The radius it arrives in the sidebar with, the row's dot.
const LANDED_R: f32 = 3.5;

/// Where each style's face has turned from tired to smiling.
fn smile_span(style: Style) -> (f32, f32) {
    match style {
        Style::Relief => (0.44, 0.56),
        Style::Wake => (0.6, 0.7),
        Style::Sunrise => (0.42, 0.54),
        Style::Heart => (0.38, 0.5),
        Style::Cat => (0.42, 0.54),
        Style::Tea => (0.46, 0.58),
        _ => (0.16, 0.26),
    }
}

/// What the face feels at `t`: `-_-` until its turn comes, then a smile that springs.
pub(super) fn mood(style: Style, t: f32) -> Mood {
    let (from, to) = smile_span(style);
    let turn = sl(t, from, to);
    let happy = eo(turn);
    let mut mood = Mood {
        happy: [happy, happy],
        open: [0., 0.],
        smile: back_out(turn),
        laugh: 0.,
        omega: 0.,
        blush: eo(sl(t, from + 0.03, to + 0.04)),
    };
    match style {
        Style::Wake => {
            // One eye, then the other; a blink; then both close into the smile.
            let blink = (PI * sl(t, 0.54, 0.58)).sin();
            let leave = 1. - turn;
            mood.open = [
                eo(sl(t, 0.48, 0.52)) * (1. - blink) * leave,
                back_out(sl(t, 0.38, 0.43)) * (1. - blink) * leave,
            ];
        }
        Style::Cat => mood.omega = 1. - 0.25 * turn,
        Style::Heart => mood.laugh = eo(sl(t, 0.46, 0.56)),
        Style::Hop => mood.laugh = eo(sl(t, 0.22, 0.3)),
        _ => {}
    }
    mood
}

/// One frame of a scene.
struct Scene {
    style: Style,
    t: f32,
    /// Where the face sits, and its size.
    center: Point<Pixels>,
    r: f32,
    /// The face's scale against the full size, for whatever is drawn around it.
    s: f32,
    body: Bounds<Pixels>,
    target: Option<Point<Pixels>>,
    skin: Hsla,
    ink: Hsla,
}
/// Where the face is on its way out.
struct Leaving {
    at: Point<Pixels>,
    r: f32,
    alpha: f32,
    /// From its own color to the finished green.
    green: f32,
    gone: bool,
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

impl Painter<'_> {
    pub(super) fn cozy(
        &mut self,
        style: Style,
        t: f32,
        body: Bounds<Pixels>,
        target: Option<Point<Pixels>>,
    ) {
        let side = f32::from(body.size.width.min(body.size.height));
        let s = (side / FULL_SIZE_SIDE).clamp(MIN_SCALE, 1.);
        let scene = Scene {
            style,
            t,
            center: point(body.center().x, body.center().y - px(FACE_LIFT * s)),
            r: FACE_R * s,
            s,
            body,
            target,
            skin: Hsla {
                s: self.warm.s.max(0.7),
                l: 0.7,
                a: 1.,
                ..self.warm
            },
            ink: Hsla {
                h: self.warm.h,
                s: 0.5,
                l: 0.17,
                a: 1.,
            },
        };
        match style {
            Style::Relief => self.relief(&scene),
            Style::Wake => self.wake(&scene),
            Style::Sunrise => self.sunrise(&scene),
            Style::Heart => self.big_heart(&scene),
            Style::Cat => self.cat(&scene),
            Style::Tea => self.tea(&scene),
            _ => self.hop(&scene),
        }
    }
    /// The face's way out: to the sidebar row, shrinking and turning green; with no row
    /// in sight it stays and fades with the pane.
    fn leaving(&self, sc: &Scene) -> Leaving {
        let tm = timing(sc.style);
        let k = sl(sc.t, tm.fade.0, tm.land);
        let fade = Workbench::completing_opacity(sc.style, sc.t);
        match sc.target {
            Some(_) => {
                let (_, at) = self.flight_at(eio(k), sc.center, sc.target, (-120., 0.));
                Leaving {
                    at,
                    r: lerp(sc.r, LANDED_R, eo(k)),
                    alpha: 1.,
                    green: k,
                    gone: k >= 1.,
                }
            }
            None => Leaving {
                at: sc.center,
                r: sc.r,
                alpha: fade,
                green: 0.,
                gone: false,
            },
        }
    }
    /// A soft warm light behind the face.
    fn halo(&mut self, at: Point<Pixels>, r: f32, strength: f32, color: Hsla) {
        if strength <= 0. {
            return;
        }
        for (grow, a) in [(30., 0.05), (20., 0.08), (11., 0.12)] {
            self.disc(at, r + grow * strength, color.opacity(a * strength));
        }
    }
    /// How the plain face is drawn in most scenes, popping in at the start.
    fn plain(&mut self, sc: &Scene, leave: &Leaving, squash: (f32, f32), tilt: f32, lift: f32) {
        let pop = back_out(sl(sc.t, 0.06, 0.16)).max(0.);
        let skin = mix(sc.skin, self.green, leave.green);
        self.face(&Face {
            at: point(leave.at.x, leave.at.y - px(lift)),
            r: leave.r * pop,
            squash,
            tilt,
            mood: mood(sc.style, sc.t),
            skin: Some(skin),
            ink: sc.ink,
            alpha: sl(sc.t, 0.06, 0.1) * leave.alpha,
        });
    }

    /// Takes a deep breath, sighs it out with a sweat drop falling, and smiles.
    fn relief(&mut self, sc: &Scene) {
        let t = sc.t;
        let leave = self.leaving(sc);
        if leave.gone {
            return;
        }
        let breath = eio(sl(t, 0.16, 0.32)) - eo(sl(t, 0.32, 0.44));
        let droop = eo(sl(t, 0.32, 0.4)) * (1. - eio(sl(t, 0.44, 0.54)));
        let jump = bounce(sl(t, 0.46, 0.68), 2.) * sc.r * 0.55;
        let squash = (
            1. + 0.05 * breath + 0.08 * droop,
            1. + 0.1 * breath - 0.11 * droop,
        );
        let glow = eo(sl(t, 0.46, 0.6)) * leave.alpha * (1. - leave.green);
        self.halo(leave.at, leave.r, glow, sc.skin);
        // The sigh.
        for i in 0..4 {
            let k = sl(t, 0.33 + i as f32 * 0.025, 0.54 + i as f32 * 0.025);
            if k > 0. && k < 1. {
                let at = point(
                    sc.center.x - px((0.12 + k * 1.75) * sc.r),
                    sc.center.y + px((0.42 + k * 0.5 - k * k * 0.42) * sc.r),
                );
                let size = (0.08 + 0.17 * k) * sc.r;
                self.disc(at, size, self.soft.opacity(0.42 * (1. - k)));
            }
        }
        self.plain(sc, &leave, squash, 0., jump);
        // The sweat drop slides down the temple, then lets go with the sigh.
        let slide = sl(t, 0.1, 0.3);
        let fall = sl(t, 0.34, 0.5);
        let drop = point(
            sc.center.x + px(sc.r * 0.98),
            sc.center.y + px((-0.86 + 0.34 * slide + 1.6 * fall * fall) * sc.r),
        );
        self.drop_of_sweat(
            drop,
            0.36 * sc.r * sl(t, 0.08, 0.14),
            gpui::hsla(0.56, 0.65, 0.68, 1. - sl(fall, 0.5, 1.)),
        );
        self.twinkles(sc.center, sc.r, sl(t, 0.48, 0.74), 11, sc.skin);
    }

    /// Dozes under its Z's, opens one eye and the other, stretches, and smiles.
    fn wake(&mut self, sc: &Scene) {
        let t = sc.t;
        let leave = self.leaving(sc);
        if leave.gone {
            return;
        }
        let asleep = 1. - sl(t, 0.36, 0.4);
        for i in 0..3 {
            let k = sl(t, 0.08 + i as f32 * 0.1, 0.34 + i as f32 * 0.1);
            if k > 0. && k < 1. {
                let at = point(
                    sc.center.x + px((0.8 + 1.15 * k) * sc.r + (k * 9.).sin() * 4. * sc.s),
                    sc.center.y - px((0.7 + 1.55 * k) * sc.r),
                );
                let size = (0.18 + 0.26 * k) * sc.r;
                let a = (PI * k).sin() * (1. - sl(t, 0.38, 0.44));
                self.z(at, size, (size * 0.14).max(1.3), self.soft.opacity(a));
            }
        }
        let nod = (t * 22.).sin() * 0.07 * asleep;
        let breath = (t * 26.).sin() * 0.025 * asleep;
        let startled = bounce(sl(t, 0.38, 0.46), 1.) * sc.r * 0.3;
        let reach = (PI * sl(t, 0.58, 0.74)).sin();
        let joy = bounce(sl(t, 0.68, 0.84), 2.) * sc.r * 0.32;
        let squash = (1. + breath - 0.08 * reach, 1. - breath + 0.14 * reach);
        let glow = eo(sl(t, 0.6, 0.72)) * leave.alpha * (1. - leave.green);
        self.halo(leave.at, leave.r, glow, sc.skin);
        self.plain(sc, &leave, squash, nod + 0.05 * asleep, startled + joy);
        self.twinkles(sc.center, sc.r, sl(t, 0.62, 0.84), 23, sc.skin);
    }

    /// Comes up over the horizon like the sun; the sky warms and the rays open.
    fn sunrise(&mut self, sc: &Scene) {
        let t = sc.t;
        let tm = timing(sc.style);
        let leave = self.leaving(sc);
        let sky = eo(sl(t, 0.08, 0.5)) * (1. - sl(t, tm.fade.0, tm.fade.0 + 0.06));
        let horizon = sc.center.y + px(sc.r * 1.25);
        let top = sc.body.top();
        if sky > 0. {
            self.rect(
                Bounds::new(
                    point(sc.body.left(), top),
                    size(sc.body.size.width, horizon - top),
                ),
                gpui::linear_gradient(
                    0.,
                    gpui::linear_color_stop(sc.skin.opacity(0.3 * sky), 0.),
                    gpui::linear_color_stop(sc.skin.opacity(0.), 1.),
                ),
            );
            let line = f32::from(sc.body.size.width) * 0.72 * eo(sl(t, 0.06, 0.28));
            self.pill(
                point(sc.center.x, horizon),
                line,
                2.,
                sc.skin.opacity(0.75 * sky),
            );
        }
        if leave.gone {
            return;
        }
        let rise = eio(sl(t, 0.08, 0.5));
        let up = lerp(sc.r * 2.5, 0., rise);
        let clip = Bounds::new(
            point(sc.body.left(), top),
            size(sc.body.size.width, horizon - top),
        );
        let rays = eo(sl(t, 0.42, 0.58)) * (1. - sl(t, tm.fade.0 - 0.02, tm.fade.0 + 0.03));
        let leaving = t >= tm.fade.0 && sc.target.is_some();
        let bounds = if leaving {
            Bounds::new(point(px(0.), px(0.)), self.window.viewport_size())
        } else {
            clip
        };
        let at = point(leave.at.x, leave.at.y + px(up));
        let scene_mood = mood(sc.style, t);
        let skin = mix(sc.skin, self.green, leave.green);
        let (r, alpha, ink) = (leave.r, leave.alpha, sc.ink);
        self.clipped(bounds, |p| {
            p.halo(at, r, rise * alpha * (1. - leave.green) * 1.3, skin);
            for i in 0..12 {
                let a = i as f32 / 12. * TAU + t * 1.6;
                let (from, to) = (r * 1.22, r * (1.22 + 0.34 * rays));
                let wave = if i % 2 == 0 { 1. } else { 0.68 };
                p.stroke(
                    &[
                        point(at.x + px(a.cos() * from), at.y + px(a.sin() * from)),
                        point(
                            at.x + px(a.cos() * lerp(from, to, wave)),
                            at.y + px(a.sin() * lerp(from, to, wave)),
                        ),
                    ],
                    (r * 0.07).max(1.),
                    skin.opacity(rays * alpha),
                );
            }
            p.face(&Face {
                at,
                r,
                squash: (1., 1.),
                tilt: 0.,
                mood: scene_mood,
                skin: Some(skin),
                ink,
                alpha,
            });
        });
    }

    /// A small heart floats up and touches the face, which blushes; hearts rise around it.
    fn big_heart(&mut self, sc: &Scene) {
        let t = sc.t;
        let leave = self.leaving(sc);
        let tm = timing(sc.style);
        // Rising hearts go on while the face leaves.
        let mut rand = rng(5);
        for _ in 0..11 {
            let (x, sway, big, start) = (
                rand() * 2. - 1.,
                rand() * TAU,
                0.2 + rand() * 0.26,
                0.42 + rand() * 0.34,
            );
            let k = sl(t, start, start + 0.3);
            if k > 0. && k < 1. {
                let at = point(
                    sc.center.x + px((x * 2.5 + (k * 6. + sway).sin() * 0.16) * sc.r),
                    sc.center.y + px((0.9 - 3.1 * k) * sc.r),
                );
                let color = self.rose.opacity((PI * k).sin().min(0.5) * 2. * 0.72);
                self.heart(at, big * sc.r * (0.75 + 0.5 * k), color);
            }
        }
        if leave.gone {
            return;
        }
        // The big heart behind the face beats twice, and stays until the face leaves.
        let grown = back_out(sl(t, 0.38, 0.5)).max(0.);
        let beat = (PI * sl(t, 0.54, 0.62)).sin() * 0.09 + (PI * sl(t, 0.64, 0.72)).sin() * 0.06;
        let gone = 1. - sl(t, tm.fade.0, tm.fade.0 + 0.04);
        self.halo(leave.at, leave.r, grown.min(1.) * gone, self.rose);
        self.heart(
            point(leave.at.x, leave.at.y - px(sc.r * 0.02)),
            leave.r * 2.75 * grown * (1. + beat),
            self.rose.opacity(0.9 * gone),
        );
        let lean = bounce(sl(t, 0.4, 0.58), 1.) * sc.r * 0.15;
        self.plain(sc, &leave, (1., 1.), 0., lean);
        // The first small heart, on its way up from below.
        let k = sl(t, 0.16, 0.38);
        if k > 0. && k < 1. {
            let at = point(
                sc.center.x + px((0.06 * (k * 13.).sin()) * sc.r),
                sc.center.y + px(lerp(2.4, 0.9, eio(k)) * sc.r),
            );
            let size = sc.r * (0.42 + 0.06 * (k * 18.).sin());
            self.heart(at, size, self.rose.opacity(sl(k, 0., 0.15)));
        }
    }

    /// A sleepy cat: its ears perk up one by one, it smiles, waves, and leaves paw prints.
    fn cat(&mut self, sc: &Scene) {
        let t = sc.t;
        let tm = timing(sc.style);
        let leave = self.leaving(sc);
        if let Some(target) = sc.target {
            // Paw prints trot along the way out, then fade where they were left.
            let n = 7;
            for i in 0..n {
                let along = (i as f32 + 1.) / (n as f32 + 1.);
                let start = lerp(tm.fade.0, tm.land, along);
                let shown = sl(t, start, start + 0.02) * (1. - sl(t, start + 0.03, start + 0.08));
                if shown > 0. {
                    let (_, spot) = self.flight_at(along, sc.center, Some(target), (-120., 0.));
                    let side = if i % 2 == 0 { -7. } else { 7. } * sc.s;
                    self.paw(
                        point(spot.x, spot.y + px(side)),
                        lerp(11., 6., along) * sc.s,
                        sc.skin.opacity(shown * 0.85),
                    );
                }
            }
        }
        if leave.gone {
            return;
        }
        let pop = back_out(sl(t, 0.06, 0.16)).max(0.);
        let r = leave.r * pop;
        let skin = mix(sc.skin, self.green, leave.green);
        let alpha = sl(t, 0.06, 0.1) * leave.alpha;
        let hop = bounce(sl(t, 0.46, 0.64), 2.) * sc.r * 0.3;
        let nod = (t * 20.).sin() * 0.05 * (1. - sl(t, 0.3, 0.38));
        let face = Face {
            at: point(leave.at.x, leave.at.y - px(hop)),
            r,
            squash: (1.06, 0.97),
            tilt: nod,
            mood: mood(sc.style, t),
            skin: Some(skin),
            ink: sc.ink,
            alpha,
        };
        if r >= 8. {
            // Ears, each drooped until its moment.
            for (side, from) in [(-1f32, 0.26), (1., 0.33)] {
                let up = back_out(sl(t, from, from + 0.07)).max(0.);
                let twitch = (PI * 2. * sl(t, from + 0.1, from + 0.16)).sin() * 0.05;
                let tip = (
                    side * lerp(1.04, 0.82, up) + side * twitch,
                    lerp(-0.86, -1.34, up),
                );
                let ear = [(side * 0.24, -0.86), tip, (side * 0.98, -0.34)];
                let outer = ear.map(|(x, y)| face.point(x, y));
                self.shape(&outer, skin.opacity(alpha));
                let middle = (side * 0.62, -0.72);
                let inner = ear
                    .map(|(x, y)| (lerp(middle.0, x, 0.55), lerp(middle.1, y, 0.55)))
                    .map(|(x, y)| face.point(x, y));
                self.shape(&inner, self.rose.opacity(0.55 * alpha));
            }
        }
        self.face(&face);
        if r >= 8. {
            // Whiskers.
            let width = (r * 0.04).max(1.);
            for side in [-1f32, 1.] {
                for dy in [-0.09f32, 0.09] {
                    self.stroke(
                        &[
                            face.point(side * 0.78, 0.2 + dy),
                            face.point(side * 1.2, 0.17 + dy * 2.),
                        ],
                        width,
                        sc.ink.opacity(0.6 * alpha),
                    );
                }
            }
            // The paw comes up beside the face and waves.
            let up = eo(sl(t, 0.5, 0.58)) * (1. - sl(t, tm.fade.0 - 0.03, tm.fade.0));
            if up > 0. {
                let wave = (t * 44.).sin() * 0.13 * up;
                let paw = face.point(1.08 + wave, lerp(1.2, 0.62, up));
                let size = r * 0.5;
                self.pill(paw, size, size * 1.1, skin.opacity(alpha * up));
                for x in [-0.25f32, 0., 0.25] {
                    self.pill(
                        point(paw.x + px(x * size), paw.y - px(size * 0.32)),
                        size * 0.1,
                        size * 0.26,
                        sc.ink.opacity(0.55 * alpha * up),
                    );
                }
            }
        }
    }

    /// A cup with the tired face on it; the warmth reaches it, steam curls into a heart.
    fn tea(&mut self, sc: &Scene) {
        let t = sc.t;
        let leave = self.leaving(sc);
        if leave.gone {
            return;
        }
        let pop = back_out(sl(t, 0.06, 0.16)).max(0.);
        let r = leave.r * pop;
        let a = sl(t, 0.06, 0.1) * leave.alpha;
        let at = point(leave.at.x, leave.at.y + px(r * 0.14));
        let cream = Hsla {
            s: 0.62,
            l: 0.84,
            a: 1.,
            ..sc.skin
        };
        let cup = mix(cream, self.green, leave.green);
        if r < 9. {
            self.disc(at, r.max(LANDED_R), cup.opacity(a));
            return;
        }
        let local = |x: f32, y: f32| point(at.x + px(x * r), at.y + px(y * r));
        let glow = eo(sl(t, 0.46, 0.6)) * a * (1. - leave.green);
        self.halo(at, r * 0.95, glow, sc.skin);
        // Steam: three lines that wander upward and ease off once the heart forms.
        let steam = sl(t, 0.12, 0.26) * (1. - sl(t, 0.66, 0.78)) * a;
        if steam > 0. {
            for (i, x) in [-0.42f32, 0., 0.42].into_iter().enumerate() {
                let points: Vec<_> = (0..=10)
                    .map(|j| {
                        let u = j as f32 / 10.;
                        let wander = ((u * 7.) - t * 22. + i as f32 * 1.9).sin() * 0.1 * (0.3 + u);
                        local(x + wander, -0.86 - 1.1 * u)
                    })
                    .collect();
                self.stroke(&points, (r * 0.055).max(1.), self.soft.opacity(0.6 * steam));
            }
        }
        // The heart above the cup.
        let heart = back_out(sl(t, 0.52, 0.64)).max(0.) * a * (1. - leave.green);
        let beat = 1. + 0.07 * (PI * sl(t, 0.68, 0.76)).sin();
        self.heart(
            local(0., -2.28),
            r * 0.72 * heart * beat,
            self.rose.opacity(heart.min(1.)),
        );
        // The handle goes under the body.
        let handle: Vec<_> = (0..=14)
            .map(|i| {
                let a = lerp(-1.5, 1.5, i as f32 / 14.);
                local(0.88 + 0.42 * a.cos(), 0.08 + 0.42 * a.sin())
            })
            .collect();
        self.stroke(
            &handle,
            (r * 0.15).max(2.),
            mix(sc.skin, self.green, leave.green).opacity(a),
        );
        // The saucer, the body, the tea at the rim.
        self.pill(local(0., 0.92), r * 2.5, r * 0.16, cup.opacity(a * 0.85));
        let mut body: Vec<Point<Pixels>> = Vec::new();
        for i in 0..=8 {
            let u = i as f32 / 8.;
            body.push(local(-lerp(1., 0.68, u * u), lerp(-0.62, 0.72, u)));
        }
        for i in 1..8 {
            let a = PI * (1. - i as f32 / 8.);
            body.push(local(0.68 * a.cos(), 0.72 + 0.16 * a.sin()));
        }
        for i in (0..=8).rev() {
            let u = i as f32 / 8.;
            body.push(local(lerp(1., 0.68, u * u), lerp(-0.62, 0.72, u)));
        }
        self.shape(&body, cup.opacity(a));
        // An edge, so the cup still shows on a light pane.
        self.stroke(
            &body,
            (r * 0.03).max(1.),
            mix(sc.skin, self.green, leave.green).opacity(a),
        );
        let tea = Hsla {
            s: 0.6,
            l: 0.42,
            a: 1.,
            ..sc.skin
        };
        self.pill(local(0., -0.62), r * 2., r * 0.2, tea.opacity(a));
        self.face(&Face {
            at: local(0., 0.1),
            r: r * 0.86,
            squash: (1., 1.),
            tilt: 0.,
            mood: mood(sc.style, t),
            skin: None,
            ink: sc.ink,
            alpha: a,
        });
    }

    /// Cheers up, crouches, and hops all the way to its sidebar row.
    fn hop(&mut self, sc: &Scene) {
        const HOPS: f32 = 5.;
        let t = sc.t;
        let tm = timing(sc.style);
        if t >= tm.land && sc.target.is_some() {
            return;
        }
        let end = sc
            .target
            .unwrap_or(point(sc.body.right() - px(sc.r), sc.center.y));
        let going = sl(t, 0.34, tm.land) * HOPS;
        let (i, f) = (
            going.floor().min(HOPS - 1.),
            going - going.floor().min(HOPS - 1.),
        );
        let along = (i + eio(f.min(1.))) / HOPS;
        let air = (PI * f.min(1.)).sin();
        let r = lerp(sc.r, FACE_R * 0.22 * sc.s, along);
        let ground = point(
            px(lerp(f32::from(sc.center.x), f32::from(end.x), along)),
            px(lerp(f32::from(sc.center.y), f32::from(end.y), along)),
        );
        let crouch = (PI * sl(t, 0.26, 0.34)).sin() * (1. - sl(going, 0., 0.2));
        let contact = if going > 0. { (1. - air).powi(4) } else { 0. };
        let squash = (
            1. + 0.14 * crouch + 0.16 * contact - 0.06 * air,
            1. - 0.2 * crouch - 0.18 * contact + 0.1 * air,
        );
        let high = air * sc.r * 1.5 * (1. - 0.13 * i);
        let fade = if sc.target.is_some() {
            1.
        } else {
            1. - sl(t, 0.76, 0.88)
        };
        let green = sl(along, 0.7, 1.);
        // Its shadow on the ground, which shrinks as it jumps.
        let pop = back_out(sl(t, 0.06, 0.16)).max(0.);
        self.pill(
            point(ground.x, ground.y + px(r * 1.1)),
            r * 1.5 * (1. - 0.4 * air) * pop,
            r * 0.2,
            self.soft.opacity(0.26 * fade * (1. - green)),
        );
        // A little puff where it left the ground.
        if going > 0. && f < 0.45 {
            let k = f / 0.45;
            for side in [-1f32, 1.] {
                self.disc(
                    point(
                        ground.x + px(side * r * (0.5 + 0.7 * k)),
                        ground.y + px(r * 1.05),
                    ),
                    r * 0.14 * (1. - k * 0.4),
                    self.soft.opacity(0.32 * (1. - k) * fade),
                );
            }
        }
        let leave = Leaving {
            at: ground,
            r,
            alpha: fade,
            green,
            gone: false,
        };
        let waddle = (t * 34.).sin() * 0.05 * sl(t, 0.2, 0.3) * (1. - sl(going, 0., 0.5));
        self.plain(sc, &leave, squash, waddle, high);
        self.twinkles(sc.center, sc.r, sl(t, 0.18, 0.4), 31, sc.skin);
    }
    /// Draws within `bounds`.
    fn clipped(&mut self, bounds: Bounds<Pixels>, draw: impl FnOnce(&mut Painter)) {
        let (green, on_accent, warm, rose, soft) =
            (self.green, self.on_accent, self.warm, self.rose, self.soft);
        let mut counts = (0, 0);
        self.window
            .with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                let mut p = Painter {
                    window,
                    paths: 0,
                    quads: 0,
                    green,
                    on_accent,
                    warm,
                    rose,
                    soft,
                };
                draw(&mut p);
                counts = (p.paths, p.quads);
            });
        self.paths += counts.0;
        self.quads += counts.1;
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
            let start = mood(style, 0.12);
            assert_eq!(start.happy, [0., 0.], "{style:?} starts with - - eyes");
            assert_eq!(start.open, [0., 0.], "{style:?} starts with its eyes shut");
            assert!(
                start.smile.abs() < 1e-5,
                "{style:?} starts with a flat mouth"
            );
            let tm = timing(style);
            let later = mood(style, tm.fade.0);
            assert_eq!(later.happy, [1., 1.], "{style:?} ends with happy eyes");
            assert_eq!(later.open, [0., 0.], "{style:?} ends with its eyes arched");
            assert!((later.smile - 1.).abs() < 1e-4, "{style:?} ends smiling");
            assert!((later.blush - 1.).abs() < 1e-4, "{style:?} ends blushing");
            assert!(
                smile_span(style).1 <= tm.words.1,
                "{style:?}: it is smiling by the time the words are in"
            );
            assert!(
                tm.fade.1 <= tm.land + 1e-5,
                "{style:?} lands after the fade"
            );
            assert!(style.greeting().is_some(), "{style:?} says something warm");
        }
    }

    #[test]
    fn waking_opens_one_eye_before_the_other() {
        let peek = mood(Style::Wake, 0.46);
        assert!(peek.open[1] > 0.9 && peek.open[0] < 0.05, "{peek:?}");
        let both = mood(Style::Wake, 0.53);
        assert!(both.open[0] > 0.9 && both.open[1] > 0.9, "{both:?}");
    }

    #[test]
    fn the_cat_keeps_its_mouth() {
        assert_eq!(mood(Style::Cat, 0.12).omega, 1.);
        assert!(mood(Style::Cat, 0.8).omega > 0.7);
    }
}
