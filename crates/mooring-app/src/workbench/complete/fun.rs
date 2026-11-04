//! The six styles that play with the terminal itself or the session's way home: 靠岸,
//! 字往下掉, 漩涡, 退格收工, 老显示器关机 and 彩纸. A run snapshots what the screen shows
//! when it starts; every frame is still a pure function of `t` and that snapshot.
//! Prototype: docs/complete-animation/directions.html.
use super::snapshot::{LIFT, Letter};
use super::*;
use std::f32::consts::{PI, TAU};

/// How far a character bounces off the pile.
const BOUNCE: f32 = 9.;
/// 靠岸's card when it leaves the pane, as insets of the pane, and when it arrives.
const CARD_INSET: (f32, f32) = (0.1, 0.12);
const CARD_HOME: (f32, f32) = (22., 14.);
/// How far from the row's dot it stops; its border; the rings it leaves in the water.
const CARD_HOME_GAP: f32 = 6.;
const CARD_BORDER: f32 = 1.2;
const CARD_WAKE_RADIUS: f32 = 8.;
const CARD_WAKE_W: f32 = 1.;
/// 字往下掉's specks: how high they arc, and where they go with no sidebar row to reach.
const SPECK_ARC: f32 = 120.;
const SPECK_ARC_SPREAD: f32 = 50.;
const SPECK_RISE: f32 = 40.;
/// 彩纸's pieces in a pane of `CONFETTI_SIDE` on its short side; bigger panes throw them
/// further, up to `CONFETTI_MAX_SCALE`.
const CONFETTI: usize = 130;
const CONFETTI_SIDE: f32 = 280.;
const CONFETTI_MAX_SCALE: f32 = 2.2;

/// How far the lids have closed on 老显示器关机.
pub(super) fn crt_lids(t: f32) -> f32 {
    eio(sl(t, 0.16, 0.42))
}
/// Paints what moves inside the pane in one layer and keeps it there. A layer spares each
/// of its thousands of primitives a place in GPUI's bounds tree, but clips nothing itself.
fn in_pane(window: &mut Window, body: Bounds<Pixels>, paint: impl FnOnce(&mut Window)) {
    window.with_content_mask(Some(gpui::ContentMask { bounds: body }), |window| {
        window.paint_layer(body, paint)
    });
}
fn mix(a: Hsla, b: Hsla, k: f32) -> Hsla {
    Hsla {
        h: lerp(a.h, b.h, k),
        s: lerp(a.s, b.s, k),
        l: lerp(a.l, b.l, k),
        a: a.a,
    }
}

impl Painter<'_> {
    /// Draws specks in one layer: hundreds of quads outside one each cost a place in
    /// GPUI's bounds tree.
    fn specks(&mut self, specks: &[(Point<Pixels>, f32, Hsla)]) {
        self.quads += specks.len();
        if specks.is_empty() {
            return;
        }
        let bounds = Bounds::new(point(px(0.), px(0.)), self.window.viewport_size());
        self.window.paint_layer(bounds, |window| {
            for &(at, r, color) in specks {
                window.paint_quad(gpui::quad(
                    Bounds::new(
                        point(at.x - px(r), at.y - px(r)),
                        size(px(r * 2.), px(r * 2.)),
                    ),
                    px(r),
                    color,
                    px(0.),
                    gpui::transparent_black(),
                    Default::default(),
                ));
            }
        });
    }

    pub(super) fn moor(
        &mut self,
        t: f32,
        pane: Bounds<Pixels>,
        body: Bounds<Pixels>,
        target: Option<Point<Pixels>>,
        text: Option<&Snapshot>,
    ) {
        let g = self.green;
        let rope = self.warm;
        let k = eio(sl(t, 0.44, 0.84));
        let (pw, ph) = (f32::from(pane.size.width), f32::from(pane.size.height));
        let home = target.map_or(body.center(), |d| point(d.x + px(CARD_HOME_GAP), d.y));
        // The card, from most of the pane to a chip beside the row's dot.
        let big = (
            f32::from(pane.left()) + pw * CARD_INSET.0,
            f32::from(pane.top()) + ph * CARD_INSET.1,
            pw * (1. - CARD_INSET.0 * 2.),
            ph * (1. - CARD_INSET.1 * 2.),
        );
        let small = (
            f32::from(home.x),
            f32::from(home.y) - CARD_HOME.1 / 2.,
            CARD_HOME.0,
            CARD_HOME.1,
        );
        let bob = (t * 38.).sin() * 2.6 * sl(t, 0.44, 0.54) * (1. - sl(t, 0.74, 0.84));
        let (x, y, w, h) = (
            lerp(big.0, small.0, k),
            lerp(big.1, small.1, k) + bob,
            lerp(big.2, small.2, k),
            lerp(big.3, small.3, k),
        );
        let land = timing(Style::Moor).land;
        let show = sl(t, 0.42, 0.48) * (1. - sl(t, 0.82, land));
        // Without a row to be hauled to, the card only shrinks away in place.
        if let Some(dot) = target
            && t > 0.08
            && t < land
        {
            let shot = eo(sl(t, 0.08, 0.24));
            let on_card = sl(t, 0.42, 0.48);
            let edge = point(
                px(lerp(f32::from(pane.left()) + 1., x, on_card)),
                px(lerp(f32::from(pane.top()) + ph / 2., y + h / 2., on_card)),
            );
            let end = point(
                dot.x + (edge.x - dot.x) * shot,
                dot.y + (edge.y - dot.y) * shot,
            );
            let sag = lerp(46., 3., eo(sl(t, 0.24, 0.46))) * (1. - 0.5 * k)
                + (t * 30.).sin() * 2. * (1. - sl(t, 0.24, 0.5));
            let ctl = point((dot.x + end.x) / 2., (dot.y + end.y) / 2. + px(sag));
            let points: Vec<_> = (0..=20)
                .map(|i| bez(dot, ctl, end, i as f32 / 20.))
                .collect();
            self.stroke(&points, 1.5, rope.opacity(0.95));
            self.disc(end, 2.5, rope);
            self.disc(dot, 3., rope);
        }
        if show <= 0. {
            return;
        }
        // The water it leaves: two rings spreading off the card while it moves.
        if k > 0.03 {
            for i in 1..=2 {
                let wake = (t * 5. + i as f32 * 0.5).fract();
                let grow = wake * 12. * i as f32;
                self.quads += 1;
                self.window.paint_quad(gpui::quad(
                    Bounds::new(
                        point(px(x - grow), px(y - grow * 0.6)),
                        size(px(w + grow * 2.), px(h + grow * 1.2)),
                    ),
                    px(CARD_WAKE_RADIUS),
                    gpui::transparent_black(),
                    px(CARD_WAKE_W),
                    rope.opacity(0.28 * (1. - wake) * show),
                    Default::default(),
                ));
            }
        }
        let card = Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
        self.quads += 1;
        self.window.paint_quad(gpui::quad(
            card,
            px(lerp(8., 3., k)),
            T::color(T::pane()).opacity(show),
            px(CARD_BORDER),
            g.opacity(show),
            Default::default(),
        ));
        // The screen in miniature: each line of text a bar, where it was.
        if let Some(s) = text {
            let (gw, gh) = (
                f32::from(s.grid.size.width).max(1.),
                f32::from(s.grid.size.height).max(1.),
            );
            let (sx, sy) = (w * 0.9 / gw, h * 0.9 / gh);
            let thick = (f32::from(s.row) * sy * 0.5).max(0.6);
            for &(left, right, top, color) in &s.lines {
                let at = point(
                    px(x + w * 0.05 + f32::from(left - s.grid.left()) * sx),
                    px(y + h * 0.05 + f32::from(top - s.grid.top()) * sy),
                );
                self.rect(
                    Bounds::new(at, size(px(f32::from(right - left) * sx), px(thick))),
                    color.opacity(0.55 * show),
                );
            }
        }
        let mark = point(px(x + w / 2.), px(y + h / 2.));
        let r = lerp(28., 5.5, k);
        let a = eo(sl(t, 0.46, 0.54)) * show;
        self.glow(mark, r, 0.6 * a * (1. - k));
        self.disc(mark, r, g.opacity(a));
        self.check(
            mark,
            r * 1.6,
            sl(t, 0.48, 0.58),
            self.on_accent.opacity(a),
            (r / 8.).max(1.2),
        );
    }

    pub(super) fn gravity(
        &mut self,
        t: f32,
        body: Bounds<Pixels>,
        target: Option<Point<Pixels>>,
        text: Option<&Snapshot>,
    ) {
        let Some(s) = text.filter(|_| t >= LIFT) else {
            return;
        };
        let g = self.green;
        let height = f32::from(body.size.height).max(1.);
        let width = f32::from(s.grid.size.width).max(1.);
        let drain_of =
            |l: &Letter| 0.6 + (f32::from(l.pile.x - s.grid.left()) / width).clamp(0., 1.) * 0.13;
        let tint = sl(t, 0.45, 0.62) * 0.75;
        in_pane(self.window, body, |window| {
            window.with_glyph_dilation(None, |window| {
                for l in &s.letters {
                    if t >= drain_of(l) {
                        continue;
                    }
                    let down = (f32::from(l.at.y - body.top()) / height).clamp(0., 1.);
                    let start = LIFT + 0.03 + (1. - down) * 0.2 + l.jitter * 0.05;
                    let tau = sl(t, start, start + 0.17);
                    let fall = tau * tau * 1.7;
                    let mut y = lerp(f32::from(l.at.y), f32::from(l.pile.y), fall.min(1.));
                    if fall > 1. {
                        let hit = (1f32 / 1.7).sqrt();
                        let b = (tau - hit) / (1. - hit);
                        y = f32::from(l.pile.y) - (b * TAU).sin().abs() * (1. - b) * BOUNCE;
                    }
                    let x = lerp(f32::from(l.at.x), f32::from(l.pile.x), tau);
                    let color = if tau > 0. {
                        mix(l.color, g, tint)
                    } else {
                        l.color
                    };
                    s.draw(l, point(px(x), px(y)), color, window);
                }
            })
        });
        if t < 0.6 {
            return;
        }
        let to = target.unwrap_or(point(body.center().x, body.top() - px(SPECK_RISE)));
        let mut arrived = 0usize;
        let mut specks = Vec::new();
        for l in &s.letters {
            let drain = drain_of(l);
            let go = sl(t, drain, drain + 0.13);
            if go >= 1. {
                arrived += 1;
            } else if go > 0. {
                let from = s.middle(l, l.pile);
                let ctl = point(
                    px(lerp(f32::from(from.x), f32::from(to.x), 0.4)),
                    from.y - px(SPECK_ARC + l.jitter * SPECK_ARC_SPREAD),
                );
                specks.push((
                    bez(from, ctl, to, eio(go)),
                    1.5,
                    g.opacity(0.92 * (1. - sl(go, 0.9, 1.))),
                ));
            }
        }
        self.specks(&specks);
        if let Some(at) = target
            && !s.letters.is_empty()
        {
            let f = arrived as f32 / s.letters.len() as f32;
            let left = 1. - sl(t, 0.93, 1.);
            if f > 0. {
                self.glow(at, 3.5, f * left);
                self.disc(at, 3.5 + 1.5 * f, g.opacity((0.25 + 0.75 * f) * left));
            }
        }
    }

    pub(super) fn swirl(
        &mut self,
        t: f32,
        body: Bounds<Pixels>,
        target: Option<Point<Pixels>>,
        text: Option<&Snapshot>,
    ) {
        let g = self.green;
        let center = point(body.center().x, body.center().y - px(SEAL_LIFT));
        let reach = f32::from(body.size.width)
            .hypot(f32::from(body.size.height))
            .max(1.)
            / 2.;
        let mut inside = 0usize;
        let mut total = 0usize;
        if let Some(s) = text.filter(|_| t >= LIFT) {
            total = s.letters.len();
            let mut specks = Vec::new();
            in_pane(self.window, body, |window| {
                window.with_glyph_dilation(None, |window| {
                    for l in &s.letters {
                        let mid = s.middle(l, l.at);
                        let (dx, dy) = (f32::from(mid.x - center.x), f32::from(mid.y - center.y));
                        let r = dx.hypot(dy);
                        let start = LIFT + (r / reach).min(1.) * 0.26 + l.jitter * 0.05;
                        let tau = sl(t, start, start + 0.32);
                        if tau >= 1. {
                            inside += 1;
                            continue;
                        }
                        if tau <= 0. {
                            s.draw(l, l.at, l.color, window);
                            continue;
                        }
                        let e = eio(tau);
                        let angle = dy.atan2(dx) + e * e * 7.;
                        let at = point(
                            center.x + px(angle.cos() * r * (1. - e)),
                            center.y + px(angle.sin() * r * (1. - e) * 0.92),
                        );
                        if tau < 0.55 {
                            let shift = point(at.x - (mid.x - l.at.x), at.y - (mid.y - l.at.y));
                            let color = mix(l.color, g, sl(tau, 0.15, 0.5));
                            s.draw(l, shift, color, window);
                        } else {
                            specks.push((at, lerp(1.8, 1., tau), g.opacity(0.9)));
                        }
                    }
                })
            });
            self.specks(&specks);
        }
        let filled = if total == 0 {
            0.
        } else {
            inside as f32 / total as f32
        };
        let pop = back_out(sl(t, 0.54, 0.64)).max(0.);
        let (fly, pos) = self.flight(t, 0.72, 0.86, center, target, (-150., 20.));
        let mut r = lerp(3., 24., (filled * 0.6).max(pop * 0.97));
        if target.is_some() {
            r = lerp(r, 3.5, eo(sl(t, 0.72, 0.84)));
        }
        let a = if target.is_some() {
            1.
        } else {
            Workbench::completing_opacity(Style::Swirl, t)
        };
        if (filled > 0. || pop > 0.) && (t < 0.86 || target.is_none()) {
            self.glow(pos, r, 0.8 * a);
            self.disc(pos, r, g.opacity(a));
            self.check(
                pos,
                r * 1.6,
                sl(t, 0.58, 0.68),
                self.on_accent.opacity(a),
                (r / 8.).max(1.2),
            );
        }
        if fly > 0. && fly < 1. {
            for i in 1..=5 {
                let (_, p) = self.flight_at(
                    (fly - i as f32 * 0.04).max(0.),
                    center,
                    target,
                    (-150., 20.),
                );
                self.disc(
                    p,
                    (r * (1. - i as f32 / 7.)).max(1.),
                    g.opacity(0.3 * (1. - i as f32 / 6.)),
                );
            }
        }
    }

    pub(super) fn backspace(
        &mut self,
        t: f32,
        target: Option<Point<Pixels>>,
        text: Option<&Snapshot>,
    ) {
        let Some(s) = text else {
            return;
        };
        let g = self.green;
        let line_top = s.grid.top();
        // Deleting: the cursor sits after the last character left. Then it types.
        let mut end = match s.kept(t).checked_sub(1) {
            Some(last) if t < 0.54 => point(s.reading[last].0, s.reading[last].1),
            _ => s.grid.origin,
        };
        if t >= 0.54 {
            let shown = (s.typed.len() as f32 * sl(t, 0.57, 0.72)).round() as usize;
            let fade = Workbench::completing_opacity(Style::Backspace, t);
            let mut x = s.grid.left();
            let window = &mut *self.window;
            window.with_glyph_dilation(None, |window| {
                for (i, &(ch, width)) in s.typed.iter().take(shown).enumerate() {
                    let color = if i == 0 { g } else { s.fg };
                    s.draw_char(
                        ch,
                        width,
                        Default::default(),
                        point(x, line_top),
                        color.opacity(fade),
                        window,
                    );
                    x += s.cell * width as f32;
                }
            });
            end = point(x, line_top);
        }
        // Only the first third of the way home does it stay a full cursor.
        let fly = eio(sl(t, 0.78, 0.9));
        let (cw, ch) = (f32::from(s.cell), f32::from(s.row));
        if fly <= 0. {
            let blink = t > 0.72 && (t * 60.).sin() < 0.;
            if t > 0.12 && !blink {
                self.rect(Bounds::new(end, size(s.cell, s.row)), g);
            }
            return;
        }
        if let Some(dot) = target.filter(|_| fly < 1.) {
            let from = point(end.x + s.cell / 2., end.y + s.row / 2.);
            for i in (1..=4).rev() {
                let (_, b) = self.flight_at(
                    (fly - i as f32 * 0.05).max(0.),
                    from,
                    Some(dot),
                    (-40., 30.),
                );
                self.disc(
                    b,
                    3. - i as f32 * 0.4,
                    g.opacity(0.3 * (1. - i as f32 / 5.)),
                );
            }
            let (_, q) = self.flight_at(fly, from, Some(dot), (-40., 30.));
            let (w, h) = (lerp(cw, 4., fly), lerp(ch, 6., fly));
            self.rect(
                Bounds::new(
                    point(q.x - px(w / 2.), q.y - px(h / 2.)),
                    size(px(w), px(h)),
                ),
                g,
            );
        } else if target.is_none() {
            self.rect(
                Bounds::new(end, size(s.cell, s.row)),
                g.opacity(Workbench::completing_opacity(Style::Backspace, t)),
            );
        }
    }

    pub(super) fn crt(&mut self, t: f32, body: Bounds<Pixels>, target: Option<Point<Pixels>>) {
        let g = self.green;
        // The tube's light: nearly white on a dark pane, where a light pane needs the green.
        let light = if T::color(T::pane()).l > 0.5 {
            g
        } else {
            gpui::hsla(0.41, 0.6, 0.93, 1.)
        };
        let flash = sl(t, 0.12, 0.16) * (1. - sl(t, 0.16, 0.26));
        if flash > 0. {
            self.rect(body, light.opacity(0.32 * flash));
        }
        let lid = crt_lids(t);
        let closing = sl(lid, 0.5, 1.);
        let mid = body.center();
        let w = f32::from(body.size.width);
        let shrink = eio(sl(t, 0.42, 0.56));
        let width = lerp(w, 3., shrink);
        if closing > 0. && t < 0.56 {
            let h = lerp(f32::from(body.size.height) * (1. - lid), 2.2, closing);
            for (grow, a) in [(8., 0.08), (4., 0.16), (0., 1.)] {
                let color = if grow > 0. { g.opacity(a) } else { light };
                let b = Bounds::new(
                    point(mid.x - px(width / 2. + grow), mid.y - px(h / 2. + grow)),
                    size(px(width + grow * 2.), px(h + grow * 2.)),
                );
                self.rect(b, color);
            }
        }
        if t >= 0.56 {
            let (fly, pos) = self.flight(t, 0.6, 0.76, mid, target, (-110., 0.));
            if target.is_some() {
                if fly < 1. {
                    let pulse = 1. + 0.6 * (sl(t, 0.56, 0.6) * PI).sin();
                    self.glow(pos, 3., 1.);
                    self.disc(pos, 3.5 * pulse, light);
                }
            } else {
                let k = sl(t, 0.58, 0.74);
                self.glow(mid, 3., 1. - k);
                self.disc(mid, 3.5, light.opacity(1. - k));
            }
        }
    }

    pub(super) fn confetti(&mut self, t: f32, body: Bounds<Pixels>, target: Option<Point<Pixels>>) {
        let g = self.green;
        let center = point(body.center().x, body.center().y - px(SEAL_LIFT));
        let hit = 0.26;
        let tau = sl(t, hit, 0.82);
        let colors = [g, self.warm, self.rose, self.ink, self.soft];
        let side = f32::from(body.size.width.min(body.size.height));
        let scale = (side / CONFETTI_SIDE).clamp(1., CONFETTI_MAX_SCALE);
        if tau > 0. && tau < 1. {
            let mut rand = rng(7);
            let a = 1. - sl(tau, 0.72, 1.);
            let s = tau * 2.35;
            let drift = (1. - (-s * 2.2).exp()) / 2.2;
            self.quads += CONFETTI;
            in_pane(self.window, body, |window| {
                for _ in 0..CONFETTI {
                    let (dir, speed) = (rand() * TAU, 170. + rand() * 360.);
                    let (w, h) = (3. + rand() * 2.6, 5. + rand() * 3.6);
                    let (spin, phase) = (5. + rand() * 9., rand() * 6.);
                    let color = colors[(rand() * colors.len() as f32) as usize % colors.len()];
                    let x = dir.cos() * speed * scale * drift;
                    let y = (dir.sin() * speed * 0.9 - 170.) * scale * drift + 64. * scale * s * s;
                    let flip = (s * spin + phase).cos().abs();
                    let tall = (h * flip).max(0.8);
                    window.paint_quad(gpui::fill(
                        Bounds::new(
                            point(center.x + px(x - w / 2.), center.y + px(y - tall / 2.)),
                            size(px(w), px(tall)),
                        ),
                        color.opacity(a),
                    ));
                }
            });
        }
        let drop = sl(t, 0.12, hit);
        let settle = sl(t, hit, 0.34);
        let mut r = lerp(48., 27., eo(drop)) * (1. - 0.07 * (PI * settle).sin());
        let (_, pos) = self.flight(t, 0.74, 0.88, center, target, (-150., 0.));
        if target.is_some() {
            r = lerp(r, 3.5, eo(sl(t, 0.74, 0.86)));
        }
        let a = sl(t, 0.12, 0.2)
            * if target.is_some() {
                1.
            } else {
                Workbench::completing_opacity(Style::Confetti, t)
            };
        if a > 0. && (t < 0.88 || target.is_none()) {
            if t < 0.5 {
                self.glow(pos, r, (1. - eo(sl(t, hit, 0.5)) * 0.7) * a);
            }
            self.disc(pos, r, g.opacity(a));
            self.check(
                pos,
                r * 1.6,
                sl(t, 0.24, 0.36),
                self.on_accent.opacity(a),
                (r / 9.).max(1.2),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::snapshot::typed_line;
    use super::*;

    #[test]
    fn the_typed_line_fits_and_counts_wide_characters_twice() {
        let line = typed_line(Some("修好登录页"), 80);
        assert_eq!(line[0], ('✓', 1));
        assert!(line.iter().any(|&(c, w)| c == '修' && w == 2));
        let short = typed_line(Some("一个很长很长很长很长的标题"), 12);
        let used: usize = short.iter().map(|&(_, w)| w as usize).sum();
        assert!(used + 2 <= 12, "{used}");
    }

    #[test]
    fn the_new_styles_keep_a_way_to_keep_the_pane_before_it_goes() {
        for style in [
            Style::Moor,
            Style::Gravity,
            Style::Swirl,
            Style::Backspace,
            Style::Crt,
            Style::Confetti,
        ] {
            let tm = timing(style);
            // In full view for a while, while the pane is still there behind it.
            assert!(tm.words.1 < tm.words.2, "{style:?}");
            assert!(tm.words.1 < tm.fade.0, "{style:?}");
            assert!(tm.fade.1 <= 1., "{style:?}");
        }
    }
}
