//! Claude Code's next-prompt suggestion: the dimmed words it leaves in an empty input box when
//! a turn ends. No hook or log carries them, so they are read off the screen, and only from
//! the input box itself: a row opening with `❯`, directly above the box's bottom rule, whose
//! every word is faint. Anything the user typed is not faint, so a half-typed line is no
//! suggestion. The empty box's own hint is faint too and is told apart by its words.
use fibo_core::terminal::{
    libghostty_vt::{
        screen::CellWide,
        terminal::{Point, PointCoordinate},
        Terminal,
    },
    Vt,
};
use std::io;

const PROMPT: &str = "❯";
/// Its generator asks for 2-12 words; a longer faint line is something else.
const MOST_CHARS: usize = 200;
/// Claude's own faint words in an empty box, which it did not mean as a suggestion.
const NOT_SUGGESTIONS: &[&str] = &["Try \"", "Press up to edit"];

struct Cell {
    text: String,
    faint: bool,
    inverse: bool,
}

fn cell(t: &Terminal<'static, 'static>, x: u16, y: u16) -> io::Result<Option<Cell>> {
    let at = t
        .grid_ref(Point::Active(PointCoordinate { x, y: u32::from(y) }))
        .map_err(io::Error::other)?;
    let wide = at.cell().and_then(|c| c.wide()).map_err(io::Error::other)?;
    if matches!(wide, CellWide::SpacerHead | CellWide::SpacerTail) {
        return Ok(None);
    }
    let mut chars = ['\0'; 16];
    let len = at.graphemes(&mut chars).unwrap_or(0);
    let style = at.style().map_err(io::Error::other)?;
    Ok(Some(Cell {
        text: chars[..len].iter().collect(),
        faint: style.faint,
        inverse: style.inverse,
    }))
}

fn blank(text: &str) -> bool {
    text.chars().all(|c| c.is_whitespace())
}

/// The first thing written on a row, within its first few columns.
fn opening(t: &Terminal<'static, 'static>, y: u16, cols: u16) -> io::Result<Option<(u16, String)>> {
    for x in 0..cols.min(3) {
        if let Some(c) = cell(t, x, y)? {
            if !blank(&c.text) {
                return Ok(Some((x, c.text)));
            }
        }
    }
    Ok(None)
}

/// The words after the prompt on row `y`, when all of them are faint. A drawn cursor may sit
/// on the first character: inverse, and then not necessarily faint.
fn faint_words(
    t: &Terminal<'static, 'static>,
    y: u16,
    from: u16,
    cols: u16,
) -> io::Result<Option<String>> {
    let (mut text, mut any_faint) = (String::new(), false);
    for x in from..cols {
        let Some(c) = cell(t, x, y)? else { continue };
        if c.text.is_empty() {
            text.push(' ');
            continue;
        }
        if !blank(&c.text) {
            let cursor = c.inverse && text.trim().is_empty();
            if !c.faint && !cursor {
                return Ok(None);
            }
            any_faint |= c.faint;
        }
        text.push_str(&c.text);
    }
    Ok(any_faint.then(|| text.trim().to_owned()))
}

/// What the input box suggests right now, if it does.
pub(super) fn read(vt: &Vt) -> io::Result<Option<String>> {
    let t = vt.terminal();
    let rows = t.rows().map_err(io::Error::other)?;
    let cols = t.cols().map_err(io::Error::other)?;
    // From the bottom: earlier prompts in the conversation stay on screen above the box.
    for y in (0..rows.saturating_sub(1)).rev() {
        let Some((x, first)) = opening(t, y, cols)? else {
            continue;
        };
        if first != PROMPT {
            continue;
        }
        let ruled = opening(t, y + 1, cols)?.is_some_and(|(_, c)| c == "─");
        if !ruled {
            // A wrapped suggestion, a menu, or a sent message: nothing to offer whole.
            return Ok(None);
        }
        let words = faint_words(t, y, x + 1, cols)?;
        return Ok(words.filter(|w| {
            !w.is_empty()
                && w.chars().count() <= MOST_CHARS
                && !NOT_SUGGESTIONS.iter().any(|no| w.starts_with(no))
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The box as Claude Code 2.1.318 draws it, `input` being the bytes after the prompt.
    fn screen(input: &str) -> Vt {
        let mut vt = Vt::new(60, 12).unwrap();
        let rule = "─".repeat(60);
        vt.feed(
            format!(
                "\x1b[2J\x1b[H❯ an earlier message\r\n\r\n⏺ Done.\r\n\r\n\x1b[38;2;136;136;136m{rule}\x1b[39m\r\n❯\u{a0}{input}\x1b[0m\r\n\x1b[38;2;136;136;136m{rule}\x1b[39m\r\n  ⏵⏵ auto mode on (shift+tab to cycle)\r\n"
            )
            .as_bytes(),
        );
        vt
    }

    #[test]
    fn the_dimmed_words_in_the_box_are_the_suggestion() {
        // The bytes captured from a real session.
        let vt = screen("\x1b[2mrun the tests\x1b[22m");
        assert_eq!(read(&vt).unwrap().as_deref(), Some("run the tests"));
        let vt = screen("\x1b[2m跑一下测试\x1b[22m");
        assert_eq!(read(&vt).unwrap().as_deref(), Some("跑一下测试"));
    }

    #[test]
    fn a_cursor_drawn_on_its_first_letter_changes_nothing() {
        let vt = screen("\x1b[7mr\x1b[27m\x1b[2mun the tests\x1b[22m");
        assert_eq!(read(&vt).unwrap().as_deref(), Some("run the tests"));
    }

    #[test]
    fn what_the_user_typed_is_never_a_suggestion() {
        for input in [
            "run the tests",
            "run\x1b[2m the tests\x1b[22m",
            "\x1b[2mrun\x1b[22m the tests",
            "\x1b[7mr\x1b[27mun the tests",
            "r\x1b[7m \x1b[27m",
        ] {
            assert_eq!(read(&screen(input)).unwrap(), None, "{input:?}");
        }
    }

    #[test]
    fn an_empty_box_and_its_own_hints_are_not_suggestions() {
        for input in [
            "",
            "\x1b[7m \x1b[27m",
            "\x1b[2mTry \"edit main.rs to...\"\x1b[22m",
            "\x1b[2mPress up to edit queued messages\x1b[22m",
        ] {
            assert_eq!(read(&screen(input)).unwrap(), None, "{input:?}");
        }
    }

    #[test]
    fn only_the_box_itself_counts() {
        // A menu's pointer, faint words and no rule under them.
        let mut vt = Vt::new(60, 8).unwrap();
        vt.feed(
            "\x1b[2J\x1b[H Do you want to proceed?\r\n❯ \x1b[2m1. Yes\x1b[22m\r\n  2. No\r\n"
                .as_bytes(),
        );
        assert_eq!(read(&vt).unwrap(), None);
        // Nothing like a prompt.
        let mut vt = Vt::new(60, 8).unwrap();
        vt.feed(b"\x1b[2J\x1b[H$ \x1b[2mls\x1b[22m\r\n");
        assert_eq!(read(&vt).unwrap(), None);
    }

    #[test]
    fn a_suggestion_that_wrapped_is_not_offered_in_part() {
        let long = "word ".repeat(20);
        let vt = screen(&format!("\x1b[2m{long}\x1b[22m"));
        assert_eq!(read(&vt).unwrap(), None);
    }
}
