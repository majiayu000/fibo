//! A program killed or crashed with mouse reporting on leaves it on: every mouse move over
//! the pane then reaches the shell, which types it out as `35;12;9M35;13;9M…`, and focus
//! changes type `[I` / `[O`. A prompt never asks for these, so once the shell is back at
//! its prompt after a command (OSC 133 D after C), the ones still on are turned off, in
//! the host's screen and in every viewer's.
use fibo_core::terminal::{libghostty_vt::terminal::Mode, Vt};

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
const OFF: &[u8] = b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1004l\
\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l";

/// The bytes that turn off the reporting a finished command left on; None when none is.
pub fn left_on(vt: &Vt) -> Option<&'static [u8]> {
    let t = vt.terminal();
    REPORTING
        .iter()
        .any(|&m| t.mode(m).unwrap_or(false))
        .then_some(OFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_modes_left_on_are_reset_and_then_none_remain() {
        let mut vt = Vt::new(80, 24).unwrap();
        assert_eq!(left_on(&vt), None, "a fresh shell has none on");
        // As a killed TUI leaves it: any-motion tracking in SGR form, and focus events.
        vt.feed(b"\x1b[?1003h\x1b[?1006h\x1b[?1004h");
        let off = left_on(&vt).expect("tracking is on");
        vt.feed(off);
        assert!(!vt.terminal().is_mouse_tracking().unwrap());
        assert_eq!(left_on(&vt), None);
        vt.feed(b"\x1b[?1004h");
        assert!(left_on(&vt).is_some(), "focus reporting alone counts too");
    }
}
