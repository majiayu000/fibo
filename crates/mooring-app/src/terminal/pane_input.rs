//! What the user types reaching the program.
use super::Pane;
use crate::{connection::Connection, terminal::screen};
use gpui::{Context, KeyDownEvent, Window};
use mooring_core::{
    protocol::Request,
    terminal::libghostty_vt::{key::Mods, terminal::ScrollViewport},
};
use std::time::Instant;

impl Pane {
    pub fn interrupt(&mut self, cx: &mut Context<Self>) {
        self.input(vec![3], cx);
    }
    /// Like Ghostty, typing also drops the selection.
    pub(super) fn input(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        self.clear_selection();
        self.follow_input(cx);
        self.send_input(bytes, false, cx);
    }
    pub(super) fn follow_input(&mut self, cx: &mut Context<Self>) {
        self.scroll_lines = 0.;
        let moved = self.connection.state.lock().is_ok_and(|mut state| {
            state.vt.as_mut().is_some_and(|vt| {
                let away = !vt.terminal().viewport_active().unwrap_or(true);
                if away {
                    vt.scroll(ScrollViewport::Bottom);
                }
                away
            })
        });
        // A key the program does not echo would otherwise leave the old frame drawn.
        if moved {
            cx.notify();
        }
    }
    /// Mouse reports may be dropped by the host while the program is behind on input.
    pub(super) fn pointer(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        self.send_input(bytes, true, cx);
    }
    /// Ghostty's 600 ms phase. The timer runs only while this pane holds focus in the
    /// active window, so idle panes keep no timer.
    pub(super) fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.blink_epoch += 1;
        if !self.blink_on {
            self.blink_on = true;
            cx.notify();
        }
        if !self.last_focus {
            return;
        }
        let epoch = self.blink_epoch;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(600))
                    .await;
                let running = this.update(cx, |pane, cx| {
                    if pane.blink_epoch != epoch {
                        return false;
                    }
                    let blinks = crate::ui::terminal_text::current().cursor_blink
                        && pane.screen.cursor_blinking;
                    let next = !blinks || !pane.blink_on;
                    if next != pane.blink_on {
                        pane.blink_on = next;
                        cx.notify();
                    }
                    true
                });
                if !matches!(running, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
    /// A peek that is used, not just looked at, sizes the terminal to itself.
    /// Another machine's terminal is taken over, where it allows that, to be sized here.
    pub(super) fn take_size(&mut self, cx: &mut Context<Self>) {
        if !self.peek {
            return;
        }
        if let Some(slot) = &self.remote {
            let Some(linked) = slot.lock().unwrap().clone().filter(|l| l.resizing) else {
                return;
            };
            let wake = self.wake.clone();
            self.connection = Connection::remote(Some(linked), &self.terminal, true, wake);
        }
        self.peek = false;
        cx.notify();
    }
    pub(super) fn send_input(&mut self, bytes: Vec<u8>, lossy: bool, cx: &mut Context<Self>) {
        self.take_size(cx);
        self.typed = Some(Instant::now());
        self.restart_blink(cx);
        if let Err(error) = self.connection.send(Request::Input { bytes, lossy }) {
            self.local_error = Some(error);
            cx.notify();
        }
    }
    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let k = &event.keystroke;
        if k.modifiers.platform && k.key == "c" {
            self.copy(cx);
            cx.stop_propagation();
            return;
        }
        if k.modifiers.platform && k.key == "v" {
            self.paste(cx);
            cx.stop_propagation();
            return;
        }
        // Keymap shortcuts: the terminal's own run here, the workbench's pass up without
        // reaching the program; any other Cmd key still goes to it.
        if let Some(action) = crate::keymap::action(k, cx) {
            if !action.in_terminal() {
                return;
            }
            use crate::keymap::Action;
            match action {
                Action::ZoomIn | Action::ZoomOut | Action::ZoomReset => {
                    if let Err(error) = screen::zoom(match action {
                        Action::ZoomIn => Some(1),
                        Action::ZoomOut => Some(-1),
                        _ => None,
                    }) {
                        self.local_error = Some(error);
                    }
                    cx.refresh_windows();
                }
                Action::SelectAll => self.select_all(cx),
                Action::Find => self.open_search(window, cx),
                Action::ScrollTop => self.scroll_to(ScrollViewport::Top, cx),
                Action::ScrollBottom => self.scroll_to(ScrollViewport::Bottom, cx),
                Action::PageUp | Action::PageDown => {
                    let rows = self.screen.rows.len().max(1) as isize;
                    let rows = if action == Action::PageUp {
                        -rows
                    } else {
                        rows
                    };
                    self.scroll_to(ScrollViewport::Delta(rows), cx);
                }
                Action::Clear => {
                    self.clear_selection();
                    if let Err(e) = self.connection.send(Request::ClearHistory) {
                        self.local_error = Some(e.to_string());
                    }
                    cx.notify();
                }
                _ => (),
            }
            cx.stop_propagation();
            return;
        }
        if !self.marked.is_empty() {
            return;
        }
        let Some(key) = crate::terminal::keyboard::physical_key(&k.key) else {
            return;
        };
        // Ordinary text belongs to the IME/text-input path, avoiding duplicate commits.
        if !k.modifiers.control
            && !k.modifiers.alt
            && crate::terminal::keyboard::is_text_key(&k.key)
        {
            return;
        }
        // Option typing a layout's character (German @, Polish ą) goes the same way:
        // macOS inserts what it composed, dead keys included.
        if k.modifiers.alt
            && !k.modifiers.control
            && !k.modifiers.platform
            && crate::terminal::keyboard::option_types(
                crate::ui::terminal_text::option_key(),
                &k.key,
                k.key_char.as_deref(),
            )
        {
            return;
        }
        let result = (|| -> Result<Vec<u8>, String> {
            let state = self
                .connection
                .state
                .lock()
                .map_err(|_| "terminal state unavailable")?;
            let vt = state.vt.as_ref().ok_or("终端尚未恢复")?;
            let mut mods = Mods::empty();
            if k.modifiers.control {
                mods |= Mods::CTRL;
            }
            if k.modifiers.alt {
                mods |= Mods::ALT;
            }
            if k.modifiers.shift {
                mods |= Mods::SHIFT;
            }
            if k.modifiers.platform {
                mods |= Mods::SUPER;
            }
            crate::terminal::keyboard::encode(vt, key, &k.key, mods)
        })();
        match result {
            Ok(bytes) => self.input(bytes, cx),
            Err(error) => {
                self.local_error = Some(error);
                cx.notify();
            }
        }
        let _ = window;
        cx.stop_propagation();
    }
}
