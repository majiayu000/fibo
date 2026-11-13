//! What a 标记完成 run draws over the whole window: seals, rings, specks and the light.
use super::*;

/// The sweep: a soft trail behind the light and a short glow ahead of it.
const SWEEP_TRAIL: f32 = 64.;
const SWEEP_GLOW: f32 = 16.;
/// The light's core line, centered on its x.
const SWEEP_LINE_W: f32 = 1.5;
/// The check drawn above the closed line once the sweep shuts the pane.
const SWEEP_CHECK_LIFT: f32 = 34.;
/// Where the dissolve's specks head when the session has no sidebar row in sight.
const DISSOLVE_RISE: f32 = 40.;
/// Each speck's arc: at least this high above its start, plus up to the spread.
const ARC_MIN: f32 = 30.;
const ARC_SPREAD: f32 = 90.;
/// A circle's bounding box side.
fn diameter(radius: f32) -> Pixels {
    px(radius * 2.)
}

pub(super) struct Painter<'a> {
    pub(super) window: &'a mut Window,
    pub(super) cx: &'a mut gpui::App,
    /// The terminal's font, for the faces written in characters.
    pub(super) mono: gpui::Font,
    /// The frames a plugin drew, in place of the face style's own.
    pub(super) face: Option<&'static fibo_core::face::Script>,
    /// Drawn on a card rather than a pane: its sizes go lower than a pane's ever need.
    pub(super) small: bool,
    /// Where text stays, when it must not go past something.
    pub(super) clip: Option<Bounds<Pixels>>,
    pub(super) paths: usize,
    pub(super) quads: usize,
    pub(super) green: Hsla,
    pub(super) on_accent: Hsla,
    /// The face styles' colors, from the theme: the text, the smile and the sun, hearts,
    /// and what is neither (steam, breath, a sleeper's z).
    pub(super) ink: Hsla,
    pub(super) warm: Hsla,
    pub(super) rose: Hsla,
    pub(super) soft: Hsla,
}
impl Painter<'_> {
    pub(super) fn disc(&mut self, at: Point<Pixels>, r: f32, color: Hsla) {
        if r <= 0.05 || color.a <= 0.003 {
            return;
        }
        let b = Bounds::new(
            point(at.x - px(r), at.y - px(r)),
            size(diameter(r), diameter(r)),
        );
        self.quads += 1;
        self.window.paint_quad(gpui::quad(
            b,
            px(r),
            color,
            px(0.),
            gpui::transparent_black(),
            Default::default(),
        ));
    }
    /// A soft halo: a few wider, fainter discs under the real one.
    pub(super) fn glow(&mut self, at: Point<Pixels>, r: f32, strength: f32) {
        if strength <= 0. {
            return;
        }
        for (grow, a) in [(16., 0.08), (10., 0.12), (5., 0.18)] {
            self.disc(at, r + grow * strength, self.green.opacity(a * strength));
        }
    }
    pub(super) fn ring(&mut self, at: Point<Pixels>, r: f32, width: f32, color: Hsla) {
        if r <= 0. || color.a <= 0.003 {
            return;
        }
        let b = Bounds::new(
            point(at.x - px(r), at.y - px(r)),
            size(diameter(r), diameter(r)),
        );
        self.quads += 1;
        self.window.paint_quad(gpui::quad(
            b,
            px(r),
            gpui::transparent_black(),
            px(width),
            color,
            Default::default(),
        ));
    }
    pub(super) fn rect(&mut self, b: Bounds<Pixels>, color: impl Into<gpui::Background>) {
        self.quads += 1;
        self.window.paint_quad(gpui::fill(b, color));
    }
    pub(super) fn stroke(&mut self, points: &[Point<Pixels>], width: f32, color: Hsla) {
        if points.len() < 2 || color.a <= 0.003 {
            return;
        }
        let mut path = PathBuilder::stroke(px(width));
        path.move_to(points[0]);
        for &p in &points[1..] {
            path.line_to(p);
        }
        if let Ok(path) = path.build() {
            self.paths += 1;
            self.window.paint_path(path, color);
        }
    }
    /// The check mark drawn `progress` of the way along its two strokes.
    pub(super) fn check(
        &mut self,
        at: Point<Pixels>,
        s: f32,
        progress: f32,
        color: Hsla,
        width: f32,
    ) {
        if progress <= 0. || s <= 0. {
            return;
        }
        let pts = [(-0.24, 0.02), (-0.07, 0.19), (0.26, -0.17)]
            .map(|(x, y)| point(at.x + px(x * s), at.y + px(y * s)));
        let d =
            |a: Point<Pixels>, b: Point<Pixels>| f32::from(a.x - b.x).hypot(f32::from(a.y - b.y));
        let (l1, l2) = (d(pts[0], pts[1]), d(pts[1], pts[2]));
        let len = (l1 + l2) * progress;
        let mid = |a: Point<Pixels>, b: Point<Pixels>, k: f32| {
            point(a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k)
        };
        let line = if len <= l1 {
            vec![pts[0], mid(pts[0], pts[1], len / l1)]
        } else {
            vec![pts[0], pts[1], mid(pts[1], pts[2], (len - l1) / l2)]
        };
        self.stroke(&line, width, color);
        // Round the joints and ends a stroke path leaves square.
        for p in &line {
            self.disc(*p, width / 2., color);
        }
    }
    fn arc(&mut self, at: Point<Pixels>, r: f32, sweep: f32, width: f32, color: Hsla) {
        let n = (48. * sweep).ceil().max(2.) as usize;
        let pts: Vec<_> = (0..=n)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::TAU * sweep * i as f32 / n as f32;
                point(at.x + px(r * a.cos()), at.y + px(r * a.sin()))
            })
            .collect();
        self.stroke(&pts, width, color);
    }

    pub(super) fn frame(
        &mut self,
        style: Style,
        t: f32,
        pane: Bounds<Pixels>,
        target: Option<Point<Pixels>>,
        glyphs: &[Glyph],
        text: Option<&Snapshot>,
    ) {
        let body = Bounds::new(
            point(pane.left(), pane.top() + px(T::PANE_HEADER)),
            size(pane.size.width, pane.size.height - px(T::PANE_HEADER)),
        );
        let center = point(body.center().x, body.center().y - px(SEAL_LIFT));
        let fade = Workbench::completing_opacity(style, t);
        let g = self.green;
        match style {
            Style::Seal => {
                let pop = back_out(sl(t, 0.15, 0.42)).max(0.);
                let a = sl(t, 0.15, 0.25) * fade;
                self.disc(center, 28. * pop, g.opacity(a));
                if pop > 0.3 {
                    self.check(center, 45. * pop, 1., self.on_accent.opacity(a), 3.);
                }
            }
            Style::Fade => {}
            Style::Moor => self.moor(t, pane, body, target, text),
            Style::Gravity => self.gravity(t, body, target, text),
            Style::Swirl => self.swirl(t, body, target, text),
            Style::Backspace => self.backspace(t, target, text),
            Style::Crt => self.crt(t, body, target),
            Style::Confetti => self.confetti(t, body, target),
            Style::Relief
            | Style::Wake
            | Style::Sunrise
            | Style::Heart
            | Style::Cat
            | Style::Tea
            | Style::Hop => self.cozy(style, t, body, target),
            Style::Stamp => {
                let drop = sl(t, 0.12, 0.3);
                let settle = sl(t, 0.3, 0.4);
                let mut r =
                    lerp(48., 28., eo(drop)) * (1. - 0.07 * (std::f32::consts::PI * settle).sin());
                let glow = if drop < 1. {
                    drop * 0.5
                } else {
                    1. - eo(sl(t, 0.28, 0.55)) * 0.7
                };
                for (from, to, max, w) in [(0.3, 0.62, 120., 2.5), (0.35, 0.72, 80., 1.5)] {
                    let e = eo(sl(t, from, to));
                    if e > 0. && e < 1. {
                        self.ring(center, 28. + max * e, w, g.opacity(0.7 * (1. - e)));
                    }
                }
                let burst = sl(t, 0.3, 0.64);
                if burst > 0. && burst < 1. {
                    let mut rand = rng(7);
                    let tau = burst * 0.55;
                    for i in 0..22 {
                        let a = i as f32 / 22. * std::f32::consts::TAU + (rand() - 0.5) * 0.4;
                        let speed = 110. + rand() * 130.;
                        let s = (2.5 + rand() * 2.5) * (1. - burst * 0.7);
                        let light = rand() < 0.3;
                        let at = point(
                            center.x + px(a.cos() * speed * eo(burst) * 0.6),
                            center.y
                                + px((a.sin() * speed - 30.) * eo(burst) * 0.6 + 160. * tau * tau),
                        );
                        let color = if light {
                            gpui::hsla(0.41, 0.6, 0.93, 1.)
                        } else {
                            g
                        };
                        self.disc(at, s / 2., color.opacity(1. - burst));
                    }
                }
                let (fly, pos) = self.flight(t, 0.74, 0.88, center, target, (-150., 0.));
                if target.is_some() {
                    r = lerp(r, 3.5, eo(sl(t, 0.74, 0.86)));
                }
                let a = sl(t, 0.12, 0.2) * if target.is_some() { 1. } else { fade };
                if fly > 0. && fly < 1. {
                    for i in 1..=6 {
                        let k = (fly - i as f32 * 0.035).max(0.);
                        let (_, p) = self.flight_at(k, center, target, (-150., 0.));
                        self.disc(
                            p,
                            (r * (1. - i as f32 / 8.)).max(1.),
                            g.opacity(0.35 * (1. - i as f32 / 7.)),
                        );
                    }
                }
                if t < 0.88 || target.is_none() {
                    self.glow(pos, r, glow * a);
                    self.disc(pos, r, g.opacity(a));
                    self.check(
                        pos,
                        r * 1.6,
                        sl(t, 0.24, 0.38),
                        self.on_accent.opacity(a),
                        (r / 9.).max(1.2),
                    );
                }
            }
            Style::Sweep => {
                let x = body.left() + body.size.width * sweep_at(t);
                let sweep = eio(sl(t, 0.05, 0.4));
                if sweep > 0. && sweep < 1. {
                    let clip = |l: Pixels, r: Pixels| (l.max(body.left()), r.min(body.right()));
                    let (l, r) = clip(x - px(SWEEP_TRAIL), x);
                    if r > l {
                        let b = Bounds::new(point(l, pane.top()), size(r - l, pane.size.height));
                        self.rect(
                            b,
                            gpui::linear_gradient(
                                90.,
                                gpui::linear_color_stop(g.opacity(0.), 0.),
                                gpui::linear_color_stop(g.opacity(0.22), 1.),
                            ),
                        );
                    }
                    let (l, r) = clip(x, x + px(SWEEP_GLOW));
                    if r > l {
                        let b = Bounds::new(point(l, pane.top()), size(r - l, pane.size.height));
                        self.rect(
                            b,
                            gpui::linear_gradient(
                                90.,
                                gpui::linear_color_stop(g.opacity(0.22), 0.),
                                gpui::linear_color_stop(g.opacity(0.), 1.),
                            ),
                        );
                    }
                    if x > body.left() && x < body.right() {
                        self.rect(
                            Bounds::new(
                                point(x - px(SWEEP_LINE_W / 2.), pane.top()),
                                size(px(SWEEP_LINE_W), pane.size.height),
                            ),
                            gpui::hsla(0.41, 0.6, 0.93, 0.9),
                        );
                    }
                }
                let close = eio(sl(t, 0.66, 0.84));
                let width = eo(sl(t, 0.4, 0.56)) * 180. * (1. - eio(sl(t, 0.84, 0.94)));
                let ly = body.center().y;
                let a = fade;
                if width > 0.5 {
                    let h = 1.5 + close;
                    let b = Bounds::new(
                        point(center.x - px(width / 2.), ly - px(h / 2.)),
                        size(px(width), px(h)),
                    );
                    for (grow, ga) in [(6., 0.08), (3., 0.15)] {
                        let gb = Bounds::new(
                            point(b.left() - px(grow), b.top() - px(grow)),
                            size(
                                b.size.width + diameter(grow),
                                b.size.height + diameter(grow),
                            ),
                        );
                        self.quads += 1;
                        self.window.paint_quad(gpui::quad(
                            gb,
                            px(grow),
                            g.opacity(ga * (0.5 + close) * a),
                            px(0.),
                            gpui::transparent_black(),
                            Default::default(),
                        ));
                    }
                    self.rect(b, g.opacity(a));
                }
                self.check(
                    point(center.x, ly - px(SWEEP_CHECK_LIFT)),
                    40.,
                    sl(t, 0.46, 0.6),
                    g.opacity(1. - sl(t, 0.62, 0.7)),
                    3.,
                );
                // The line, shrunk to a point, takes off for the sidebar.
                let (fly, pos) =
                    self.flight(t, 0.86, 0.94, point(center.x, ly), target, (-120., 0.));
                if target.is_some() && fly > 0. && fly < 1. {
                    self.glow(pos, 3., 1.);
                    self.disc(pos, 3.5, g);
                } else if target.is_none() && t > 0.86 {
                    let k = sl(t, 0.88, 0.97);
                    self.disc(point(center.x, ly), 2. + 10. * eo(k), g.opacity(1. - k));
                }
            }
            Style::Dissolve => {
                let target_or_up =
                    target.unwrap_or(point(center.x, pane.top() - px(DISSOLVE_RISE)));
                let mut rand = rng(42);
                let mut arrived = 0usize;
                let width = f32::from(body.size.width).max(1.);
                let height = f32::from(body.size.height).max(1.);
                for (at, color) in glyphs.iter() {
                    // When it sets off and how it flies, the same on every frame.
                    let ord = f32::from(at.y - body.top()) / height * DISSOLVE_DOWN
                        + f32::from(at.x - body.left()) / width * DISSOLVE_ACROSS;
                    let (j, cx_, cy_, rad) = (rand(), rand(), rand(), 0.7 + rand() * 0.8);
                    let start =
                        DISSOLVE_START + DISSOLVE_SPAN * ord.clamp(0., 1.) + j * DISSOLVE_JITTER;
                    let tau = sl(t, start, start + 0.36);
                    if tau <= 0. {
                        continue;
                    }
                    if tau >= 1. {
                        arrived += 1;
                        continue;
                    }
                    let ctl = point(
                        px(lerp(f32::from(at.x), f32::from(target_or_up.x), 0.35)
                            + (cx_ - 0.5) * ARC_SPREAD),
                        at.y.min(target_or_up.y) - px(ARC_MIN + cy_ * ARC_SPREAD),
                    );
                    let k1 = sl(tau, 0., 0.2);
                    let m = eio(sl(tau, 0.08, 1.));
                    let p = bez(*at, ctl, target_or_up, m);
                    let a = k1 * 0.9 * (1. - sl(tau, 0.9, 1.));
                    // The tail is two fainter, smaller dots behind it: a stroke per speck
                    // is a path for the GPU to rasterize, and hundreds of them halve the
                    // frame rate.
                    for (back, shrink, fade) in [(0.011, 0.8, 0.3), (0.022, 0.6, 0.15)] {
                        let q = bez(*at, ctl, target_or_up, (m - back).max(0.));
                        self.disc(q, rad * shrink, g.opacity(a * fade));
                    }
                    let tint = Hsla {
                        h: lerp(color.h, g.h, k1),
                        s: lerp(color.s, g.s, k1),
                        l: lerp(color.l, g.l, k1),
                        a,
                    };
                    self.disc(p, rad, tint);
                }
                if let Some(at) = target {
                    let f = arrived as f32 / glyphs.len().max(1) as f32;
                    let left = 1. - sl(t, 0.93, 1.);
                    if f > 0. {
                        self.glow(at, 3.5, f * left);
                        self.disc(at, 3.5 + 1.5 * f, g.opacity((0.25 + 0.75 * f) * left));
                    }
                }
                let ring = eo(sl(t, 0.03, 0.2));
                let fill = back_out(sl(t, 0.2, 0.3));
                let (_, pos) = self.flight(t, 0.72, 0.86, center, target, (-170., 40.));
                let r = if target.is_some() {
                    lerp(24., 3.5, eo(sl(t, 0.72, 0.84)))
                } else {
                    24.
                };
                let a = if target.is_some() { 1. } else { fade };
                if t < 0.86 || target.is_none() {
                    if fill < 0.02 {
                        self.ring(center, 24., 2.5, g.opacity(0.18 * a));
                        self.arc(center, 24., ring, 2.5, g.opacity(a));
                    } else {
                        let rr = r * fill.min(1.15);
                        self.glow(pos, rr, 0.6 * a);
                        self.disc(pos, rr, g.opacity(a));
                        self.check(
                            pos,
                            r * 1.6,
                            sl(t, 0.24, 0.36),
                            self.on_accent.opacity(a),
                            (r / 8.).max(1.2),
                        );
                    }
                }
            }
        }
    }
    /// Progress of the flight between `from` and `to` of the run, and where it is.
    pub(super) fn flight(
        &self,
        t: f32,
        from: f32,
        to: f32,
        start: Point<Pixels>,
        target: Option<Point<Pixels>>,
        bend: (f32, f32),
    ) -> (f32, Point<Pixels>) {
        self.flight_at(eio(sl(t, from, to)), start, target, bend)
    }
    pub(super) fn flight_at(
        &self,
        k: f32,
        start: Point<Pixels>,
        target: Option<Point<Pixels>>,
        bend: (f32, f32),
    ) -> (f32, Point<Pixels>) {
        match target {
            Some(end) => {
                let ctl = point((start.x + end.x) / 2. + px(bend.1), start.y + px(bend.0));
                (k, bez(start, ctl, end, k))
            }
            None => (0., start),
        }
    }
    /// The sidebar row lights up and its dot sends two ripples once the session lands.
    pub(super) fn landed(&mut self, t: f32, at: f32, row: Bounds<Pixels>, dot: Point<Pixels>) {
        if t < at {
            return;
        }
        let k = sl(t, at, 1.);
        let wash = self.green.opacity(0.2 * (1. - k));
        self.quads += 1;
        self.window.paint_quad(gpui::quad(
            row,
            px(T::CONTROL_RADIUS),
            wash,
            px(0.),
            gpui::transparent_black(),
            Default::default(),
        ));
        for (delay, max) in [(0., 18.), (0.25, 12.)] {
            let e = eo(sl(k, delay, 1.));
            if e > 0. && e < 1. {
                self.ring(dot, 3.5 + max * e, 1.5, self.green.opacity(0.7 * (1. - e)));
            }
        }
    }
}
