use super::*;
use gpui::{Bounds, Pixels, point, size};

const HEIGHT: f32 = 30.;
const VIEW: f32 = 300.;

/// A strip `VIEW` wide holding tabs of these widths side by side, from its left edge.
fn strip(widths: &[f32]) -> (Bounds<Pixels>, Vec<Bounds<Pixels>>) {
    let origin = point(px(0.), px(0.));
    let mut x = 0.;
    let tabs = widths
        .iter()
        .map(|w| {
            let tab = Bounds::new(point(px(x), px(0.)), size(px(*w), px(HEIGHT)));
            x += w;
            tab
        })
        .collect();
    (Bounds::new(origin, size(px(VIEW), px(HEIGHT))), tabs)
}

#[test]
fn a_wheel_turn_past_the_end_hides_no_tab_that_fits() {
    let (view, tabs) = strip(&[100., 100., 100.]);
    let hidden = hidden_past_edges(view, px(-VIEW), px(0.), tabs.into_iter());
    assert_eq!(hidden, (vec![], vec![]));
}

#[test]
fn an_overflowing_strip_counts_only_up_to_its_end() {
    let (view, tabs) = strip(&[100., 100., 100., 100.]);
    let end = px(VIEW / 3.);
    let hidden = hidden_past_edges(view, -end * 4., end, tabs.clone().into_iter());
    assert_eq!(hidden, (vec![0], vec![]));
    let hidden = hidden_past_edges(view, px(0.), end, tabs.into_iter());
    assert_eq!(hidden, (vec![], vec![3]));
}
