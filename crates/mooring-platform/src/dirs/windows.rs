use std::path::PathBuf;

/// Not `HOME`: on Windows that comes from tools like Git Bash, when it is there at all.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

pub fn default_config_home() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

pub fn user_name() -> Option<String> {
    std::env::var("USERNAME").ok()
}
