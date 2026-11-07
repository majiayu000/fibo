//! Where an account's files are.
use std::path::{Path, PathBuf};

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

/// This account's home directory.
pub fn home() -> Option<PathBuf> {
    imp::home()
}

/// This account's name, whatever the environment says: one started bare has none.
pub fn user_name() -> Option<String> {
    imp::user_name()
}

/// The home directory as text, for what shows or compares paths as text. None if it is not
/// valid Unicode.
pub fn home_str() -> Option<String> {
    home()?.into_os_string().into_string().ok()
}

/// Where this account's programs keep their settings. `XDG_CONFIG_HOME` wins everywhere: it is
/// what a test or a user points elsewhere on purpose.
pub fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(imp::default_config_home)
}

/// Whether a program was left here by the system starting it, not by anyone choosing a folder:
/// `/` from Finder and Dock, the Windows folder or a bare drive from Task Scheduler and the like.
pub fn is_system_start(dir: &Path) -> bool {
    imp::is_system_start(dir)
}
