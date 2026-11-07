//! 先不管这台's flight: without it a machine just vanishes and a gray line appears at the
//! foot, and the eye does not connect them. The machine's own square and name travel
//! between where they are drawn and where they will be drawn: both ends are measured, so at
//! each end they lie exactly over the real ones and nothing seems to jump. Then the line
//! glows once. It ends like all the column's motion (see `sidebar_motion`): nothing is left
//! drawn and nothing asks for a frame.
use super::super::sidebar_machines::{MARK, machine_letter, machine_mark};
use super::*;

const FLY: f32 = 420.;
const GLOW: f32 = 600.;
const PILL: f32 = 26.;
/// The pill around the name shows and goes over this much of its flight, so at both ends
/// only the square and the name are left.
const FADE: f32 = 70.;
/// How long what it lands on takes to come up: short, as the two would read double.
const COME_UP: f32 = 30.;

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4. * t * t * t
    } else {
        1. - (2. - 2. * t).powi(3) / 2.
    }
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
/// `from` turned `t` of the way to `to`, for the words as the line glows and as they travel.
pub(in crate::workbench) fn mix(from: gpui::Hsla, to: gpui::Hsla, t: f32) -> gpui::Hsla {
    gpui::Hsla {
        l: lerp(from.l, to.l, t),
        a: lerp(from.a, to.a, t),
        ..from
    }
}
/// Where a square `MARK` wide sits centered in `b`: `b` is the square itself on a heading or
/// a chip, and the whole button on the rail.
fn mark_in(b: gpui::Bounds<gpui::Pixels>) -> (f32, f32) {
    (
        f32::from(b.origin.x) + (f32::from(b.size.width) - MARK) / 2.,
        f32::from(b.origin.y) + (f32::from(b.size.height) - MARK) / 2.,
    )
}

pub(super) struct Flight {
    host: String,
    name: String,
    letter: String,
    /// Toward the gray line, rather than back from it.
    leaving: bool,
    /// Where the square starts.
    from: (f32, f32),
    /// From the rail, which shows a letter and no name: the name comes up on the way.
    rail: bool,
    /// A space's colors: it flies as the dot it is at both ends, not as a machine's square.
    dot: Option<[u32; 2]>,
    started: Instant,
}
impl Flight {
    fn elapsed(&self, now: Instant) -> f32 {
        now.saturating_duration_since(self.started).as_secs_f32() * 1000.
    }
    fn total(&self) -> f32 {
        if self.leaving { FLY + GLOW } else { FLY }
    }
    pub(super) fn ended(&self, now: Instant) -> bool {
        self.elapsed(now) >= self.total()
    }
    /// Where the square is on its way from `from` to `to`.
    fn at(&self, to: (f32, f32), now: Instant) -> (f32, f32) {
        let t = ease_in_out(part(self.elapsed(now), FLY));
        (lerp(self.from.0, to.0, t), lerp(self.from.1, to.1, t))
    }
    /// How much of the pill around the name shows: none at either end.
    fn chrome(&self, now: Instant) -> f32 {
        let e = self.elapsed(now);
        part(e, FADE).min(part(FLY - e, FADE))
    }
    /// What it lands on, whole: there at the very end, under the square and name.
    fn landed(&self, now: Instant) -> f32 {
        part(self.elapsed(now) - (FLY - COME_UP), COME_UP)
    }
    /// The words after the machine's name on the gray line, once it has landed.
    fn rest(&self, now: Instant) -> f32 {
        part(self.elapsed(now) - FLY, FADE * 2.)
    }
    fn glow(&self, now: Instant) -> f32 {
        1. - ease_out(part(self.elapsed(now) - FLY, GLOW))
    }
}

/// The fold of a whole machine's section as it leaves for the line or comes back.
pub(in crate::workbench) fn block_key(host: &str) -> String {
    format!("quiet-block:{host}")
}

/// Where the squares were last drawn, by machine.
#[derive(Default)]
pub(in crate::workbench) struct Seats {
    /// On the heading, chip or rail button of a machine you attend to.
    marks: Rc<RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>>,
    /// On a quiet machine's gray line.
    lines: Rc<RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>>,
}

/// How the gray line draws under a flight.
pub(in crate::workbench) struct LineState {
    pub shown: f32,
    pub rest: f32,
    pub glow: f32,
}

impl SidebarMotion {
    /// Starts `host`'s flight from its square, wherever that was last drawn.
    pub(in crate::workbench) fn fly(&mut self, host: &str, name: &str, leaving: bool, rail: bool) {
        if mooring_core::effect::reduce_motion() {
            return;
        }
        let now = clock();
        let drawn = |seats: &Rc<RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>>| {
            seats.borrow().get(host).copied().map(mark_in)
        };
        let (mark, line) = (drawn(&self.seats.marks), drawn(&self.seats.lines));
        // Turning back midway starts from where it is.
        let midway = (self.flight.as_ref())
            .filter(|f| f.host == host && !f.ended(now))
            .and_then(|f| Some(f.at(if f.leaving { line? } else { mark? }, now)));
        let from = midway.or(if leaving { mark } else { line });
        // Where it goes back to moves with the list: measure it again once drawn.
        if !leaving {
            self.seats.marks.borrow_mut().remove(host);
        }
        // Never drawn there (it came from ⌘P in another layout): just the line's glow.
        let from = from.unwrap_or((T::S14, F::WIDTH * 0.4));
        // What it heads closes and opens with it, so what lies below moves rather than jumps.
        self.fold(&block_key(host), !leaving, now);
        self.flight = Some(Flight {
            host: host.to_owned(),
            name: name.to_owned(),
            letter: machine_letter(name),
            leaving,
            from,
            rail,
            dot: None,
            started: now,
        });
    }
    /// What just took off is a space.
    pub(in crate::workbench) fn fly_as_dot(&mut self, tile: [u32; 2]) {
        if let Some(flight) = &mut self.flight {
            flight.dot = Some(tile);
        }
    }
    /// How the line for `host` draws now.
    pub(in crate::workbench) fn line_state(&self, host: &str) -> LineState {
        let now = clock();
        (self.flight.as_ref())
            .filter(|f| f.leaving && f.host == host)
            .map_or(
                LineState {
                    shown: 1.,
                    rest: 1.,
                    glow: 0.,
                },
                |f| LineState {
                    shown: f.landed(now),
                    rest: f.rest(now),
                    glow: f.glow(now),
                },
            )
    }
    /// How a machine's heading, chip or button draws now: it gives way to the square in
    /// flight as that sets off, and comes up as that lands going back.
    pub(in crate::workbench) fn source_shown(&self, host: &str) -> f32 {
        let now = clock();
        (self.flight.as_ref())
            .filter(|f| f.host == host)
            .map_or(1., |f| {
                if f.leaving {
                    1. - part(f.elapsed(now), FADE)
                } else {
                    f.landed(now)
                }
            })
    }
    /// Whether a quiet machine's section is still closing, so it is still drawn.
    pub(in crate::workbench) fn block_closing(&self, host: &str) -> bool {
        self.folding(&block_key(host)).is_some()
    }
    /// Draws nothing but remembers where `host`'s square is.
    pub(in crate::workbench) fn mark_spot(&self, host: &str, line: bool) -> gpui::Canvas<()> {
        let seats = if line {
            self.seats.lines.clone()
        } else {
            self.seats.marks.clone()
        };
        let host = host.to_owned();
        gpui::canvas(
            move |bounds, _, _| {
                seats.borrow_mut().insert(host, bounds);
            },
            |_, _, _, _| (),
        )
    }
    /// Where the square is and where it lands; `None` once landed.
    fn course(&self, now: Instant) -> Option<(&Flight, (f32, f32))> {
        let f = self.flight.as_ref()?;
        if f.elapsed(now) >= FLY {
            return None;
        }
        let seats = if f.leaving {
            &self.seats.lines
        } else {
            &self.seats.marks
        };
        // What it lands on has yet to be drawn on the first frame: it waits there.
        let to = seats.borrow().get(&f.host).copied().map_or(f.from, mark_in);
        Some((f, f.at(to, now)))
    }
    /// The square and name in flight, in window coordinates (the column starts at its corner).
    pub(in crate::workbench) fn pill(&self) -> Option<gpui::Div> {
        let now = clock();
        let (f, (x, y)) = self.course(now)?;
        let chrome = f.chrome(now);
        let t = ease_in_out(part(f.elapsed(now), FLY));
        // How far along from the heading's look to the line's.
        let quiet = if f.leaving { t } else { 1. - t };
        let name = if f.rail {
            part(f.elapsed(now), FADE * 2.).min(if f.leaving {
                1.
            } else {
                part(FLY - f.elapsed(now), FADE * 2.)
            })
        } else {
            1.
        };
        let pad = T::S6;
        let ink = mix(T::color(F::bright()), T::color(F::third()), quiet);
        Some(
            div()
                .absolute()
                .left(px(x - pad))
                .top(px(y - (PILL - MARK) / 2.))
                .h(px(PILL))
                .pl(px(pad - T::BORDER_WIDTH))
                .pr(px(T::S10))
                .flex()
                .items_center()
                .gap(px(T::S6))
                .rounded(px(PILL / 2.))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(F::chip_on_line()).opacity(chrome))
                .bg(T::color(T::raised()).opacity(chrome))
                .shadow(vec![gpui::BoxShadow {
                    color: gpui::black().opacity(0.3 * chrome),
                    offset: gpui::point(px(0.), px(T::S4)),
                    blur_radius: px(T::S12),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .font_family(T::UI_FONT)
                .font_weight(if quiet < 0.5 {
                    gpui::FontWeight::SEMIBOLD
                } else {
                    gpui::FontWeight::NORMAL
                })
                .text_size(px(lerp(T::SMALL, F::TAG, quiet)))
                .text_color(ink)
                .child(match f.dot {
                    Some([from, to]) => div()
                        .size(px(F::CHIP_DOT))
                        .my(px((MARK - F::CHIP_DOT) / 2.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(gpui::linear_gradient(
                            135.,
                            gpui::linear_color_stop(T::color(from), 0.),
                            gpui::linear_color_stop(T::color(to), 1.),
                        )),
                    None => machine_mark(f.letter.clone(), quiet >= 0.5),
                })
                .child(div().opacity(name).child(f.name.clone())),
        )
    }
}

#[cfg(feature = "visual-test")]
impl SidebarMotion {
    /// Where `host`'s square was drawn on its heading and on its line, and is in flight now.
    pub(in crate::workbench) fn visual_marks(&self, host: &str) -> [Option<(f32, f32)>; 3] {
        let drawn = |s: &Rc<RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>>| {
            s.borrow().get(host).copied().map(mark_in)
        };
        [
            drawn(&self.seats.marks),
            drawn(&self.seats.lines),
            self.course(clock()).map(|(_, at)| at),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flight(leaving: bool, started: Instant) -> Flight {
        Flight {
            host: "me@apple".into(),
            name: "apple".into(),
            letter: "A".into(),
            leaving,
            from: (30., 200.),
            rail: false,
            dot: None,
            started,
        }
    }
    fn after(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn the_square_starts_and_ends_exactly_on_the_real_ones() {
        let start = Instant::now();
        let f = flight(true, start);
        assert_eq!(f.at((60., 700.), start), (30., 200.));
        let (x, y) = f.at((60., 700.), after(start, 420));
        assert!((x - 60.).abs() < 0.01 && (y - 700.).abs() < 0.01, "{x} {y}");
        let tops: Vec<f32> = (0..=42)
            .map(|i| f.at((60., 700.), after(start, i * 10)).1)
            .collect();
        assert!(tops.windows(2).all(|w| w[1] >= w[0]), "{tops:?}");
    }
    #[test]
    fn only_the_square_and_name_are_left_at_each_end() {
        let start = Instant::now();
        let f = flight(true, start);
        assert_eq!(f.chrome(start), 0.);
        assert_eq!(f.chrome(after(start, 200)), 1.);
        assert_eq!(f.chrome(after(start, 420)), 0.);
    }
    #[test]
    fn the_line_comes_up_only_as_it_lands_then_stops_glowing() {
        let start = Instant::now();
        let f = flight(true, start);
        assert_eq!(f.landed(after(start, 380)), 0., "it would read double");
        assert_eq!(f.landed(after(start, 420)), 1.);
        assert_eq!(
            f.rest(after(start, 420)),
            0.,
            "the words came before it landed"
        );
        assert_eq!(
            f.glow(after(start, 420)),
            1.,
            "the glow starts when it lands"
        );
        assert_eq!(f.glow(after(start, 1020)), 0., "the glow never ends");
    }
    #[test]
    fn it_ends_so_the_column_stops_asking_for_frames() {
        let start = Instant::now();
        assert!(!flight(true, start).ended(after(start, 1000)));
        assert!(flight(true, start).ended(after(start, 1020)));
        assert!(!flight(false, start).ended(after(start, 400)));
        assert!(
            flight(false, start).ended(after(start, 420)),
            "going back has no glow"
        );
    }
}
