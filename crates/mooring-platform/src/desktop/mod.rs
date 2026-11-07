//! What the desktop says about how this account wants things to look.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod imp;

/// Whether the system is set to reduce motion. May take a while: off the main thread.
pub fn reduce_motion() -> bool {
    imp::reduce_motion()
}
