use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const SYSTEM_PATH: &str = r"C:\Windows\System32;C:\Windows";

/// Windows has it only with Git for Windows. Found once per process.
pub fn sh() -> OsString {
    static SH: OnceLock<OsString> = OnceLock::new();
    SH.get_or_init(|| {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let dirs: Vec<_> = std::env::split_paths(&path).collect();
        let installed = std::env::var_os("ProgramFiles").map(|p| Path::new(&p).join("Git"));
        super::sh_among(&dirs, installed.as_deref()).map_or_else(|| "sh".into(), Into::into)
    })
    .clone()
}

/// Windows gives a program the account's whole `PATH`, so there is no login shell to ask; a
/// `$SHELL` there comes from something like Git Bash and writes `PATH` in its own form.
pub fn login_shell() -> Option<PathBuf> {
    None
}
