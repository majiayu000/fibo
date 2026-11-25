//! The window's frame where the system leaves it to the app and does not hit-test it either:
//! GNOME on Wayland draws no title bar, and GPUI on Linux ignores window control areas. The
//! buttons work by click here, and the window's edges resize it.
use super::*;
use gpui::{Decorations, ResizeEdge, Tiling};

/// Thick enough to find with a pointer, thin enough to leave the content its clicks.
const EDGE: f32 = 6.;

/// Whether this window has to draw and handle its own frame.
pub(in crate::workbench) fn draws_frame(window: &Window) -> bool {
    !fibo_platform::desktop::DRAWS_WINDOW_BUTTONS && client_tiling(window).is_some()
}

fn client_tiling(window: &Window) -> Option<Tiling> {
    match window.window_decorations() {
        Decorations::Client { tiling } => Some(tiling),
        Decorations::Server => None,
    }
}

/// Minimize, maximize and close, each doing its job when clicked.
pub(in crate::workbench) fn buttons(window: &Window) -> gpui::Div {
    let button = |id: &'static str| {
        div()
            .id(id)
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .w(px(T::WINDOW_BUTTON_WIDTH))
            .h_full()
            .text_color(T::color(T::secondary()))
    };
    let mark = |d: gpui::Div| d.border_color(T::color(T::secondary()));
    let maximized = window.is_maximized();
    div()
        .flex()
        .flex_shrink_0()
        .h_full()
        .child(
            button("window-minimize")
                .hover(|d| d.bg(T::color(T::selection())))
                .child(mark(
                    div()
                        .w(px(T::S10))
                        .h(px(T::BORDER_WIDTH))
                        .bg(T::color(T::secondary())),
                ))
                .on_click(|_, window, _| window.minimize_window()),
        )
        .child(
            button("window-maximize")
                .hover(|d| d.bg(T::color(T::selection())))
                .child(mark(
                    div()
                        .size(px(if maximized { T::S8 } else { T::S10 }))
                        .border(px(T::BORDER_WIDTH)),
                ))
                .on_click(|_, window, _| window.zoom_window()),
        )
        .child(
            button("window-close")
                .text_size(px(T::S16))
                .hover(|d| {
                    d.bg(gpui::rgb(T::WINDOW_CLOSE_HOVER))
                        .text_color(gpui::white())
                })
                .child("\u{00d7}")
                .on_click(|_, window, _| window.remove_window()),
        )
}

/// Thin strips along the edges and corners that start a resize, except where the window is
/// tiled or maximized against the screen's side.
pub(in crate::workbench) fn resize_edges(root: gpui::Div, window: &Window) -> gpui::Div {
    let Some(tiling) = client_tiling(window).filter(|_| draws_frame(window)) else {
        return root;
    };
    if window.is_maximized() {
        return root;
    }
    let (top, bottom, left, right) = (!tiling.top, !tiling.bottom, !tiling.left, !tiling.right);
    let strip = |id: &'static str, edge: ResizeEdge| {
        div().id(id).absolute().occlude().on_mouse_down(
            gpui::MouseButton::Left,
            move |_, window, cx| {
                cx.stop_propagation();
                window.start_window_resize(edge);
            },
        )
    };
    let e = px(EDGE);
    root.when(top, |d| {
        d.child(
            strip("resize-top", ResizeEdge::Top)
                .top_0()
                .left(e)
                .right(e)
                .h(e)
                .cursor_ns_resize(),
        )
    })
    .when(bottom, |d| {
        d.child(
            strip("resize-bottom", ResizeEdge::Bottom)
                .bottom_0()
                .left(e)
                .right(e)
                .h(e)
                .cursor_ns_resize(),
        )
    })
    .when(left, |d| {
        d.child(
            strip("resize-left", ResizeEdge::Left)
                .left_0()
                .top(e)
                .bottom(e)
                .w(e)
                .cursor_ew_resize(),
        )
    })
    .when(right, |d| {
        d.child(
            strip("resize-right", ResizeEdge::Right)
                .right_0()
                .top(e)
                .bottom(e)
                .w(e)
                .cursor_ew_resize(),
        )
    })
    .when(top && left, |d| {
        d.child(
            strip("resize-top-left", ResizeEdge::TopLeft)
                .top_0()
                .left_0()
                .size(e)
                .cursor_nwse_resize(),
        )
    })
    .when(top && right, |d| {
        d.child(
            strip("resize-top-right", ResizeEdge::TopRight)
                .top_0()
                .right_0()
                .size(e)
                .cursor_nesw_resize(),
        )
    })
    .when(bottom && left, |d| {
        d.child(
            strip("resize-bottom-left", ResizeEdge::BottomLeft)
                .bottom_0()
                .left_0()
                .size(e)
                .cursor_nesw_resize(),
        )
    })
    .when(bottom && right, |d| {
        d.child(
            strip("resize-bottom-right", ResizeEdge::BottomRight)
                .bottom_0()
                .right_0()
                .size(e)
                .cursor_nwse_resize(),
        )
    })
}
