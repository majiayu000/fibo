//! 先不管这台's flight: without it a machine just vanishes and a gray line appears at the
//! foot, and the eye does not connect them. A pill with its name travels between where the
//! machine was and its line, then the line glows once. It ends like all the column's
//! motion (see `sidebar_motion`): nothing is left drawn and nothing asks for a frame.
use super::*;

const FLY: f32 = 420.;
const GLOW: f32 = 600.;
const PILL: f32 = 26.;
/// The pill shows and lands over this much of its flight.
const FADE: f32 = 70.;

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4. * t * t * t
    } else {
        1. - (2. - 2. * t).powi(3) / 2.
    }
}

pub(super) struct Flight {
    host: String,
    words: String,
    letter: String,
    /// Toward the gray line, rather than back from it.
    leaving: bool,
    from: f32,
    /// Where it lands; the line's place is known once the line has been drawn.
    to: Option<f32>,
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
    /// The pill's top, from `from` to where it lands.
    fn top(&self, to: f32, now: Instant) -> f32 {
        self.from + (to - self.from) * ease_in_out(part(self.elapsed(now), FLY))
    }
    fn opacity(&self, now: Instant) -> f32 {
        let e = self.elapsed(now);
        part(e, FADE).min(part(FLY - e, FADE))
    }
    /// The gray line under this flight: how shown it is, and how much it glows. It comes
    /// up as the pill lands in it.
    fn line(&self, now: Instant) -> (f32, f32) {
        let e = self.elapsed(now);
        (
            part(e - FLY + FADE / 3., FADE),
            1. - ease_out(part(e - FLY, GLOW)),
        )
    }
}

/// Where each quiet machine's line was last drawn, and where you left each machine from.
#[derive(Default)]
pub(in crate::workbench) struct Seats {
    lines: Rc<RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>>,
    left_from: HashMap<String, f32>,
}

impl SidebarMotion {
    /// Starts the pill for `host`. From a menu it starts where that menu was opened;
    /// from anywhere else, at the top of the list.
    pub(in crate::workbench) fn fly(
        &mut self,
        host: &str,
        name: &str,
        leaving: bool,
        menu: Option<f32>,
    ) {
        if mooring_core::effect::reduce_motion() {
            return;
        }
        let list_top = F::WIDTH * 0.4;
        let line = (self.seats.lines.borrow().get(host))
            .map(|b| f32::from(b.origin.y) + (f32::from(b.size.height) - PILL) / 2.);
        // Turning back midway starts from where the pill is.
        let now = clock();
        let midway = (self.flight.as_ref())
            .filter(|f| f.host == host && !f.ended(now))
            .and_then(|f| Some(f.top(f.to.or(line)?, now)));
        let (from, to) = if leaving {
            let from = menu.map_or(list_top, |y| y - PILL / 2.);
            self.seats.left_from.insert(host.to_owned(), from);
            (midway.unwrap_or(from), line)
        } else {
            let Some(from) = midway.or(line) else {
                return;
            };
            let back = self.seats.left_from.get(host).copied();
            (from, Some(back.unwrap_or(list_top)))
        };
        self.flight = Some(Flight {
            host: host.to_owned(),
            words: format!("{name} · {}", if leaving { "先不管" } else { "接着管" }),
            letter: super::super::sidebar_machines::machine_letter(name),
            leaving,
            from,
            to,
            started: now,
        });
    }
    /// How much the line for `host` shows and glows now.
    pub(in crate::workbench) fn line_state(&self, host: &str) -> (f32, f32) {
        (self.flight.as_ref())
            .filter(|f| f.leaving && f.host == host)
            .map_or((1., 0.), |f| f.line(clock()))
    }
    /// Draws nothing but remembers where the line for `host` is.
    pub(in crate::workbench) fn line_spot(&self, host: &str) -> gpui::Canvas<()> {
        let (lines, host) = (self.seats.lines.clone(), host.to_owned());
        gpui::canvas(
            move |bounds, _, _| {
                lines.borrow_mut().insert(host, bounds);
            },
            |_, _, _, _| (),
        )
    }
    /// The pill, while it travels, `left` from the column's edge.
    pub(in crate::workbench) fn pill(&self, left: f32) -> Option<gpui::Div> {
        let f = self.flight.as_ref()?;
        let now = clock();
        if f.elapsed(now) >= FLY {
            return None;
        }
        // The line's first frame has yet to be drawn: wait there.
        let line = self
            .seats
            .lines
            .borrow()
            .get(&f.host)
            .map(|b| f32::from(b.origin.y) + (f32::from(b.size.height) - PILL) / 2.);
        let to = f.to.or(line).unwrap_or(f.from);
        Some(
            div()
                .absolute()
                .left(px(left + T::S10))
                .top(px(f.top(to, now)))
                .h(px(PILL))
                .pl(px(T::S6))
                .pr(px(T::S10))
                .flex()
                .items_center()
                .gap(px(T::S6))
                .opacity(f.opacity(now))
                .rounded(px(PILL / 2.))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(F::chip_on_line()))
                .bg(T::color(T::raised()))
                .shadow_lg()
                .font_family(T::UI_FONT)
                .text_size(px(F::TAG))
                .text_color(T::color(F::bright()))
                .child(super::super::sidebar_machines::machine_mark(
                    f.letter.clone(),
                    false,
                ))
                .child(f.words.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flight(leaving: bool, started: Instant) -> Flight {
        Flight {
            host: "me@apple".into(),
            words: String::new(),
            letter: "A".into(),
            leaving,
            from: 200.,
            to: Some(700.),
            started,
        }
    }
    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn the_pill_goes_all_the_way_without_turning_back() {
        let start = Instant::now();
        let f = flight(true, start);
        let tops: Vec<f32> = (0..=42).map(|i| f.top(700., at(start, i * 10))).collect();
        assert_eq!(tops[0], 200.);
        assert!((tops[42] - 700.).abs() < 0.01, "{}", tops[42]);
        assert!(tops.windows(2).all(|w| w[1] >= w[0]), "{tops:?}");
    }
    #[test]
    fn the_line_comes_up_as_the_pill_lands_then_stops_glowing() {
        let start = Instant::now();
        let f = flight(true, start);
        assert_eq!(f.line(at(start, 380)).0, 0., "its words crossed the pill's");
        assert_eq!(f.opacity(at(start, 200)), 1., "the pill is faint in flight");
        assert_eq!(
            f.line(at(start, 420)).1,
            1.,
            "the glow starts when it lands"
        );
        assert_eq!(
            f.line(at(start, 470)).0,
            1.,
            "the line stayed faint after landing"
        );
        assert_eq!(f.line(at(start, 1020)).1, 0., "the glow never ends");
    }
    #[test]
    fn it_ends_so_the_column_stops_asking_for_frames() {
        let start = Instant::now();
        assert!(!flight(true, start).ended(at(start, 1000)));
        assert!(flight(true, start).ended(at(start, 1020)));
        assert!(!flight(false, start).ended(at(start, 400)));
        assert!(
            flight(false, start).ended(at(start, 420)),
            "going back has no glow"
        );
    }
}
