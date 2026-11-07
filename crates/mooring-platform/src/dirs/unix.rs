use std::path::PathBuf;

pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn default_config_home() -> Option<PathBuf> {
    home().map(|h| h.join(".config"))
}
