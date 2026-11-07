//! What the desktop says about how this account wants things to look.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod imp;

/// Whether keyboards here have a key for app commands that programs in a terminal never see, as
/// the Mac's ⌘ is. Elsewhere commands borrow Ctrl+Shift.
pub const HAS_COMMAND_KEY: bool = imp::HAS_COMMAND_KEY;

/// Whether a window with its title bar hidden must draw minimize, maximize and close itself.
pub const DRAWS_WINDOW_BUTTONS: bool = imp::DRAWS_WINDOW_BUTTONS;

/// Whether the system is set to reduce motion. May take a while: off the main thread.
pub fn reduce_motion() -> bool {
    imp::reduce_motion()
}

/// Makes a font this process carries available to its text system, for faces GPUI's own loader
/// passes over. It must stay loaded for as long as the process runs.
pub fn register_font(bytes: &'static [u8]) -> Result<(), &'static str> {
    imp::register_font(bytes)
}
