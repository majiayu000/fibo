//! Where an account's files are.
use std::path::PathBuf;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

/// This account's home directory.
pub fn home() -> Option<PathBuf> {
    imp::home()
}

/// Where this account's programs keep their settings. `XDG_CONFIG_HOME` wins everywhere: it is
/// what a test or a user points elsewhere on purpose.
pub fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(imp::default_config_home)
}
