//! What is typed while the link to the host is down (#517): kept instead of dropped, and
//! offered once the link is back. Never sent by itself: by then the program may be at
//! another prompt, where the same keys would answer something else.
use super::Pane;
use crate::connection::State;
use gpui::Context;

/// Room for the line being written; more than a hand types during a reconnect.
pub(super) const LIMIT: usize = 4 * 1024;
/// How much of the end of what was kept the offer shows.
const PREVIEW_CHARS: usize = 60;

#[derive(Default)]
pub(super) struct OfflineInput {
    kept: Vec<u8>,
    /// Some input was refused for room: the offer says so, since the warning made at
    /// the time is gone with the link it was shown on.
    overflowed: bool,
}

impl OfflineInput {
    /// Keeps `bytes` after what is already kept. Past `LIMIT` none of them are kept, so
    /// what is offered later is never a line with a hole in it.
    pub(super) fn keep(&mut self, bytes: &[u8]) -> bool {
        if self.kept.len() + bytes.len() > LIMIT {
            self.overflowed = true;
            return false;
        }
        self.kept.extend_from_slice(bytes);
        true
    }
    pub(super) fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }
    pub(super) fn overflowed(&self) -> bool {
        self.overflowed
    }
    pub(super) fn take(&mut self) -> Vec<u8> {
        self.overflowed = false;
        std::mem::take(&mut self.kept)
    }
    /// What `take` gave out and the link refused, back in front of anything kept since.
    pub(super) fn give_back(&mut self, mut bytes: Vec<u8>, overflowed: bool) {
        bytes.append(&mut self.kept);
        self.kept = bytes;
        self.overflowed |= overflowed;
    }
    pub(super) fn clear(&mut self) {
        self.kept.clear();
        self.overflowed = false;
    }
    /// The end of what was kept as the offer shows it: Enter as ⏎, a backspace as ⌫, Tab
    /// as ⇥, and key sequences (arrows and the like) left out, since they draw nothing.
    pub(super) fn preview(&self) -> String {
        let text = String::from_utf8_lossy(&self.kept);
        let mut out = Vec::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\x1b' => {
                    // CSI / SS3: ESC, '[' or 'O', parameters, one final byte.
                    if matches!(chars.peek(), Some('[' | 'O')) {
                        chars.next();
                        for c in chars.by_ref() {
                            if ('\x40'..='\x7e').contains(&c) {
                                break;
                            }
                        }
                    } else {
                        chars.next();
                    }
                }
                '\r' | '\n' => out.push('⏎'),
                '\x7f' | '\x08' => out.push('⌫'),
                '\t' => out.push('⇥'),
                c if c.is_control() => (),
                c => out.push(c),
            }
        }
        match out.len().checked_sub(PREVIEW_CHARS) {
            Some(cut) if cut > 0 => std::iter::once('…')
                .chain(out[cut..].iter().copied())
                .collect(),
            _ => out.into_iter().collect(),
        }
    }
    /// What becomes of one input. Only what the user typed or pasted is kept: what the
    /// pane makes up itself (arrows from the wheel, focus and mouse reports) answers a
    /// screen that is gone by the time the link is back, and would fill the room.
    pub(super) fn route(
        &mut self,
        bytes: &[u8],
        typed: bool,
        state: &State,
        recovering: bool,
    ) -> Route {
        if !typed || !link_down(state, recovering) {
            Route::Send
        } else if self.keep(bytes) {
            Route::Kept
        } else {
            Route::Full
        }
    }
    /// What the bar under the pane offers to send: only once the link is back.
    pub(super) fn offer(&self, state: &State) -> Option<String> {
        (!self.is_empty() && link_back(state)).then(|| self.preview())
    }
}

/// What `OfflineInput::route` decided for one input.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Route {
    /// Goes to the link as before (which may refuse it and say so).
    Send,
    /// Kept for the offer.
    Kept,
    /// Not kept, for room; the pane says so.
    Full,
}

/// Input now would be kept: the program still runs and the link to it dropped, or is
/// coming back (`recovering`) without the screen and input yet.
pub(super) fn link_down(state: &State, recovering: bool) -> bool {
    !state.ended && (state.dropped || (recovering && !(state.vt.is_some() && state.writable)))
}
/// What was kept can be offered: the link is back with the screen, and takes input.
pub(super) fn link_back(state: &State) -> bool {
    !state.ended && !state.dropped && state.vt.is_some() && state.writable
}

impl Pane {
    /// Keeps what was `typed` while the link is down; true when it was taken care of
    /// here (kept, or refused for room, which says so), not to be sent.
    pub(super) fn keep_offline(
        &mut self,
        bytes: &[u8],
        typed: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let recovering = self.reconnecting || self.reconnect_delay > 0;
        let route = match self.connection.state.lock() {
            Ok(state) => self.offline.route(bytes, typed, &state, recovering),
            Err(_) => Route::Send,
        };
        if route == Route::Send {
            return false;
        }
        if route == Route::Full {
            self.local_error = Some(t!("断线时留着的输入已满 4 KiB，这次的没留").into());
        }
        cx.notify();
        true
    }
    /// What was kept, for the offer, once the link is back, and whether some was not.
    pub(super) fn offline_offer(&self) -> Option<(String, bool)> {
        let state = self.connection.state.lock().ok()?;
        let text = self.offline.offer(&state)?;
        Some((text, self.offline.overflowed()))
    }
    /// Sends what was kept; refused by the link (its outbox full), it stays on offer.
    pub(super) fn send_offline(&mut self, cx: &mut Context<Self>) {
        let overflowed = self.offline.overflowed();
        let bytes = self.offline.take();
        if !bytes.is_empty() && !self.input_sent(bytes.clone(), cx) {
            self.offline.give_back(bytes, overflowed);
        }
        cx.notify();
    }
    pub(super) fn drop_offline(&mut self, cx: &mut Context<Self>) {
        self.offline.clear();
        cx.notify();
    }
}

#[cfg(test)]
#[path = "pane_offline_tests.rs"]
mod tests;
