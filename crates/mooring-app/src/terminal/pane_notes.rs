//! The notes floating over the grid. What was copied and how the mouse works are separate,
//! each with its own place and clock, so telling one never takes the other off screen.
use super::Pane;
use gpui::{Context, SharedString};
use std::time::Duration;

pub(super) struct MouseHint(&'static str);
impl gpui::IntoElement for MouseHint {
    type Element = <&'static str as gpui::IntoElement>::Element;
    fn into_element(self) -> Self::Element {
        mooring_core::i18n::text(self.0).into_element()
    }
}
pub(super) const MOUSE_HINT: MouseHint = MouseHint(tk!("程序接管了鼠标，选字和复制由它处理"));
const NOTE_LIFETIME: Duration = Duration::from_secs(3);

#[derive(Default)]
pub(super) struct Notes {
    copied: Option<SharedString>,
    copied_seq: u64,
    mouse_hint: bool,
    hint_seq: u64,
}
impl Notes {
    pub(super) fn copied(&self) -> Option<SharedString> {
        self.copied.clone()
    }
    pub(super) fn mouse_hint(&self) -> bool {
        self.mouse_hint
    }
    fn set_copied(&mut self, text: SharedString) -> u64 {
        self.copied = Some(text);
        self.copied_seq += 1;
        self.copied_seq
    }
    fn set_mouse_hint(&mut self) -> u64 {
        self.mouse_hint = true;
        self.hint_seq += 1;
        self.hint_seq
    }
    /// A newer note restarts the clock instead of being cut short.
    fn expire_copied(&mut self, seq: u64) -> bool {
        let current = self.copied_seq == seq;
        if current {
            self.copied = None;
        }
        current
    }
    fn expire_mouse_hint(&mut self, seq: u64) -> bool {
        let current = self.hint_seq == seq;
        if current {
            self.mouse_hint = false;
        }
        current
    }
}
impl Pane {
    pub(super) fn show_toast(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        let seq = self.notes.set_copied(text.into());
        self.after_note_lifetime(cx, move |notes| notes.expire_copied(seq));
    }
    pub(super) fn show_mouse_hint(&mut self, cx: &mut Context<Self>) {
        let seq = self.notes.set_mouse_hint();
        self.after_note_lifetime(cx, move |notes| notes.expire_mouse_hint(seq));
    }
    fn after_note_lifetime(
        &mut self,
        cx: &mut Context<Self>,
        expire: impl FnOnce(&mut Notes) -> bool + 'static,
    ) {
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTE_LIFETIME).await;
            let _ = this.update(cx, |pane, cx| {
                if expire(&mut pane.notes) {
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
#[cfg(test)]
mod tests {
    use super::Notes;

    #[test]
    fn the_mouse_hint_never_takes_a_copied_note_off_screen() {
        let mut notes = Notes::default();
        notes.set_copied("已复制 3 行".into());
        let hint = notes.set_mouse_hint();
        assert_eq!(notes.copied().as_deref(), Some("已复制 3 行"));
        assert!(notes.mouse_hint());
        // And a copied note leaves the hint up; each runs on its own clock.
        notes.set_copied("已复制 4 行".into());
        assert!(notes.mouse_hint());
        assert!(notes.expire_mouse_hint(hint));
        assert_eq!(notes.copied().as_deref(), Some("已复制 4 行"));
    }
    #[test]
    fn a_newer_note_outlives_the_older_ones_clock() {
        let mut notes = Notes::default();
        let old = notes.set_copied("旧".into());
        notes.set_copied("新".into());
        assert!(!notes.expire_copied(old));
        assert_eq!(notes.copied().as_deref(), Some("新"));
    }
}
