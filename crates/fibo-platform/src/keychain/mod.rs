//! Whether this system has the keychain `/usr/bin/security` reaches (plugin secrets).
#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod imp;

/// macOS only for now.
pub const AVAILABLE: bool = imp::AVAILABLE;
