//! Read an existing password. Background reads never ask the user for access.
use std::io;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod imp;

/// Whether this system has the keychain `/usr/bin/security` reaches (macOS only for now).
pub const AVAILABLE: bool = imp::AVAILABLE;

/// `interactive` is only for an explicit user action. Never writes or refreshes a credential.
pub fn password(service: &str, interactive: bool) -> io::Result<Option<Vec<u8>>> {
    imp::password(service, interactive)
}
