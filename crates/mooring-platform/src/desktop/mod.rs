//! What the desktop says about how this account wants things to look.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod imp;

/// Whether the system is set to reduce motion. May take a while: off the main thread.
pub fn reduce_motion() -> bool {
    imp::reduce_motion()
}

/// Makes a font this process carries available to its text system, for faces GPUI's own loader
/// passes over. It must stay loaded for as long as the process runs.
pub fn register_font(bytes: &'static [u8]) -> Result<(), &'static str> {
    imp::register_font(bytes)
}
