//! What the user types reaching the program.
use super::Pane;
use crate::{connection::Connection, terminal::screen};
use fibo_core::{
    protocol::Request,
    terminal::libghostty_vt::{key::Mods, terminal::ScrollViewport},
};
use gpui::{Context, KeyDownEvent, Window};
use std::time::Instant;

/// Where input comes from. Only what the user typed is kept while the link is down.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    /// Keys, paste, IME, a dropped path, an interrupt.
    Typed,
    /// Made up by the pane from the screen it shows: wheel arrows, focus reports.
    Made,
    /// Mouse reports, which the host may drop while the program is behind on input.
    Pointer,
}

impl Pane {
    pub fn interrupt(&mut self, cx: &mut Context<Self>) {
        self.input(vec![3], cx);
    }
    /// Like Ghostty, typing also drops the selection.
    pub(super) fn input(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        self.input_sent(bytes, cx);
    }
    /// `input`, false when the link refused it (the pane says why): nothing was sent,
    /// kept or held.
    pub(super) fn input_sent(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) -> bool {
        // Clear a painted highlight even when the program sends no output.
        if self.clear_selection() && self.screen.selection.iter().any(Option::is_some) {
            cx.notify();
        }
        self.follow_input(cx);
        let sent = self.deliver(bytes, Origin::Typed, cx);
        cx.emit(super::PaneEvent::Typed);
        sent
    }
    pub fn terminal_id(&self) -> &str {
        &self.terminal
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
        self.send_input(bytes, Origin::Pointer, cx);
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
    /// A peek that is used, not just looked at, sizes the terminal to itself. A look at
    /// another machine's terminal also takes its input over, as 点击接管输入 does.
    pub(super) fn take_size(&mut self, cx: &mut Context<Self>) {
        if !self.peek {
            return;
        }
        self.peek = false;
        if self.host().attached() {
            let wake = self.wake.clone();
            self.connection = Connection::new(&self.socket, &self.terminal, true, wake);
        }
        cx.notify();
    }
    /// What was typed while input was being taken over, once the link has the screen.
    pub(super) fn send_held(&mut self) {
        let ready = self.connection.state.lock().is_ok_and(|s| s.vt.is_some());
        if self.held.is_empty() || !ready {
            return;
        }
        let bytes = std::mem::take(&mut self.held);
        if let Err(error) = self.connection.send(Request::Input {
            bytes,
            lossy: false,
        }) {
            self.local_error = Some(error);
        }
    }
    pub(super) fn send_input(&mut self, bytes: Vec<u8>, origin: Origin, cx: &mut Context<Self>) {
        self.deliver(bytes, origin, cx);
    }
    fn deliver(&mut self, bytes: Vec<u8>, origin: Origin, cx: &mut Context<Self>) -> bool {
        let lossy = origin == Origin::Pointer;
        self.take_size(cx);
        self.typed = Some(Instant::now());
        self.restart_blink(cx);
        // Behind what is still waiting for the taken link, in order.
        if !self.held.is_empty() {
            self.held.extend(bytes);
            return true;
        }
        // Typed while the link is down: kept for the user to send once it is back.
        if self.keep_offline(&bytes, origin == Origin::Typed, cx) {
            return true;
        }
        let ready = self.connection.state.lock().is_ok_and(|s| s.vt.is_some());
        if !ready && self.host().attached() {
            self.held = bytes;
            return true;
        }
        match self.connection.send(Request::Input { bytes, lossy }) {
            Err(error) => {
                self.local_error = Some(error);
                cx.notify();
                false
            }
            // A refusal is about that one input: once typing gets through again, the
            // line stops claiming the pane is broken.
            Ok(()) => {
                if !lossy && self.local_error.take().is_some() {
                    cx.notify();
                }
                true
            }
        }
    }
    /// The terminal's own keymap actions (查找、清屏、字号、滚动), from a key or the menu bar.
    pub(crate) fn run_key(
        &mut self,
        action: crate::keymap::Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    }
    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let k = &event.keystroke;
        if crate::keymap::command_key(&k.modifiers) && k.key.eq_ignore_ascii_case("c") {
            self.copy(cx);
            cx.stop_propagation();
            return;
        }
        if crate::keymap::command_key(&k.modifiers) && k.key.eq_ignore_ascii_case("v") {
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
            self.run_key(action, window, cx);
            cx.stop_propagation();
            return;
        }
        // The search field has the keyboard: what it left alone (arrows, Tab, Ctrl-C)
        // bubbles here, and is not the program's.
        if self.search_has_keys(window, cx) {
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
        let recovering = self.reconnecting || self.reconnect_delay > 0;
        let result = (|| -> Result<Vec<u8>, String> {
            let state = self
                .connection
                .state
                .lock()
                .map_err(|_| "terminal state unavailable")?;
            // Before the new screen arrives Backspace and Enter are kept with the text
            // typed around them, or the line offered later is not the one typed (#517).
            let vt = match state.vt.as_ref() {
                None if !super::offline::link_down(&state, recovering) => {
                    return Err(t!("终端尚未恢复").into());
                }
                vt => vt,
            };
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
