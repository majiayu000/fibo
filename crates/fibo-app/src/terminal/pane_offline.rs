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
pub(super) struct OfflineInput(Vec<u8>);

impl OfflineInput {
    /// Keeps `bytes` after what is already kept. Past `LIMIT` none of them are kept, so
    /// what is offered later is never a line with a hole in it.
    pub(super) fn keep(&mut self, bytes: &[u8]) -> bool {
        if self.0.len() + bytes.len() > LIMIT {
            return false;
        }
        self.0.extend_from_slice(bytes);
        true
    }
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(super) fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
    /// The end of what was kept as the offer shows it: Enter as ⏎, a backspace as ⌫, Tab
    /// as ⇥, and key sequences (arrows and the like) left out, since they draw nothing.
    pub(super) fn preview(&self) -> String {
        let text = String::from_utf8_lossy(&self.0);
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
    /// Keeps `bytes` when the link is down; true when they were taken care of here (kept,
    /// or refused for room, which says so), not to be sent.
    pub(super) fn keep_offline(&mut self, bytes: &[u8], cx: &mut Context<Self>) -> bool {
        let recovering = self.reconnecting || self.reconnect_delay > 0;
        let down = (self.connection.state.lock()).is_ok_and(|s| link_down(&s, recovering));
        if !down {
            return false;
        }
        if !self.offline.keep(bytes) {
            self.local_error = Some(t!("断线时留着的输入已满 4 KiB，这次的没留").into());
        }
        cx.notify();
        true
    }
    /// What was kept, for the offer, once the link is back.
    pub(super) fn offline_offer(&self) -> Option<String> {
        if self.offline.is_empty() {
            return None;
        }
        let back = self.connection.state.lock().is_ok_and(|s| link_back(&s));
        back.then(|| self.offline.preview())
    }
    pub(super) fn send_offline(&mut self, cx: &mut Context<Self>) {
        let bytes = self.offline.take();
        if !bytes.is_empty() {
            self.input(bytes, cx);
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
