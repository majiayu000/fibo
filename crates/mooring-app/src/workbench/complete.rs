//! 标记完成 from a pane's header. The animation is the one an `effects.json` plugin picks
//! (`mooring_core::effect`), the built-in seal otherwise: every frame is a pure function
//! of the run's progress `t`, drawn partly in the pane (button, veil, words, lids) and
//! partly on a window-wide overlay so it can leave the pane and land in the sidebar row.
//! 「留着这个画面」 keeps the pane open; the session stays completed either way.
//! Prototype of every style: specs/complete-animation/directions.html.
use super::layout::PaneAction;
use super::*;
use gpui::{Bounds, Hsla, PathBuilder, Pixels, Point, point, size};
use mooring_core::effect::{Complete, Style};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

mod cozy;
mod fun;
mod paint;
mod snapshot;
mod timing;
use paint::Painter;
use snapshot::Snapshot;
use timing::*;

/// The most specks the dissolve sends; a fuller screen sends one from every n-th cell,
/// though the wipe still clears every row.
const MAX_SPECKS: usize = 900;
/// When the dissolve reaches a spot: its text starts going at `START`, the last spot
/// `SPAN` later, the wave running mostly down the pane and a little across it; each speck
/// leaves up to `JITTER` after the wave.
const DISSOLVE_START: f32 = 0.2;
const DISSOLVE_SPAN: f32 = 0.3;
const DISSOLVE_JITTER: f32 = 0.04;
const DISSOLVE_DOWN: f32 = 0.72;
const DISSOLVE_ACROSS: f32 = 0.28;
/// The seal's line under 「做完了」: the session title, cut to the seal's width.
const SEAL_TITLE_MAX_W: f32 = 336.;
/// The fill bar along the bottom of the pane header while the button presses.
const PRESS_BAR_H: f32 = 2.;
/// How far above the pane body's center the seal sits.
const SEAL_LIFT: f32 = 14.;
/// One pane's run.
pub(super) struct Run {
    /// So a late timer from an earlier, kept run does not close the pane.
    seq: u64,
    start: Instant,
    effect: Complete,
    session: String,
    /// Where the dissolve's specks set off, and their colors. Shared with each frame's
    /// overlay rather than copied every frame.
    glyphs: Rc<[Glyph]>,
    /// The text itself, for the styles that move or erase it.
    text: Option<Rc<Snapshot>>,
    /// The session was in review under its turn bar when the run started.
    holds_turn_bar: bool,
    /// Held at one point of the run for screenshots.
    pinned: Option<f32>,
}
impl Run {
    fn t(&self) -> f32 {
        self.pinned.unwrap_or_else(|| {
            (self.start.elapsed().as_secs_f32() / self.effect.duration.as_secs_f32()).min(1.)
        })
    }
}

/// Each overlay paint: when it started and how long it took, for the frame-time check.
#[cfg(feature = "visual-test")]
pub(super) static SPECKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Paths and quads the overlay drew in its busiest frame since the last reset.
#[cfg(feature = "visual-test")]
pub(super) static PRIMITIVES: std::sync::Mutex<(usize, usize)> = std::sync::Mutex::new((0, 0));
#[cfg(feature = "visual-test")]
pub(super) static PAINTS: std::sync::Mutex<Vec<(Instant, std::time::Duration)>> =
    std::sync::Mutex::new(Vec::new());

/// A speck when the run started: its cell's middle in the window and its color.
type Glyph = (Point<Pixels>, Hsla);

/// Where the panes and sidebar rows were last drawn, recorded while painting so the
/// overlay, painted after them in the same frame, knows where to go.
#[derive(Default)]
pub(super) struct Spots {
    pub(super) panes: HashMap<String, Bounds<Pixels>>,
    pub(super) rows: HashMap<String, Bounds<Pixels>>,
}
pub(super) type SharedSpots = Rc<RefCell<Spots>>;

/// A canvas filling its parent that records the parent's bounds under `key`. An absolute
/// child here starts at the parent's content box whatever its insets say, though it takes
/// the padded size; `padding` moves it back onto the parent's edge.
pub(super) fn spot(
    spots: &SharedSpots,
    key: String,
    padding: (f32, f32),
    pick: fn(&mut Spots) -> &mut HashMap<String, Bounds<Pixels>>,
) -> impl IntoElement {
    let spots = spots.clone();
    gpui::canvas(
        move |bounds, _, _| {
            let edge = point(
                bounds.origin.x - px(padding.0),
                bounds.origin.y - px(padding.1),
            );
            pick(&mut spots.borrow_mut()).insert(key, Bounds::new(edge, bounds.size));
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// How far the dissolve's wipe has come along the row `down` the pane (0 at the top, 1 at
/// the bottom), as a fraction of the pane's width.
fn wipe_edge(t: f32, down: f32) -> f32 {
    let wave = (t - DISSOLVE_START - DISSOLVE_JITTER / 2.) / DISSOLVE_SPAN;
    ((wave - DISSOLVE_DOWN * down) / DISSOLVE_ACROSS).clamp(0., 1.)
}
/// Clears the terminal's rows as the dissolve passes them, wherever the grid is now and
/// whatever it shows, so no program's text or colored cells stay behind the specks.
fn dissolve_wipe(pane: Entity<Pane>, t: f32) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| (),
        move |body, _, window, cx| {
            let Some((grid, row, ground)) = pane.read(cx).grid_area() else {
                return;
            };
            let height = f32::from(body.size.height).max(1.);
            let mut top = grid.top();
            while top < grid.bottom() {
                let down = f32::from(top + row / 2. - body.top()) / height;
                let right = (body.left() + body.size.width * wipe_edge(t, down)).min(grid.right());
                if right > grid.left() {
                    window.paint_quad(gpui::fill(
                        Bounds::new(point(grid.left(), top), size(right - grid.left(), row)),
                        T::color(ground),
                    ));
                }
                top += row;
            }
        },
    )
    .absolute()
    .inset_0()
}
/// Where the sweep's light is across the body, as a fraction of its width.
fn sweep_at(t: f32) -> f32 {
    eio(sl(t, 0.05, 0.4)) * 1.3 - 0.15
}

impl Workbench {
    /// Settles a session from its card. 标记完成 from the card on the session's own pane
    /// folds the card and plays the animation picked in settings there, like the pane's
    /// button; every other ending, or a card in the overview, is only recorded.
    pub(super) fn settle_session(
        &mut self,
        session: &AgentSession,
        outcome: Outcome,
        in_pane: bool,
        cx: &mut Context<Self>,
    ) {
        let pane = (in_pane && outcome == Outcome::Completed)
            .then(|| live_terminal(&self.workspace, session))
            .flatten()
            .map(|t| t.id.clone())
            .filter(|t| self.layout.panes.contains_key(t));
        match pane {
            Some(terminal) => {
                self.overview.here = None;
                self.overview.here_more = None;
                self.complete_pane(terminal, session.id.clone(), cx)
            }
            None => self.edit(
                Edit::SetOutcome {
                    seen: None,
                    id: session.id.clone(),
                    outcome,
                },
                cx,
            ),
        }
    }
    /// Marks the pane's session completed and closes the pane once the animation ends,
    /// unless the user keeps it or the host turned the mark down.
    pub(super) fn complete_pane(
        &mut self,
        terminal: String,
        session: String,
        cx: &mut Context<Self>,
    ) {
        self.complete_pane_with(terminal, session, mooring_core::effect::complete(), cx);
    }
    pub(super) fn complete_pane_with(
        &mut self,
        terminal: String,
        session: String,
        effect: Complete,
        cx: &mut Context<Self>,
    ) {
        self.play_complete(terminal, session, effect, false, cx);
    }
    /// Plays `effect` (None: the animation 标记完成 would) on the focused pane without
    /// marking or closing anything; false when no pane is showing.
    pub(super) fn preview_complete(
        &mut self,
        effect: Option<Complete>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(terminal) = self
            .nav
            .focused
            .clone()
            .filter(|id| self.layout.panes.contains_key(id))
        else {
            return false;
        };
        let session = self
            .pane_session(&terminal)
            .map(|s| s.id.clone())
            .unwrap_or_default();
        let mut effect = effect.map_or_else(mooring_core::effect::complete, |e| {
            e.played(mooring_core::effect::reduce_motion())
        });
        // Without a session there is no sidebar row to land in.
        effect.land &= !session.is_empty();
        self.play_complete(terminal, session, effect, true, cx);
        true
    }
    fn play_complete(
        &mut self,
        terminal: String,
        session: String,
        effect: Complete,
        preview: bool,
        cx: &mut Context<Self>,
    ) {
        self.layout.completing_seq += 1;
        let seq = self.layout.completing_seq;
        let glyphs = match (effect.style, self.layout.panes.get(&terminal)) {
            (Style::Dissolve, Some(pane)) => pane.read(cx).glyph_cells(),
            _ => vec![],
        };
        let step = glyphs.len().div_ceil(MAX_SPECKS).max(1);
        let glyphs: Vec<_> = glyphs
            .into_iter()
            .step_by(step)
            .map(|(p, c)| (p, T::color(c)))
            .collect();
        let text = self.layout.panes.get(&terminal).and_then(|pane| {
            let title = self.pane_session(&terminal).map(|s| self.session_title(s));
            Snapshot::of(effect.style, pane.read(cx), title.as_deref(), cx).map(Rc::new)
        });
        #[cfg(feature = "visual-test")]
        SPECKS.store(
            text.as_ref().map_or(glyphs.len(), |s| s.letters.len()),
            std::sync::atomic::Ordering::Relaxed,
        );
        let holds_turn_bar = matches!(self.pane_status(&terminal), Some((Status::Review, _)))
            && self
                .pane_session(&terminal)
                .is_some_and(|s| s.outcome == Outcome::Open);
        self.layout.completing.insert(
            terminal.clone(),
            Run {
                seq,
                start: Instant::now(),
                effect,
                session: session.clone(),
                glyphs: glyphs.into(),
                text,
                holds_turn_bar,
                pinned: None,
            },
        );
        if !preview {
            self.edit(
                Edit::SetOutcome {
                    seen: None,
                    id: session.clone(),
                    outcome: Outcome::Completed,
                },
                cx,
            );
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(effect.duration).await;
            let _ = this.update(cx, |this, cx| {
                match this.layout.completing.get(&terminal) {
                    Some(run) if run.seq == seq && run.pinned.is_none() => {}
                    _ => return,
                }
                this.layout.completing.remove(&terminal);
                let done = !preview
                    && this
                        .workspace
                        .agent_sessions
                        .iter()
                        .any(|s| s.id == session && s.outcome == Outcome::Completed);
                if done {
                    this.close_pane(&terminal, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// The run's style and progress on this pane, if one is playing.
    pub(super) fn completing(&self, terminal: &str) -> Option<(Style, f32)> {
        self.layout
            .completing
            .get(terminal)
            .map(|r| (r.effect.style, r.t()))
    }
    /// Whose turn bar the pane shows, and in which state: a session in review, or for a
    /// run that began under the bar the same bar to the end, though the session has left
    /// review, since taking it away would resize the terminal and move its text mid-run.
    pub(super) fn shown_turn_bar<'a>(
        &self,
        terminal: &str,
        session: Option<&'a AgentSession>,
        review: Option<&'a AgentSession>,
        status: Option<Status>,
    ) -> Option<(&'a AgentSession, Status)> {
        match self.layout.completing.get(terminal) {
            Some(run) => session
                .filter(|_| run.holds_turn_bar)
                .map(|s| (s, Status::Review)),
            None => review.zip(status).filter(|(_, g)| *g == Status::Review),
        }
    }
    /// Whether a run is landing `session` in the sidebar, so its row records its spot.
    pub(super) fn marking(&self, session: &str) -> bool {
        self.layout
            .completing
            .values()
            .any(|r| r.effect.land && r.session == session)
    }
    /// Holds a run at `t` for screenshots, or lets it go on.
    #[cfg(feature = "visual-test")]
    pub(super) fn pin_completing(&mut self, terminal: &str, t: Option<f32>) {
        if let Some(run) = self.layout.completing.get_mut(terminal) {
            run.pinned = t;
        }
    }
    /// How much of the whole pane still shows at `t`.
    pub(super) fn completing_opacity(style: Style, t: f32) -> f32 {
        let (a, b) = timing(style).fade;
        match style {
            Style::Seal | Style::Sweep => 1. - sl(t, a, b),
            _ => 1. - eo(sl(t, a, b)),
        }
    }
    /// The header button mid-run: a fill sweeps in, the words turn to a check, a ring
    /// spreads out and fades.
    pub(super) fn completing_button(&self, id: &str, style: Style, t: f32) -> gpui::AnyElement {
        let (dot, _, _) = Tone::Ok.colors();
        let end = timing(style).button;
        let fill = eo(sl(t, 0., end));
        let turned = sl(t, end * 0.45, end);
        let ripple = eo(sl(t, end * 0.6, end * 3.));
        let out = px(-10. * ripple);
        div()
            .id(gpui::SharedString::from(format!("accept-pane-{id}")))
            .relative()
            .flex_shrink_0()
            .h(px(T::PANE_ACTION))
            .px(px(T::S6))
            .flex()
            .items_center()
            .rounded(px(T::PANE_ACTION_RADIUS))
            .bg(T::color(T::button()))
            .text_size(px(T::SMALL))
            .when(ripple > 0. && ripple < 1., |d| {
                d.child(
                    div()
                        .absolute()
                        .top(out)
                        .bottom(out)
                        .left(out)
                        .right(out)
                        .rounded(px(T::PANE_ACTION_RADIUS + 10. * ripple))
                        .border(px(T::FOCUS_RING))
                        .border_color(dot.opacity((1. - ripple) * 0.8)),
                )
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(T::PANE_ACTION_RADIUS))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(gpui::relative(fill))
                            .bg(dot),
                    ),
            )
            .child(
                div()
                    .relative()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(if turned > 0.5 {
                        T::color(T::on_accent())
                    } else {
                        T::color(T::primary())
                    })
                    .opacity(if turned > 0.5 { turned } else { 1. - turned })
                    .child(if turned > 0.5 {
                        format!("✓ {}", Status::Closed.label())
                    } else {
                        "标记完成".to_owned()
                    }),
            )
            .into_any_element()
    }
    /// What the run draws inside the pane: the veil, the words with a way to keep the
    /// pane, the sweep's wash and lids, and a line along the header counting down.
    pub(super) fn completing_veil(
        &self,
        id: &str,
        style: Style,
        t: f32,
        title: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let (dot, ink, _) = Tone::Ok.colors();
        let pane = T::color(T::pane());
        let behind = if crate::ui::style::current(cx) == crate::ui::style::Style::Terminal {
            T::app()
        } else {
            T::pane()
        };
        let tm = timing(style);
        let keep = id.to_owned();
        let (a, b, c, d) = tm.words;
        let shown = eo(sl(t, a, b)) * if c > 0. { 1. - sl(t, c, d) } else { 1. };
        let veil = match style {
            Style::Seal => 0.88 * eo(sl(t, 0.1, 0.35)),
            Style::Stamp => 0.86 * eo(sl(t, 0.06, 0.28)),
            Style::Sweep => 0.8,
            Style::Dissolve => 0.25 * sl(t, 0.05, 0.2) + 0.65 * sl(t, 0.35, 0.65),
            Style::Fade | Style::Gravity | Style::Swirl | Style::Backspace | Style::Crt => 0.,
            Style::Moor => 0.86 * eo(sl(t, 0.08, 0.28)),
            Style::Confetti => 0.86 * eo(sl(t, 0.08, 0.26)),
            _ => 0.88 * eo(sl(t, 0.05, 0.2)),
        };
        let veil_width = if style == Style::Sweep {
            sweep_at(t).clamp(0., 1.)
        } else {
            1.
        };
        let lids = match style {
            Style::Sweep => eio(sl(t, 0.66, 0.84)),
            Style::Crt => fun::crt_lids(t),
            _ => 0.,
        };
        let countdown = (t / tm.fade.0).min(1.);
        let words = div()
            .absolute()
            .left_0()
            .right_0()
            .top(gpui::relative(0.5))
            .mt(px(if matches!(style, Style::Sweep | Style::Crt) {
                34.
            } else {
                42.
            } + T::S8 * (1. - shown)))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(T::S4))
            .opacity(shown)
            // 退格收工 types these words on the terminal's first line itself.
            .when(style != Style::Backspace, |d| {
                d.child(
                    div()
                        .text_size(px(T::BODY_SIZE))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(ink)
                        .child(style.greeting().map_or_else(
                            || Status::Closed.label().to_owned(),
                            |g| format!("{}，{g}", Status::Closed.label()),
                        )),
                )
                .child(
                    div()
                        .max_w(px(SEAL_TITLE_MAX_W))
                        .truncate()
                        .text_size(px(T::META))
                        .text_color(T::color(T::secondary()))
                        .child(title.to_owned()),
                )
            })
            .child(
                button(
                    gpui::SharedString::from(format!("keep-pane-{id}")),
                    "留着这个画面",
                )
                .pane_action()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.layout.completing.remove(&keep);
                    cx.notify();
                })),
            );
        // The bar a run holds and the terminal under the veil are no longer to be used.
        let body = div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(T::PANE_HEADER))
            .bottom_0()
            .overflow_hidden()
            .occlude()
            .when_some(
                self.layout
                    .panes
                    .get(id)
                    .filter(|_| style == Style::Dissolve)
                    .cloned(),
                |d, pane| d.child(dissolve_wipe(pane, t)),
            )
            .when_some(
                self.layout
                    .panes
                    .get(id)
                    .cloned()
                    .zip(self.layout.completing.get(id).and_then(|r| r.text.clone())),
                |d, (pane, text)| d.child(snapshot::text_wipe(pane, style, t, text)),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(gpui::relative(veil_width))
                    .bg(pane.opacity(veil)),
            )
            .when(lids > 0., |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top_0()
                        .h(gpui::relative(0.5 * lids))
                        .bg(pane),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(gpui::relative(0.5 * lids))
                        .bg(pane),
                )
            })
            .when(shown > 0.01, |d| d.child(words));
        div()
            .absolute()
            .inset_0()
            .when(style == Style::Sweep, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .h(px(T::PANE_HEADER))
                        .w(gpui::relative(sweep_at(t).clamp(0., 1.)))
                        .bg(dot.opacity(0.14)),
                )
            })
            .child(body)
            .when(style != Style::Fade, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(T::PANE_HEADER - PRESS_BAR_H))
                        .h(px(PRESS_BAR_H))
                        .w(gpui::relative(countdown))
                        .bg(dot),
                )
            })
            // The pane fades by a cover in what is behind it, not by opacity: GPUI applies
            // opacity to each element alone, so a fading veil or lid lets the text it was
            // hiding show through again.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(T::color(behind).opacity(1. - Self::completing_opacity(style, t))),
            )
    }
    /// The part of every run that is not confined to its pane: seals, rings, specks and
    /// the light, drawn over the whole window and landing in the session's sidebar row.
    pub(super) fn completing_overlay(&self, window: &Window) -> Option<impl IntoElement> {
        if self.layout.completing.is_empty() {
            return None;
        }
        window.request_animation_frame();
        struct Frame {
            terminal: String,
            session: String,
            effect: Complete,
            t: f32,
            glyphs: Rc<[Glyph]>,
            text: Option<Rc<Snapshot>>,
        }
        let frames: Vec<Frame> = self
            .layout
            .completing
            .iter()
            .map(|(terminal, r)| Frame {
                terminal: terminal.clone(),
                session: r.session.clone(),
                effect: r.effect,
                t: r.t(),
                glyphs: r.glyphs.clone(),
                text: r.text.clone(),
            })
            .collect();
        let spots = self.layout.spots.clone();
        let (green, _, _) = Tone::Ok.colors();
        let on_accent = T::color(T::on_accent());
        let (ink, warm, rose, soft) = (
            T::color(T::primary()),
            Tone::Warn.dot(),
            Tone::Danger.dot(),
            T::color(T::secondary()),
        );
        Some(
            gpui::canvas(
                |_, _, _| (),
                move |_, _, window, cx| {
                    let mono = T::mono(cx);
                    #[cfg(feature = "visual-test")]
                    let started = Instant::now();
                    let spots = spots.borrow();
                    for f in &frames {
                        let Some(pane) = spots.panes.get(&f.terminal).copied() else {
                            continue;
                        };
                        let row = spots.rows.get(&f.session).copied();
                        let target = row.filter(|_| f.effect.land).map(|r| {
                            point(
                                r.left() + px(T::S8 + T::DOT / 2.),
                                r.top() + px(T::ROW / 2.),
                            )
                        });
                        let mut p = Painter {
                            window,
                            cx,
                            mono: mono.clone(),
                            clip: None,
                            paths: 0,
                            quads: 0,
                            green,
                            on_accent,
                            ink,
                            warm,
                            rose,
                            soft,
                        };
                        p.frame(
                            f.effect.style,
                            f.t,
                            pane,
                            target,
                            &f.glyphs,
                            f.text.as_deref(),
                        );
                        if let (Some(r), Some(at)) = (row.filter(|_| f.effect.land), target) {
                            p.landed(f.t, timing(f.effect.style).land, r, at);
                        }
                        #[cfg(feature = "visual-test")]
                        if let Ok(mut most) = PRIMITIVES.lock() {
                            *most = (most.0.max(p.paths), most.1.max(p.quads));
                        }
                    }
                    #[cfg(feature = "visual-test")]
                    if let Ok(mut log) = PAINTS.lock() {
                        log.push((started, started.elapsed()));
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_and_easing_stay_in_range() {
        assert_eq!(sl(0.1, 0.2, 0.4), 0.);
        assert!((sl(0.3, 0.2, 0.4) - 0.5).abs() < 1e-5);
        assert_eq!(sl(0.9, 0.2, 0.4), 1.);
        assert_eq!(eo(1.), 1.);
        assert_eq!(eio(0.), 0.);
        assert!((eio(1.) - 1.).abs() < 1e-5);
        assert!((back_out(1.) - 1.).abs() < 1e-5);
        // The stamp overshoots before it settles.
        assert!(back_out(0.7) > 1.);
    }

    #[test]
    fn every_style_ends_with_the_pane_gone() {
        for style in Style::ALL {
            assert_eq!(
                Workbench::completing_opacity(style, 0.),
                1.,
                "{style:?} starts shown"
            );
            assert!(
                Workbench::completing_opacity(style, 1.) < 1e-4,
                "{style:?} ends gone"
            );
            let tm = timing(style);
            assert!(
                tm.button < tm.fade.0,
                "{style:?}: the button turns before the fade"
            );
        }
    }

    #[test]
    fn the_wipe_clears_every_row_before_the_veil_and_fade() {
        // Nothing before it starts; every spot once the last speck has left.
        assert_eq!(wipe_edge(DISSOLVE_START, 0.), 0.);
        let done = DISSOLVE_START + DISSOLVE_SPAN + DISSOLVE_JITTER;
        for down in [0., 0.5, 1.] {
            assert_eq!(wipe_edge(done, down), 1.);
        }
        assert!(done < timing(Style::Dissolve).fade.0);
        // Top rows go first.
        let mid = DISSOLVE_START + DISSOLVE_SPAN / 2.;
        assert!(wipe_edge(mid, 0.) > wipe_edge(mid, 0.9));
    }

    #[test]
    fn the_same_seed_draws_the_same_frame() {
        let (mut a, mut b) = (rng(7), rng(7));
        for _ in 0..50 {
            let x = a();
            assert_eq!(x, b());
            assert!((0. ..1.).contains(&x));
        }
    }
}
