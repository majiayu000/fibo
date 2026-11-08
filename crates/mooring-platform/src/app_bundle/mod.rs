//! Replacing the installed app with a downloaded release: fetching, unpacking, reading and
//! checking a bundle with the tools the system ships. Only macOS replaces its own bundle;
//! elsewhere `SUPPORTED` is false and every call says so (Windows has its installer).
use std::path::{Path, PathBuf};

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod imp;

/// Whether an app here can update itself in place.
pub const SUPPORTED: bool = imp::SUPPORTED;

/// Where downloads wait to be checked.
pub fn cache_dir() -> Option<PathBuf> {
    imp::cache_dir()
}

/// A small document over `https` (and `http` when `allow_http`), at most `max` bytes.
pub fn fetch(url: &str, allow_http: bool, max: u64) -> Result<Vec<u8>, String> {
    imp::fetch(url, allow_http, max)
}

/// A file over `https` (and `http` when `allow_http`), at most `max` bytes, written to `to`.
pub fn download(url: &str, to: &Path, allow_http: bool, max: u64) -> Result<(), String> {
    imp::download(url, to, allow_http, max)
}

/// Unpacks an app archive (`ditto -c -k --keepParent`) into the directory `into`.
pub fn unzip(zip: &Path, into: &Path) -> Result<(), String> {
    imp::unzip(zip, into)
}

/// One string of the bundle's Info.plist, e.g. `CFBundleIdentifier`.
pub fn info(app: &Path, key: &str) -> Result<String, String> {
    imp::info(app, key)
}

/// The whole bundle's signature is intact (`codesign --verify --deep --strict`) and was issued
/// by Apple to a Developer ID of `team`.
pub fn verify_signature(app: &Path, team: &str) -> Result<(), String> {
    imp::verify_signature(app, team)
}

/// The Team ID of the certificate the bundle is signed with.
pub fn team_id(app: &Path) -> Result<String, String> {
    imp::team_id(app)
}

/// Gatekeeper's verdict on running it (`spctl --assess`); the error carries its words.
pub fn assess(app: &Path) -> Result<(), String> {
    imp::assess(app)
}

/// Whether `a` and `b` belong to the same account.
pub fn same_owner(a: &Path, b: &Path) -> std::io::Result<bool> {
    imp::same_owner(a, b)
}

/// Where Homebrew keeps its casks, most likely first.
pub fn homebrew_prefixes() -> Vec<PathBuf> {
    imp::homebrew_prefixes()
}
