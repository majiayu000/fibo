//! The mouse over a pane: reports to a program that took it, selection, the
//! scrollbar and the wheel.
use super::paste::{NOT_COPIED, copied_note, copies_anything};
use super::{Pane, SCROLLBAR_HIT, WHEEL_REPORTS};
use crate::terminal::screen;
use gpui::{Context, Pixels, Point, Window, px};
use mooring_core::terminal::{
    Vt,
    libghostty_vt::{key::Mods, mouse, selection::gesture::Autoscroll, terminal::ScrollViewport},
};

impl Pane {
    /// The program reads the mouse itself, so a plain drag goes to it, not to selection.
    pub fn takes_mouse(&self) -> bool {
        self.connection.state.lock().is_ok_and(|s| {
            s.vt.as_ref()
                .is_some_and(|v| v.terminal().is_mouse_tracking().unwrap_or(false))
        })
    }
    fn report_mouse(
        &mut self,
        action: mouse::Action,
        button: Option<mouse::Button>,
        position: Point<Pixels>,
        modifiers: gpui::Modifiers,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        if modifiers.shift {
            return false;
        }
        let (Some(bounds), Some(metrics)) = (self.bounds, self.metrics) else {
            return false;
        };
        if !bounds.contains(&position) && action != mouse::Action::Release {
            return false;
        }
        let result = (|| {
            let state = self.connection.state.lock().unwrap();
            let Some(v) = state.vt.as_ref().filter(|_| state.writable) else {
                return Ok(None);
            };
            if !v.terminal().is_mouse_tracking()? {
                return Ok(None);
            }
            if self.mouse.is_none() {
                self.mouse = Some(crate::terminal::terminal_input::MouseEncoder::new()?);
            }
            let mut mods = Mods::empty();
            if modifiers.control {
                mods |= Mods::CTRL;
            }
            if modifiers.alt {
                mods |= Mods::ALT;
            }
            if modifiers.platform {
                mods |= Mods::SUPER;
            }
            let pos = mouse::Position {
                x: f32::from(position.x - bounds.left()).max(0.) * scale,
                y: f32::from(position.y - bounds.top()).max(0.) * scale,
            };
            let size = mouse::EncoderSize {
                screen_width: (f32::from(bounds.size.width) * scale).round() as u32,
                screen_height: (f32::from(bounds.size.height) * scale).round() as u32,
                cell_width: (f32::from(metrics.width) * scale).round().max(1.) as u32,
                cell_height: (f32::from(metrics.height) * scale).round().max(1.) as u32,
                padding_top: 0,
                padding_bottom: 0,
                padding_left: 0,
                padding_right: 0,
            };
            crate::terminal::terminal_input::mouse_bytes(
                self.mouse.as_mut().unwrap(),
                v,
                action,
                button,
                mods,
                pos,
                size,
            )
            .map(Some)
        })();
        match result {
            Ok(Some(bytes)) => {
                if !bytes.is_empty() {
                    self.pointer(bytes, cx);
                }
                cx.stop_propagation();
                true
            }
            Ok(None) => false,
            Err(e) => {
                self.local_error = Some(e.to_string());
                cx.notify();
                true
            }
        }
    }
    pub(super) fn mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.take_size(cx);
        if event.modifiers.platform
            && event.button == gpui::MouseButton::Left
            && let Some((y, x)) = self.cell_at(event.position)
            && let Some(link) = self.screen.link_at(y, x)
        {
            cx.open_url(&link.uri);
            cx.stop_propagation();
            return;
        }
        // ⌘ opens it as iTerm2 and Ghostty do; ⌘⌥ shows it in Finder instead.
        if event.modifiers.platform
            && event.button == gpui::MouseButton::Left
            && let Some(target) = self.path_target()
        {
            self.open_target(&target, event.modifiers.alt, cx);
            cx.stop_propagation();
            return;
        }
        let button = match event.button {
            gpui::MouseButton::Left => mouse::Button::Left,
            gpui::MouseButton::Middle => mouse::Button::Middle,
            _ => mouse::Button::Right,
        };
        self.select.pressed_button = Some(event.button);
        // The thumb is Mooring's own, even over a program that took the mouse.
        if event.button == gpui::MouseButton::Left && self.on_scrollbar(event.position) {
            self.select.dragging_scrollbar = true;
            self.scroll_to_pointer(event.position, cx);
            cx.stop_propagation();
            return;
        }
        if self.report_mouse(
            mouse::Action::Press,
            Some(button),
            event.position,
            event.modifiers,
            window.scale_factor(),
            cx,
        ) {
            return;
        }
        if event.button == gpui::MouseButton::Left
            && let Some(at) = self.local(event.position)
        {
            // Ghostty's gesture counts clicks itself: cell, word, then line.
            self.scale = window.scale_factor();
            let state = self.connection.state.clone();
            if let Ok(state) = state.lock()
                && let Some(vt) = &state.vt
                && let Some(grid) = grid(vt, self.metrics, self.scale)
                && let Some(sel) = &mut self.select.selecting
            {
                let _ = sel.press(vt.terminal(), grid, at);
            }
            cx.notify();
        }
    }
    pub(super) fn mouse_up(
        &mut self,
        event: &gpui::MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The outside handler sees releases over tabs and other panes too.
        // Only capture a release for a gesture that started in this pane.
        if self.select.pressed_button != Some(event.button) {
            return;
        }
        self.select.pressed_button = None;
        if std::mem::take(&mut self.select.dragging_scrollbar) {
            return;
        }
        let button = match event.button {
            gpui::MouseButton::Left => mouse::Button::Left,
            gpui::MouseButton::Middle => mouse::Button::Middle,
            _ => mouse::Button::Right,
        };
        let reported = self.report_mouse(
            mouse::Action::Release,
            Some(button),
            event.position,
            event.modifiers,
            window.scale_factor(),
            cx,
        );
        if !reported
            && event.button == gpui::MouseButton::Left
            && self.select.selecting.as_ref().is_some_and(|s| s.active)
        {
            let at = self.local(event.position).unwrap_or_default();
            let state = self.connection.state.clone();
            if let Ok(state) = state.lock()
                && let Some(vt) = &state.vt
                && let Some(grid) = grid(vt, self.metrics, self.scale)
                && let Some(sel) = &mut self.select.selecting
            {
                sel.release(vt.terminal(), grid, at);
            }
            // Copy on select, as iTerm2 and Ghostty do; a plain click selects nothing.
            if self.selected_text().is_some() {
                self.copy(cx);
            }
            cx.notify();
        }
    }
    pub(super) fn mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.links.pointer = Some(event.position);
        self.set_modifiers(event.modifiers, cx);
        let hover = self
            .cell_at(event.position)
            .filter(|(y, x)| self.screen.link_at(*y, *x).is_some());
        if hover != self.links.hover_link {
            self.links.hover_link = hover;
            cx.notify();
        }
        let word = self
            .cell_at(event.position)
            .filter(|_| hover.is_none())
            .and_then(|(y, x)| self.screen.path_at(y, x).cloned());
        self.hover_word(word, cx);
        let button = event.pressed_button.map(|b| match b {
            gpui::MouseButton::Left => mouse::Button::Left,
            gpui::MouseButton::Middle => mouse::Button::Middle,
            _ => mouse::Button::Right,
        });
        if self.report_mouse(
            mouse::Action::Motion,
            button,
            event.position,
            event.modifiers,
            window.scale_factor(),
            cx,
        ) && button == Some(mouse::Button::Left)
            && !self.notes.mouse_hint()
        {
            self.show_mouse_hint(cx);
        }
    }
    /// Window-wide, so a selection drag keeps extending after it leaves the pane.
    pub(super) fn drag_selection(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if self.select.dragging_scrollbar {
            if event.pressed_button == Some(gpui::MouseButton::Left) {
                self.scroll_to_pointer(event.position, cx);
            }
            return;
        }
        if event.pressed_button != Some(gpui::MouseButton::Left)
            || !self.select.selecting.as_ref().is_some_and(|s| s.active)
        {
            return;
        }
        let Some(at) = self.local(event.position) else {
            return;
        };
        let state = self.connection.state.clone();
        let Ok(state) = state.lock() else { return };
        let (Some(vt), Some(sel)) = (&state.vt, &mut self.select.selecting) else {
            return;
        };
        let Some(grid) = grid(vt, self.metrics, self.scale) else {
            return;
        };
        let scroll = sel.drag(vt.terminal(), grid, at);
        let start = matches!(scroll, Ok(Autoscroll::Up | Autoscroll::Down)) && !sel.autoscrolling;
        sel.autoscrolling |= start;
        drop(state);
        if start {
            self.autoscroll(cx);
        }
        cx.notify();
    }
    /// While a selection drag sits above or below the pane, scroll and extend it.
    fn autoscroll(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
                let keep = this.update(cx, |this, cx| {
                    let state = this.connection.state.clone();
                    let Ok(mut state) = state.lock() else {
                        return false;
                    };
                    let (Some(vt), Some(sel)) = (&mut state.vt, &mut this.select.selecting) else {
                        return false;
                    };
                    let Some(grid) = grid(vt, this.metrics, this.scale) else {
                        return false;
                    };
                    let delta = match sel.autoscroll(vt.terminal()) {
                        Ok(Autoscroll::Up) if sel.active => -1,
                        Ok(Autoscroll::Down) if sel.active => 1,
                        _ => {
                            sel.autoscrolling = false;
                            return false;
                        }
                    };
                    vt.scroll(ScrollViewport::Delta(delta));
                    let _ = sel.tick(vt.terminal(), grid);
                    cx.notify();
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
    pub(super) fn scroll_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(m) = self.metrics else {
            return;
        };
        if event.touch_phase == gpui::TouchPhase::Started {
            (self.scroll_cols, self.scroll_lines) = (0., 0.);
        }
        // Trackpad pixels add up across events; a wheel notch moves three rows, like Ghostty.
        let (dx, dy) = match event.delta {
            gpui::ScrollDelta::Pixels(p) => (p.x / m.width, p.y / m.height),
            gpui::ScrollDelta::Lines(l) => (l.x * 3., l.y * 3.),
        };
        // Sums of fractions drift just below whole rows (5 x 0.2 = 0.99999994).
        let whole = |v: f32| (v + v.signum() * 1e-3).trunc().clamp(-100., 100.);
        self.scroll_lines += dy;
        self.scroll_cols += dx;
        let lines = whole(self.scroll_lines) as i32;
        let cols = whole(self.scroll_cols) as i32;
        self.scroll_lines -= lines as f32;
        self.scroll_cols -= cols as f32;
        if lines == 0 && cols == 0 {
            return;
        }
        // Sideways scrolling only means something to a program that tracks the mouse.
        for _ in 0..cols.unsigned_abs().min(WHEEL_REPORTS as u32) {
            self.report_mouse(
                mouse::Action::Press,
                Some(if cols > 0 {
                    mouse::Button::Six
                } else {
                    mouse::Button::Seven
                }),
                event.position,
                event.modifiers,
                window.scale_factor(),
                cx,
            );
        }
        // A fling is up to 100 lines; a program that took the mouse gets a few reports,
        // not a queue-filling burst.
        let mut reported = 0;
        for _ in 0..lines.unsigned_abs() {
            if reported == WHEEL_REPORTS {
                break;
            }
            if self.report_mouse(
                mouse::Action::Press,
                Some(if lines > 0 {
                    mouse::Button::Four
                } else {
                    mouse::Button::Five
                }),
                event.position,
                event.modifiers,
                window.scale_factor(),
                cx,
            ) {
                reported += 1;
                continue;
            }
            let result = {
                let mut state = self.connection.state.lock().unwrap();
                if let Some(v) = &mut state.vt {
                    match crate::terminal::terminal_input::alternate_scroll(v, lines > 0) {
                        Ok(Some(bytes)) if !event.modifiers.shift => Ok(Some(bytes)),
                        Ok(_) => {
                            v.scroll(ScrollViewport::Delta(if lines > 0 { -1 } else { 1 }));
                            Ok(None)
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    Ok(None)
                }
            };
            match result {
                // Arrow keys for a full-screen program: keep the selection.
                Ok(Some(b)) => self.send_input(b, false, cx),
                Err(e) => self.local_error = Some(e),
                _ => (),
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
    /// The thumb shows while the view is up in history, along the pane's right edge.
    pub(super) fn scrollbar_shown(&self) -> bool {
        let (top, total, len) = self.viewport;
        total > len && top + len < total
    }
    fn on_scrollbar(&self, point: Point<Pixels>) -> bool {
        self.scrollbar_shown()
            && self
                .bounds
                .is_some_and(|b| b.contains(&point) && point.x >= b.right() - px(SCROLLBAR_HIT))
    }
    /// Centers the view on the history row under the pointer's height in the pane.
    fn scroll_to_pointer(&mut self, point: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some(b), (_, total, len)) = (self.bounds, self.viewport) else {
            return;
        };
        let at = ((point.y - b.top()) / b.size.height).clamp(0., 1.);
        let row = (at * total as f32) as usize;
        if let Ok(mut s) = self.connection.state.lock()
            && let Some(vt) = &mut s.vt
        {
            vt.scroll(ScrollViewport::Row(row.saturating_sub(len / 2)));
            if let Ok(v) = vt.viewport() {
                self.viewport = v;
            }
        }
        cx.notify();
    }
    pub(super) fn scroll_to(&mut self, to: ScrollViewport, cx: &mut Context<Self>) {
        if let Ok(mut state) = self.connection.state.lock()
            && let Some(vt) = &mut state.vt
        {
            vt.scroll(to);
        }
        cx.notify();
    }
    /// The selected text, unwrapped as Ghostty copies it, even where it has scrolled
    /// out of view; None when nothing is selected.
    pub(super) fn selected_text(&self) -> Option<String> {
        let state = self.connection.state.lock().ok()?;
        state
            .vt
            .as_ref()?
            .selection_text()
            .ok()
            .flatten()
            .filter(|t| !t.is_empty())
    }
    /// Copies the selection; with nothing selected, the visible screen, unless the program
    /// holds the mouse: a highlight on screen may then be its own, just copied, which this
    /// copy would overwrite. Says what happened in the corner, away from the text being read.
    pub fn copy(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_text();
        let whole_screen = selected.is_none();
        if !copies_anything(whole_screen, self.takes_mouse()) {
            return self.show_toast(NOT_COPIED, cx);
        }
        let text = selected.unwrap_or_else(|| {
            let lines: Vec<String> = self
                .screen
                .rows
                .iter()
                .map(|row| {
                    let text: String = row.iter().map(|run| run.text.as_str()).collect();
                    text.trim_end().to_owned()
                })
                .collect();
            lines.join("\n")
        });
        let note = copied_note(&text, whole_screen);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.show_toast(note, cx);
    }
    fn cell_at(&self, point: Point<Pixels>) -> Option<(u16, u16)> {
        let bounds = self.bounds?;
        let metrics = self.metrics?;
        if !bounds.contains(&point) {
            return None;
        }
        Some((
            ((point.y - bounds.top()) / metrics.height).floor().max(0.) as u16,
            ((point.x - bounds.left()) / metrics.width).floor().max(0.) as u16,
        ))
    }
    /// A window point relative to the pane, in logical pixels; may lie outside it.
    fn local(&self, point: Point<Pixels>) -> Option<(f32, f32)> {
        let bounds = self.bounds?;
        Some((
            f32::from(point.x - bounds.left()),
            f32::from(point.y - bounds.top()),
        ))
    }
    /// Everything the terminal holds, scrollback included, as Ghostty's select_all.
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        if let Ok(state) = self.connection.state.lock()
            && let Some(vt) = &state.vt
        {
            let t = vt.terminal();
            let _ = t
                .select_all()
                .and_then(|sel| t.set_selection(sel.as_ref()).map(|_| ()));
        }
        cx.notify();
    }
    pub(super) fn clear_selection(&mut self) {
        if let Ok(state) = self.connection.state.lock()
            && let Some(vt) = &state.vt
        {
            let _ = vt.terminal().set_selection(None);
        }
    }
}
/// The pane's cell grid for the selection gesture, once its cells have been measured.
fn grid(
    vt: &Vt,
    metrics: Option<screen::CellMetrics>,
    scale: f32,
) -> Option<crate::terminal::selection::Grid> {
    let metrics = metrics?;
    let t = vt.terminal();
    Some(crate::terminal::selection::Grid {
        cols: t.cols().ok()?,
        rows: t.rows().ok()?,
        cell_w: f32::from(metrics.width),
        cell_h: f32::from(metrics.height),
        scale,
    })
}
