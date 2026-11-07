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

/// Which file something is, whatever it is called: two paths with the same id are one file,
/// and a file replaced under one path has a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId {
    pub device: u64,
    pub inode: u64,
}

/// The id of the file at `path`, whose metadata the caller has already read: most systems
/// have it there, and pay nothing more.
pub fn file_id(path: &Path, metadata: &std::fs::Metadata) -> io::Result<FileId> {
    imp::file_id(path, metadata)
}

/// The id of a file that is open.
pub fn open_file_id(file: &std::fs::File) -> io::Result<FileId> {
    imp::open_file_id(file)
}

/// For creating a file only this account may read.
pub fn private_file() -> std::fs::OpenOptions {
    imp::private_file()
}

/// A file for taking a lock on with `File::lock`, created if missing. A link at `path` is
/// refused rather than followed where the system can say.
pub fn lock_file(path: &Path) -> io::Result<std::fs::File> {
    imp::lock_file(path)
}

/// Makes `dir` a folder only this account may enter, or refuses: a link, someone else's folder,
/// one that is open to others. What it holds is safe from other accounts. Its parents must exist.
pub fn secure_dir(dir: &Path) -> io::Result<()> {
    imp::secure_dir(dir)
}

/// Creates `dir` and its parents, for this account's eyes as far as the platform allows.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    imp::create_private_dir(dir)
}
