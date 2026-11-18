//! Read an existing password. Background reads never ask the user for access.
use std::io;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod imp;

/// `interactive` is only for an explicit user action. Never writes or refreshes a credential.
pub fn password(service: &str, interactive: bool) -> io::Result<Option<Vec<u8>>> {
    imp::password(service, interactive)
}
