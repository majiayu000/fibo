use std::ffi::OsString;
use std::path::PathBuf;

/// macOS's own default.
pub const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

pub fn sh() -> OsString {
    "/bin/sh".into()
}

/// `$SHELL`, else the account's: an app started with a clean environment has no `$SHELL`.
pub fn login_shell() -> Option<PathBuf> {
    if let Some(shell) = std::env::var_os("SHELL").filter(|s| !s.is_empty()) {
        return Some(shell.into());
    }
    // SAFETY: getpwuid returns a pointer to static storage or null, read at once here.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_shell.is_null() {
            return None;
        }
        let shell = std::ffi::CStr::from_ptr((*entry).pw_shell);
        Some(std::ffi::OsStr::from_encoded_bytes_unchecked(shell.to_bytes()).into())
    }
}
