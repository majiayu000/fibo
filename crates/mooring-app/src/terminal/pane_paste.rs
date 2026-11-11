//! A user's paste, and the notes that say what was copied.
use super::Pane;
use gpui::Context;
use mooring_core::terminal::libghostty_vt::terminal::Mode;

impl Pane {
    /// A user's own paste. Without bracketed paste each line would run as soon as it
    /// arrives, so a multi-line paste asks first instead of being refused, as Ghostty's
    /// paste protection does. Text Mooring places itself still goes through `prefill`.
    pub fn paste(&mut self, cx: &mut Context<Self>) {
        let text = match paste_action(cx.read_from_clipboard().as_ref()) {
            PasteAction::Text(text) => text,
            // Claude Code and Codex read an image off the clipboard themselves on Ctrl+V.
            // Sent whatever runs here, as libghostty-swift does: a pane is not always
            // known to hold an Agent, and a shell only takes it as quoted-insert.
            PasteAction::Image => return self.input(vec![0x16], cx),
            PasteAction::Nothing => return,
        };
        if needs_paste_confirm(&text, self.bracketed()) {
            self.paste_confirm = Some(text);
            self.local_error = None;
            cx.notify();
        } else {
            self.prefill(&text, cx);
        }
    }
    pub(super) fn confirm_paste(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.paste_confirm.take() {
            self.input(plain_paste(&text), cx);
        }
        cx.notify();
    }
    pub(super) fn bracketed(&self) -> bool {
        self.connection
            .state
            .lock()
            .ok()
            .and_then(|s| {
                s.vt.as_ref()
                    .and_then(|v| v.terminal().mode(Mode::BRACKETED_PASTE).ok())
            })
            .unwrap_or(false)
    }
}
pub(super) fn line_count(text: &str) -> usize {
    text.trim_end_matches(['\r', '\n']).lines().count()
}
pub(super) struct CopyHint(&'static str);
impl From<CopyHint> for gpui::SharedString {
    fn from(h: CopyHint) -> Self {
        mooring_core::i18n::text(h.0).into()
    }
}
pub(super) const NOT_COPIED: CopyHint = CopyHint(tk!(
    "Fibo 这边没有选区，剪贴板没动。程序里选的字由它自己复制"
));
/// Whether a copy has anything to take: the selection, or the screen when there is none.
/// With the mouse taken Mooring can't see the program's own selection, so copying the screen
/// could replace what the user just copied there.
pub(super) fn copies_anything(whole_screen: bool, mouse_taken: bool) -> bool {
    !(whole_screen && mouse_taken)
}
/// "已复制 3 行" / "已复制 12 字"; a screen copied for lack of a selection says so.
pub(super) fn copied_note(text: &str, whole_screen: bool) -> String {
    let lines = text.lines().count();
    if whole_screen {
        return t!("没选中文字，已复制整屏 {lines} 行", lines);
    }
    if lines > 1 {
        t!("已复制 {lines} 行", lines)
    } else {
        t!("已复制 {chars} 字", chars = text.chars().count())
    }
}

#[derive(Debug, PartialEq)]
enum PasteAction {
    Text(String),
    /// Only an image: the program fetches it from the clipboard on Ctrl+V.
    Image,
    Nothing,
}
fn paste_action(item: Option<&gpui::ClipboardItem>) -> PasteAction {
    let Some(item) = item else {
        return PasteAction::Nothing;
    };
    if let Some(text) = item.text() {
        return PasteAction::Text(text);
    }
    let image = item
        .entries()
        .iter()
        .any(|e| matches!(e, gpui::ClipboardEntry::Image(_)));
    if image {
        PasteAction::Image
    } else {
        PasteAction::Nothing
    }
}

/// What a terminal sends for a paste without bracketed paste: newlines become CR.
/// Without bracketed paste every line break runs what precedes it, so any pasted break,
/// even one at the end of a single line, asks first. Text with escapes is refused anyway.
fn needs_paste_confirm(text: &str, bracketed: bool) -> bool {
    !bracketed && text.contains(['\n', '\r']) && !text.contains('\x1b')
}
fn plain_paste(text: &str) -> Vec<u8> {
    text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
}
#[cfg(test)]
#[path = "pane_paste_tests.rs"]
mod paste_tests;
