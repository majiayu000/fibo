//! Updating the packaged macOS app from GitHub Releases.
//!
//! - **When.** At most once a day while 设置 › 关于 › 自动检查更新 is on (the default), and
//!   whenever 检查更新 is clicked. The check is one `GET` of the latest non-prerelease
//!   release; it is the only request the app makes on its own.
//! - **Who.** Only an app running from a `.app` bundle with Mooring's bundle id. A build run
//!   from `target/` (`run.sh`, `cargo run`) never checks or installs (`packaged_bundle`).
//! - **How.** `install`: download the zip into the cache, check it against `SHA256SUMS`,
//!   extract it next to the running bundle, check the extracted app's bundle id, version,
//!   signature (`codesign --verify --deep --strict`, Team ID), and Gatekeeper's verdict
//!   (`spctl --assess`), then swap the two directories by rename. Writing into a running
//!   bundle would invalidate its signature and macOS kills the live app and host, which is
//!   why `package-macos.py` swaps the same way. The app then quits and starts the new
//!   bundle, whose host takes the terminals over from the old one, as after a manual swap.
//!   Any failed check removes what was extracted and leaves the installed app untouched.
//! - **Where not.** A bundle Homebrew manages is left to `brew upgrade --cask fibo`, and one
//!   this user cannot replace (owned by root, a read-only volume) gets the release page.
mod install;
mod release;
mod system;
mod version;

pub use install::*;
pub use release::*;
pub use system::System;
pub use version::Version;

use std::path::{Path, PathBuf};

pub const BUNDLE_ID: &str = "page.fibo.app";
/// What the app was called before the rename to Fibo. A release carrying it is still ours
/// (the team id decides who signed it), so an older build can be updated across the rename.
pub const OLD_BUNDLE_ID: &str = "local.mooring.workbench";
/// The Developer ID team every release is signed by. Nothing signed by anyone else installs.
pub const TEAM_ID: &str = "C5UWZ934C2";
pub const API_BASE: &str = "https://api.github.com";
pub const REPO: &str = "majiayu000/mooring";
/// Test only: another API base, e.g. `http://127.0.0.1:8000` for a fake release. It also
/// lets the asset links be plain `http`.
pub const API_ENV: &str = "MOORING_UPDATE_API";
/// Test only, and only beside `API_ENV`: accept a build `spctl` turns down solely because it
/// was not notarized, so a locally signed bundle can stand in for a release.
pub const UNNOTARIZED_ENV: &str = "MOORING_UPDATE_UNNOTARIZED_OK";
/// Test only, and only beside `API_ENV`: install a release the check finds without a click,
/// for an end-to-end run against a fake release (the screen may be locked).
pub const AUTO_INSTALL_ENV: &str = "MOORING_UPDATE_AUTO_INSTALL";
/// How long one check counts for.
pub const CHECK_EVERY: u64 = 24 * 3600;

/// The API base in use, and whether it is the real one.
pub fn api_base() -> (String, bool) {
    match std::env::var(API_ENV) {
        Ok(base) if !base.trim().is_empty() => {
            (base.trim().trim_end_matches('/').to_owned(), false)
        }
        _ => (API_BASE.to_owned(), true),
    }
}
pub fn latest_url(base: &str) -> String {
    format!("{base}/repos/{REPO}/releases/latest")
}

/// The `.app` bundle `exe` runs from, when it runs from one: `<X>.app/Contents/MacOS/<exe>`.
/// A binary anywhere else (`target/debug`, `target/release`) is a development build and
/// never updates itself. Whether the bundle is Mooring's is checked against its Info.plist
/// by `Tools::bundle_info`.
pub fn packaged_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then(|| bundle.to_path_buf())
}

pub use mooring_platform::app_bundle::homebrew_prefixes as brew_prefixes;
/// Whether Homebrew installed this app: the bundle lives in a Caskroom, or a `fibo` (or
/// before the rename `mooring`) cask is recorded. Homebrew then owns the version; replacing the bundle behind its back would
/// leave `brew` believing the old one is installed.
pub fn homebrew_managed(bundle: &Path, prefixes: &[PathBuf]) -> bool {
    let real = std::fs::canonicalize(bundle).unwrap_or_else(|_| bundle.to_path_buf());
    prefixes.iter().any(|prefix| {
        let room = prefix.join("Caskroom");
        real.starts_with(&room) || ["fibo", "mooring"].iter().any(|c| room.join(c).exists())
    })
}
pub const BREW_COMMAND: &str = "brew upgrade --cask fibo";

/// Whether a check is due: never checked, a day since, or a clock that went back.
pub fn due(last: Option<u64>, now: u64) -> bool {
    last.is_none_or(|last| now < last || now - last >= CHECK_EVERY)
}

/// The automatic check's switch, kept as `on`/`off` in `<config>/update-check`. No file is
/// on (the default). A file that cannot be read is not known to be on, so nothing is fetched.
pub fn auto_check(file: &Path) -> bool {
    match std::fs::read_to_string(file) {
        Ok(text) => text.trim() != "off",
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}
pub fn auto_check_file() -> Option<PathBuf> {
    crate::agent::config_dir().map(|d| d.join("update-check"))
}
/// Downloads and the time of the last check.
pub use mooring_platform::app_bundle::cache_dir;
/// Whether this system's app can replace itself; elsewhere nothing here runs.
pub const SUPPORTED: bool = mooring_platform::app_bundle::SUPPORTED;
pub fn last_check(cache: &Path) -> Option<u64> {
    std::fs::read_to_string(cache.join("last-check"))
        .ok()?
        .trim()
        .parse()
        .ok()
}
pub fn record_check(cache: &Path, now: u64) {
    if std::fs::create_dir_all(cache).is_ok() {
        let _ = std::fs::write(cache.join("last-check"), now.to_string());
    }
}

#[cfg(test)]
mod tests;
