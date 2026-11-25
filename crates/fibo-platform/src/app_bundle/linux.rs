//! Linux: the `.deb` puts the app in `/usr/lib/fibo`, owned by root and by dpkg, so it is
//! never replaced from here. The daily check still asks GitHub for the latest version (with
//! the `curl` the package depends on) and points at the release page (`PAGE_ONLY`).
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const INSTALLER: bool = false;
pub const SUPPORTED: bool = false;
pub const PAGE_ONLY: bool = true;
const NO: &str = "Linux 上用新的安装包更新";

/// `$XDG_CACHE_HOME/fibo/updates`, else `~/.cache/fibo/updates`: only the last check's time.
pub fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute())
        .or_else(|| crate::dirs::home().map(|h| h.join(".cache")))
        .map(|d| d.join("fibo").join("updates"))
}

pub fn fetch(url: &str, allow_http: bool, max: u64) -> Result<Vec<u8>, String> {
    let protocols = if allow_http { "=https,http" } else { "=https" };
    let out = Command::new("curl")
        .args(["-fsSL", "--proto", protocols, "--proto-redir", protocols])
        .args(["--max-time", "20", "--max-filesize", &max.to_string()])
        .args(["-A", concat!("Fibo/", env!("CARGO_PKG_VERSION"))])
        .args(["-H", "Accept: application/vnd.github+json"])
        .arg(url)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("运行 curl 失败：{e}"))?;
    if out.status.success() {
        return Ok(out.stdout);
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    Err(if err.is_empty() {
        format!("curl 退出码 {:?}", out.status.code())
    } else {
        err
    })
}
pub fn download(_: &str, _: &Path, _: bool, _: u64) -> Result<(), String> {
    Err(NO.into())
}
pub fn unzip(_: &Path, _: &Path) -> Result<(), String> {
    Err(NO.into())
}
pub fn info(_: &Path, _: &str) -> Result<String, String> {
    Err(NO.into())
}
pub fn verify_signature(_: &Path, _: &str) -> Result<(), String> {
    Err(NO.into())
}
pub fn team_id(_: &Path) -> Result<String, String> {
    Err(NO.into())
}
pub fn assess(_: &Path) -> Result<(), String> {
    Err(NO.into())
}
pub fn same_owner(_: &Path, _: &Path) -> std::io::Result<bool> {
    Ok(true)
}
pub fn homebrew_prefixes() -> Vec<PathBuf> {
    Vec::new()
}

pub fn install_after_exit(
    _: u32,
    _: &Path,
    _: &Path,
    _: Vec<std::ffi::OsString>,
) -> std::io::Result<()> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The check really asks: `fetch` runs curl (here against a closed local port, so its
    /// own error comes back) instead of saying updates are unsupported.
    #[test]
    fn the_release_check_goes_through_curl() {
        let err = fetch("https://127.0.0.1:1/", false, 1024).unwrap_err();
        assert!(err.contains("curl"), "{err}");
        assert_ne!(err, NO);
    }
}
