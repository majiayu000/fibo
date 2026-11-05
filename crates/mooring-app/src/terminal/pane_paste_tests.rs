use super::{PasteAction, copied_note};
use gpui::{ClipboardItem, Image, ImageFormat};

#[test]
fn an_image_goes_to_the_program_as_ctrl_v() {
    let image = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1]));
    assert_eq!(super::paste_action(Some(&image)), PasteAction::Image);
}
#[test]
fn copied_note_counts_lines_or_characters_and_flags_a_whole_screen_copy() {
    assert_eq!(copied_note("中文ab", false), "已复制 4 字");
    assert_eq!(copied_note("1\n2\n3", false), "已复制 3 行");
    assert_eq!(copied_note("a\nb", true), "没选中文字，已复制整屏 2 行");
}
#[test]
fn text_wins_over_an_image_and_an_empty_clipboard_does_nothing() {
    let text = ClipboardItem::new_string("hi".into());
    assert_eq!(
        super::paste_action(Some(&text)),
        PasteAction::Text("hi".into())
    );
    assert_eq!(super::paste_action(None), PasteAction::Nothing);
}
#[test]
fn plain_multiline_paste_counts_lines_and_sends_cr() {
    assert_eq!(super::line_count("echo 一\necho 二\n"), 2);
    assert_eq!(super::line_count("single line\n"), 1);
    assert_eq!(super::plain_paste("a\r\nb\nc"), b"a\rb\rc");
}
/// A line copied with its newline (a triple-click in a browser) used to reach
/// neither the confirmation nor the terminal: refused as unsafe, nothing pasted.
#[test]
fn a_single_line_with_its_newline_asks_before_it_runs() {
    assert!(super::needs_paste_confirm("ls -la\n", false));
    assert!(super::needs_paste_confirm("a\nb", false));
    assert!(!super::needs_paste_confirm("ls -la", false));
    assert!(!super::needs_paste_confirm("ls -la\n", true));
    assert!(!super::needs_paste_confirm("\x1b[201~\n", false));
    assert_eq!(super::plain_paste("ls -la\n"), b"ls -la\r");
}
