//! A program killed or crashed mid-run leaves its terminal modes on. With mouse reporting
//! on, every mouse move over the pane reaches the shell, which types it out as
//! `35;12;9M35;13;9M…`; focus changes type `[I` / `[O`; with the keyboard protocol pushed
//! (Codex pushes `ESC[>7u` at start), Ctrl-C reaches the shell as `ESC[99;5u` and does not
//! interrupt. So where a command's end mark is (OSC 133 D after C), whatever of these is
//! still on is turned off, in the host's screen and in every viewer's. After a host
//! handoff, the original shell can mark the end of a command whose start was not seen.
//!
//! The screen is checked at the mark itself, not after the whole read: a shell may switch
//! on its own modes for its prompt right after the mark (fish pushes the keyboard
//! protocol when the terminal has it), and those are the shell's to keep.
use fibo_core::terminal::{libghostty_vt::terminal::Mode, Vt};
use std::borrow::Cow;

const REPORTING: [Mode; 9] = [
    Mode::X10_MOUSE,
    Mode::NORMAL_MOUSE,
    Mode::BUTTON_MOUSE,
    Mode::ANY_MOUSE,
    Mode::FOCUS_EVENT,
    Mode::UTF8_MOUSE,
    Mode::SGR_MOUSE,
    Mode::URXVT_MOUSE,
    Mode::SGR_PIXELS_MOUSE,
];
/// The same modes, each reset on its own so any terminal reads it.
const REPORTING_OFF: &[u8] = b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1004l\
\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l";
/// Pops every keyboard-protocol level a program pushed; popping past the last empties it.
const KEYBOARD_OFF: &[u8] = b"\x1b[<99u";

/// The bytes that turn off what a finished command left on; empty when nothing is.
pub fn left_on(vt: &Vt) -> Vec<u8> {
    let t = vt.terminal();
    let mut off = vec![];
    if REPORTING.iter().any(|&m| t.mode(m).unwrap_or(false)) {
        off.extend_from_slice(REPORTING_OFF);
    }
    if t.kitty_keyboard_flags().is_ok_and(|f| !f.is_empty()) {
        off.extend_from_slice(KEYBOARD_OFF);
    }
    off
}

/// Feeds one read to the screen, settling the modes at each command end in it (`ends`,
/// offsets just past each end mark, in order). Returns what viewers must get: the read
/// itself, or the read with the same resets placed where the screen got them.
pub fn feed<'a>(vt: &mut Vt, bytes: &'a [u8], ends: &[usize]) -> Cow<'a, [u8]> {
    let mut out: Option<Vec<u8>> = None;
    let mut from = 0;
    for &end in ends {
        let end = end.clamp(from, bytes.len());
        vt.feed(&bytes[from..end]);
        let off = left_on(vt);
        if !off.is_empty() {
            vt.feed(&off);
        }
        match &mut out {
            Some(o) => o.extend_from_slice(&bytes[from..end]),
            None if !off.is_empty() => out = Some(bytes[..end].to_vec()),
            None => {}
        }
        if let Some(o) = &mut out {
            o.extend_from_slice(&off);
        }
        from = end;
    }
    vt.feed(&bytes[from..]);
    match out {
        Some(mut o) => {
            o.extend_from_slice(&bytes[from..]);
            Cow::Owned(o)
        }
        None => Cow::Borrowed(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(vt: &Vt) -> u8 {
        vt.terminal().kitty_keyboard_flags().unwrap().bits()
    }

    #[test]
    fn what_a_dead_program_left_on_is_reset_and_then_nothing_remains() {
        let mut vt = Vt::new(80, 24).unwrap();
        assert!(left_on(&vt).is_empty(), "a fresh shell has none on");
        // As Codex leaves it when killed: any-motion tracking in SGR form, focus events and
        // the keyboard protocol pushed twice.
        vt.feed(b"\x1b[?1003h\x1b[?1006h\x1b[?1004h\x1b[>7u\x1b[>1u");
        let off = left_on(&vt);
        vt.feed(&off);
        assert!(!vt.terminal().is_mouse_tracking().unwrap());
        assert_eq!(flags(&vt), 0);
        assert!(left_on(&vt).is_empty());
        vt.feed(b"\x1b[?1004h");
        assert!(!left_on(&vt).is_empty(), "focus reporting alone counts too");
    }

    #[test]
    fn resets_land_at_the_end_mark_and_what_the_prompt_turns_on_after_it_stays() {
        let mut vt = Vt::new(80, 24).unwrap();
        let died = b"\x1b[>7u\x1b[?1003h";
        let mark = b"\x1b]133;D;137\x07";
        // The shell's own prompt pushes the keyboard protocol after the mark, as fish does.
        let prompt = b"\x1b[>5u$ ";
        let read = [&died[..], mark, prompt].concat();
        let end = died.len() + mark.len();
        let out = feed(&mut vt, &read, &[end]);
        assert_eq!(flags(&vt), 5, "the prompt's own level is kept");
        assert!(!vt.terminal().is_mouse_tracking().unwrap());
        // A viewer reading the same bytes ends where the host's screen did.
        let mut viewer = Vt::new(80, 24).unwrap();
        viewer.feed(&out);
        assert_eq!(flags(&viewer), 5);
        assert!(!viewer.terminal().is_mouse_tracking().unwrap());
        // Nothing left on: viewers get the read as it was.
        let quiet = b"ok\r\n\x1b]133;D;0\x07$ ";
        assert!(matches!(
            feed(&mut Vt::new(80, 24).unwrap(), quiet, &[quiet.len() - 2]),
            Cow::Borrowed(_)
        ));
    }
}
