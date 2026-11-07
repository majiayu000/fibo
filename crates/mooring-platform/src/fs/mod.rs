//! File system calls std offers per platform, or with rights that differ.
use std::io;
use std::path::Path;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;
mod replace;
pub use replace::replace_file;

/// A link at `at` to the directory `to`. May be refused where links need a right the account
/// lacks: the error says so.
pub fn link_dir(to: impl AsRef<Path>, at: &Path) -> io::Result<()> {
    imp::link_dir(to.as_ref(), at)
}

/// A link at `at` to the program `to`, so that running the name runs the program.
/// `ErrorKind::Unsupported` where that is not how a command gets onto `PATH`.
pub fn link_program(to: &Path, at: &Path) -> io::Result<()> {
    imp::link_program(to, at)
}

/// Replaces `path` whole with `bytes`, for this account's eyes as far as the platform allows:
/// a crash leaves the old file or the new one, never part of either.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    imp::write_private(path, bytes)
}

/// Creates `dir` and its parents, for this account's eyes as far as the platform allows.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    imp::create_private_dir(dir)
}
