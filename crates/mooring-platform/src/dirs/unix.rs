use std::path::PathBuf;

pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn default_config_home() -> Option<PathBuf> {
    home().map(|h| h.join(".config"))
}

pub fn user_name() -> Option<String> {
    // SAFETY: getpwuid's result is read at once, before anything could call it again, and only
    // while the name is copied out.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_name.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr((*entry).pw_name)
            .to_str()
            .ok()
            .map(str::to_owned)
    }
}
