//! A run drawn small, for 设置 › 插件: the same `Painter` as on a pane, over a made-up
//! terminal and sidebar row. Nothing here keeps the window drawing unless it is playing,
//! and a card only plays while you point at it.
use super::*;
use crate::terminal::glyph::TextCell;
use crate::terminal::screen::{Attrs, CellMetrics};
use snapshot::{Faces, paint_wipe};
use std::time::Duration;

/// What the made-up terminal shows.
const LINES: [&str; 5] = [
    "$ cargo test",
    "test result: ok",
    "728 passed",
    "$ git push",
    "main -> main",
];
/// The sidebar's share of the stage, and what its row looks like.
const SIDE_SHARE: f32 = 0.16;
const SIDE_ROW_H: f32 = 18.;
const SIDE_ROW_INSET: f32 = 4.;
const SIDE_DOT: f32 = 3.;
const SIDE_BAR_H: f32 = 3.;
/// The title bar over the made-up terminal.
const HEADER: f32 = T::PANE_HEADER * 0.5;
/// How much further up a real pane's header would start: the painter counts from there.
const HEADER_SAVED: f32 = T::PANE_HEADER - HEADER;
/// How far under the middle the words sit, and how big, on a stage this short and taller.
const WORDS_DROP: f32 = 0.3;
const WORDS_SMALL: f32 = 10.;
const WORDS_LARGE: f32 = 12.;
const WORDS_LARGE_FROM: f32 = 160.;
/// A beat between loops, so the end of one is seen before the next starts.
const REST: Duration = Duration::from_millis(700);

/// Where a stage is in its run.
#[derive(Clone, Copy)]
pub(in crate::workbench) enum Play {
    /// Started at this time and stops at its end.
    Once(Instant),
    /// Started at this time and goes round for as long as it is drawn this way.
    Loop(Instant),
}

/// The moment a stopped card shows: where each style looks most like itself.
fn pose(style: Style) -> f32 {
    match style {
        Style::Relief | Style::Tea => 0.62,
        Style::Wake | Style::Sunrise => 0.7,
        Style::Heart => 0.66,
        Style::Cat | Style::Moor => 0.6,
        Style::Fade => 0.5,
        Style::Hop => 0.3,
        Style::Gravity => 0.52,
        Style::Swirl | Style::Dissolve | Style::Sweep | Style::Confetti => 0.42,
        Style::Stamp => 0.3,
        Style::Backspace => 0.3,
        Style::Crt => 0.38,
        Style::Seal => 0.55,
    }
}

/// For the performance probe: the moment every playing stage shows, so the same frames are
/// measured each run, and how many times a stage has asked for another frame.
#[cfg(feature = "visual-test")]
pub(in crate::workbench) mod probe {
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    const FREE: u32 = u32::MAX;
    static HELD: AtomicU32 = AtomicU32::new(FREE);
    pub(in crate::workbench) static ASKED: AtomicUsize = AtomicUsize::new(0);
    pub(in crate::workbench) fn hold(t: Option<f32>) {
        HELD.store(t.map_or(FREE, f32::to_bits), Ordering::Relaxed);
    }
    pub(super) fn held() -> Option<f32> {
        match HELD.load(Ordering::Relaxed) {
            FREE => None,
            bits => Some(f32::from_bits(bits)),
        }
    }
}

/// `t`, and whether another frame is wanted after this one.
fn moment(style: Style, run: Duration, play: Play) -> (f32, bool) {
    #[cfg(feature = "visual-test")]
    if let Some(t) = probe::held() {
        return (t, true);
    }
    let progress = |since: Duration| since.as_secs_f32() / run.as_secs_f32().max(f32::EPSILON);
    if mooring_core::effect::reduce_motion() {
        return (pose(style), false);
    }
    match play {
        Play::Once(from) if from.elapsed() >= run => (pose(style), false),
        Play::Once(from) => (progress(from.elapsed()), true),
        Play::Loop(from) => {
            let round = (run + REST).as_secs_f32();
            let since = from.elapsed().as_secs_f32() % round;
            (progress(Duration::from_secs_f32(since)).min(1.), true)
        }
    }
}

thread_local! {
    /// Shaped characters, kept across frames and stages: there are few, and only one size.
    static FACES: (std::cell::Cell<Pixels>, Faces) = Default::default();
}

/// Where the card to play is on screen and what it may draw in, as the page last laid it out.
pub(in crate::workbench) type StageSpot =
    Rc<std::cell::Cell<Option<(Bounds<Pixels>, Bounds<Pixels>)>>>;

/// A stopped stage, as wide as its parent and as tall as it is given. With `spot` it also
/// says where it is, for the `Player` drawing over it.
pub(in crate::workbench) fn stage(complete: Complete, spot: Option<StageSpot>) -> impl IntoElement {
    gpui::canvas(
        move |bounds, window, _| {
            if let Some(spot) = &spot {
                spot.set(Some((bounds, window.content_mask().bounds)));
            }
        },
        move |bounds, _, window, cx| {
            let t = pose(complete.style);
            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                paint(complete, t, bounds, window, cx);
            });
        },
    )
    .size_full()
}

/// The one stage playing, drawn over the page instead of inside it. Notifying this view
/// redraws it and the workbench, while settings, its layout included, is reused: laying
/// out a page of cards every frame was 62 M instructions, 80% of such a frame.
pub(in crate::workbench) struct Player {
    pub(in crate::workbench) now: Option<(Complete, Play)>,
    pub(in crate::workbench) spot: StageSpot,
}
impl Render for Player {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (now, spot) = (self.now, self.spot.clone());
        gpui::canvas(
            |_, _, _| (),
            move |_, _, window, cx| {
                let (Some((complete, play)), Some((bounds, clip))) = (now, spot.get()) else {
                    return;
                };
                let (t, more) = moment(complete.style, complete.duration, play);
                let shown = bounds.intersect(&clip);
                window.with_content_mask(Some(gpui::ContentMask { bounds: shown }), |window| {
                    paint(complete, t, bounds, window, cx);
                });
                if more {
                    #[cfg(feature = "visual-test")]
                    probe::ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .size_full()
    }
}

fn cells_of(grid: Bounds<Pixels>, metrics: CellMetrics, color: Hsla) -> Vec<TextCell> {
    let mut cells = Vec::new();
    for (y, line) in LINES.iter().enumerate() {
        let top = grid.top() + metrics.height * y as f32;
        if top + metrics.height > grid.bottom() {
            break;
        }
        for (x, ch) in line.chars().enumerate() {
            let left = grid.left() + metrics.width * x as f32;
            if ch == ' ' || left + metrics.width > grid.right() {
                continue;
            }
            cells.push(TextCell {
                ch,
                at: point(left, top),
                cols: 1,
                color,
                attrs: Attrs::default(),
            });
        }
    }
    cells
}

fn paint(
    complete: Complete,
    progress: f32,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let style = complete.style;
    let (pane_color, side_color) = (T::color(T::pane()), T::color(T::app()));
    let fg = T::color(T::primary());
    let dim = T::color(T::secondary());
    let side_w = (bounds.size.width * SIDE_SHARE).round();
    window.paint_quad(gpui::fill(bounds, side_color));
    let pane = Bounds::new(
        point(bounds.left() + side_w, bounds.top()),
        size(bounds.size.width - side_w, bounds.size.height),
    );
    window.paint_quad(gpui::fill(pane, pane_color));
    let row = Bounds::new(
        point(
            bounds.left() + px(SIDE_ROW_INSET),
            bounds.top() + px(HEADER),
        ),
        size(side_w - px(SIDE_ROW_INSET * 2.), px(SIDE_ROW_H)),
    );
    let dot = point(row.left() + px(SIDE_ROW_INSET + SIDE_DOT), row.center().y);
    let head = Bounds::new(pane.origin, size(pane.size.width, px(HEADER)));
    window.paint_quad(gpui::fill(head, T::color(T::chip())));
    let body = Bounds::new(
        point(pane.left(), pane.top() + px(HEADER)),
        size(pane.size.width, pane.size.height - px(HEADER)),
    );
    let inset = px(T::S4);
    let grid = Bounds::new(
        point(body.left() + inset, body.top() + inset),
        size(body.size.width - inset * 2., body.size.height - inset * 2.),
    );
    let Some(metrics) = CellMetrics::measure(&T::terminal_mono(cx), window) else {
        return;
    };
    let cells = cells_of(grid, metrics, dim);
    let glyphs: Vec<Glyph> = cells.iter().map(|c| (c.at, c.color)).collect();
    // A new terminal font means every shape is stale.
    let faces = FACES.with(|(width, faces)| {
        if width.replace(metrics.width) != metrics.width {
            faces.borrow_mut().clear();
        }
        faces.clone()
    });
    let text = Snapshot::from_cells(
        style,
        cells_of(grid, metrics, dim),
        None,
        (grid, metrics.height),
        metrics,
        fg,
        Some(t!("修登录")),
        faces,
        cx,
    );
    for c in &cells {
        text.draw_char(c.ch, c.cols, c.attrs, c.at, c.color, window);
    }
    if style == Style::Dissolve {
        for i in 0..LINES.len() {
            let top = grid.top() + metrics.height * i as f32;
            let down = f32::from(top - body.top()) / f32::from(body.size.height).max(1.);
            let w = body.size.width * wipe_edge(progress, down);
            window.paint_quad(gpui::fill(
                Bounds::new(point(body.left(), top), size(w, metrics.height)),
                pane_color,
            ));
        }
    }
    paint_wipe(
        style,
        progress,
        &text,
        grid,
        metrics.height,
        pane_color,
        window,
    );
    let veil_w = if style == Style::Sweep {
        sweep_at(progress).clamp(0., 1.)
    } else {
        1.
    };
    window.paint_quad(gpui::fill(
        Bounds::new(
            body.origin,
            size(body.size.width * veil_w, body.size.height),
        ),
        pane_color.opacity(veil_amount(style, progress)),
    ));
    let lids = lids_amount(style, progress);
    if lids > 0. {
        let h = body.size.height * (0.5 * lids);
        window.paint_quad(gpui::fill(
            Bounds::new(body.origin, size(body.size.width, h)),
            pane_color,
        ));
        window.paint_quad(gpui::fill(
            Bounds::new(
                point(body.left(), body.bottom() - h),
                size(body.size.width, h),
            ),
            pane_color,
        ));
    }
    // The pane fades by a cover in what is behind it, as on a real pane.
    window.paint_quad(gpui::fill(
        pane,
        side_color.opacity(1. - Workbench::completing_opacity(style, progress)),
    ));
    window.paint_quad(gpui::fill(
        Bounds::new(
            point(row.left(), row.center().y - px(SIDE_BAR_H / 2.)),
            size(row.size.width, px(SIDE_BAR_H)),
        ),
        dim.opacity(0.35),
    ));
    let (green, _, _) = Tone::Ok.colors();
    let target = complete.land.then_some(dot);
    let mono = T::mono(cx);
    let mut p = Painter {
        window,
        cx,
        mono,
        face: complete.face,
        small: true,
        clip: None,
        paths: 0,
        quads: 0,
        green,
        on_accent: T::color(T::on_accent()),
        ink: fg,
        warm: Tone::Warn.dot(),
        rose: Tone::Danger.dot(),
        soft: dim,
    };
    let drawn_from = Bounds::new(
        point(pane.left(), pane.top() - px(HEADER_SAVED)),
        size(pane.size.width, pane.size.height + px(HEADER_SAVED)),
    );
    p.frame(style, progress, drawn_from, target, &glyphs, Some(&text));
    if let Some(at) = target {
        p.landed(progress, timing(style).land, row, at);
    }
    let tm = timing(style);
    let (a, b, c, d) = tm.words;
    let shown = eo(sl(progress, a, b)) * if c > 0. { 1. - sl(progress, c, d) } else { 1. };
    if shown > 0.01 && style != Style::Backspace {
        let words = style.greeting().map_or_else(
            || Status::Closed.label().to_owned(),
            |g| t!("{done}，{g}", done = Status::Closed.label(), g),
        );
        let tall = f32::from(body.size.height) >= WORDS_LARGE_FROM;
        p.text(
            point(
                body.center().x,
                body.center().y + body.size.height * WORDS_DROP,
            ),
            words,
            if tall { WORDS_LARGE } else { WORDS_SMALL },
            green.opacity(shown),
        );
    }
}
