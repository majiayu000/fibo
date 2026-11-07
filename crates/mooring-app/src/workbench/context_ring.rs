//! A small ring saying how full a conversation's context window is, where the conversation
//! is shown: nothing below `SHOWN_FROM`, amber from `NEAR`, the numbers on hover.
use super::*;
use mooring_core::context_fill::Fill;

const RING: f32 = 12.;
const STROKE: f32 = 2.;
/// Straight pieces in a whole circle: at this size they read as a curve.
const SEGMENTS: usize = 48;

impl Workbench {
    pub(super) fn context_ring(&self, session: &AgentSession) -> Option<gpui::Stateful<gpui::Div>> {
        let now = mooring_core::content::now();
        let fills = super::redraw::context_fills(&self.workspace, now);
        let fill = *fills.get(&session.id).filter(|f| f.shown())?;
        let text: gpui::SharedString = fill.text().into();
        let tip = text.clone();
        Some(
            div()
                .id(gpui::SharedString::from(format!("context-{}", session.id)))
                .flex_shrink_0()
                .size(px(RING))
                .aria_label(text)
                .tooltip(move |_, cx| cx.new(|_| super::render::TextTip(tip.clone())).into())
                .child(ring(fill)),
        )
    }
}

fn ring(fill: Fill) -> gpui::Canvas<()> {
    let color = if fill.near() {
        Tone::Warn.dot()
    } else {
        T::color(T::secondary())
    };
    let share = f32::from(fill.percent()) / 100.;
    gpui::canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            arc(bounds, 1., T::color(T::chart_grid()), window);
            arc(bounds, share, color, window);
        },
    )
    .size_full()
}

/// `share` of the circle, clockwise from twelve o'clock.
fn arc(bounds: gpui::Bounds<gpui::Pixels>, share: f32, color: gpui::Hsla, window: &mut Window) {
    let pieces = ((SEGMENTS as f32 * share).ceil() as usize).max(1);
    let middle = bounds.center();
    let radius = (RING - STROKE) / 2.;
    let at = |i: usize| {
        let turn = share * i as f32 / pieces as f32;
        let angle = turn * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        middle + gpui::point(px(radius * angle.cos()), px(radius * angle.sin()))
    };
    let mut path = gpui::PathBuilder::stroke(px(STROKE));
    path.move_to(at(0));
    for i in 1..=pieces {
        path.line_to(at(i));
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}
