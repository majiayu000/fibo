//! Waits for pasted pictures to become attachments before the words go in. An Agent reads a
//! pasted image path off the disk and only then draws `[Image #N]` in its prompt; Enter that
//! arrives first submits the words alone, and the picture lands in the next, empty prompt
//! (11-19: one send in two lost its picture with the machine at load 90–150).
//!
//! Claude Code numbers a conversation's pictures upward; Codex starts at `[Image #1]` in every
//! prompt. A new picture shows as a higher number or as one more mark on the screen.
use fibo_core::protocol::*;
use std::io;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

/// How long a picture may take to show; past it the words go in anyway, as before.
const WAIT: Duration = Duration::from_secs(5);
const EVERY: Duration = Duration::from_millis(100);
/// Enough rows to hold the prompt and the newest messages above it.
const ROWS: usize = 200;

/// The pictures a screen shows: the highest `[Image #N]` (0 for none) and how many marks.
/// Old rows leave the top first, so the newest number stays in view while it scrolls.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Shown {
    pub newest: u32,
    pub marks: usize,
}

impl Shown {
    /// `count` more pictures than `before` showed: by number for a CLI that counts on, by
    /// marks for one that starts again at #1 in each prompt.
    pub(super) fn has_more_than(self, before: Shown, count: usize) -> bool {
        self.newest >= before.newest + count as u32 || self.marks >= before.marks + count
    }
}

pub(super) fn shown(screen: &str) -> Shown {
    let numbers: Vec<u32> = screen
        .split("[Image #")
        .skip(1)
        .filter_map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            rest[digits.len()..]
                .starts_with(']')
                .then(|| digits.parse().ok())
                .flatten()
        })
        .collect();
    Shown {
        newest: numbers.iter().copied().max().unwrap_or(0),
        marks: numbers.len(),
    }
}

/// The pictures on a terminal's screen, or `None` when it cannot be read.
pub(super) fn on_screen(socket: &Path, terminal: &str) -> Option<Shown> {
    let read = Request::ReadScreen {
        id: terminal.to_owned(),
        rows: ROWS,
    };
    match rpc(socket, &read) {
        Ok(Reply::Text(screen)) => Some(shown(&screen)),
        _ => None,
    }
}

/// Returns once the screen shows `count` more pictures than `before`, or after `WAIT`.
/// Whether they showed in time: a CLI that never draws the mark costs one wait, not a send.
pub(super) fn until_shown(
    socket: &Path,
    terminal: &str,
    before: Shown,
    count: usize,
) -> io::Result<bool> {
    let end = Instant::now() + WAIT;
    loop {
        if on_screen(socket, terminal).is_some_and(|now| now.has_more_than(before, count)) {
            return Ok(true);
        }
        if Instant::now() >= end {
            return Ok(false);
        }
        thread::sleep(EVERY);
    }
}

#[cfg(test)]
mod tests {
    use super::{shown, Shown};

    #[test]
    fn a_screen_shows_its_highest_picture_and_how_many_marks() {
        assert_eq!(
            shown("no pictures here"),
            Shown {
                newest: 0,
                marks: 0
            }
        );
        assert_eq!(
            shown("[Image #2]\n⏺ ok\n❯ [Image #3]"),
            Shown {
                newest: 3,
                marks: 2
            }
        );
        assert_eq!(
            shown("[Image #10] then [Image #9]"),
            Shown {
                newest: 10,
                marks: 2
            }
        );
        assert_eq!(
            shown("[Image #] and [Image #x] and [Image #4"),
            Shown::default()
        );
    }

    #[test]
    fn claude_counts_on_and_codex_starts_again_at_one() {
        // Claude Code: the older #1 scrolled away, the new one is #2.
        let before = shown("❯ [Image #1] look");
        assert!(shown("⏺ done\n❯ [Image #2]").has_more_than(before, 1));
        // Codex: every prompt starts again at #1, so only the count of marks grows.
        let before = shown("› [Image #1]  look\n• ok");
        assert!(!shown("› [Image #1]  look\n• ok").has_more_than(before, 1));
        assert!(shown("› [Image #1]  look\n• ok\n› [Image #1]").has_more_than(before, 1));
    }
}
