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

/// Replaces `path` whole with `bytes` the way [`replace_file`] does: a temporary of this
/// write's own, synced, then renamed, so a crash or a reader on a timer sees the old file or
/// the new one. The one way the window writes its own settings files.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    replace_file(path, bytes).map(drop)
}

/// Replaces `path` whole with `bytes`, for this account's eyes as far as the platform allows:
/// a crash leaves the old file or the new one, never part of either.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    imp::write_private(path, bytes)
}

/// Where `path` really is, in the form programs and people use: links resolved, and on Windows
/// without the `\\\\?\\` prefix `std::fs::canonicalize` adds, which many programs cannot take as a
/// directory and which never compares equal to a path from anywhere else.
pub fn canonical(path: &Path) -> io::Result<std::path::PathBuf> {
    imp::canonical(path)
}

/// Whether this platform provides allocated disk sizes for a whole directory tree.
pub const DISK_USAGE_SUPPORTED: bool = cfg!(unix);

/// A command whose first output field is the KiB allocated on the target's filesystem.
pub fn disk_usage_command(path: &Path) -> io::Result<std::process::Command> {
    #[cfg(unix)]
    {
        let mut command = std::process::Command::new("/usr/bin/du");
        command.args(["-skx"]).arg(path);
        Ok(command)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// Opens a regular file for inspection without following links or waiting on a pipe.
pub fn open_regular_nofollow(path: &Path) -> io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        if file.metadata()?.is_file() {
            Ok(file)
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(io::ErrorKind::Unsupported.into())
    }
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

/// Whether opening `path` would run it, as far as the file itself says: its mode bits on Unix,
/// its kind on Windows.
pub fn is_program(path: &Path) -> bool {
    imp::is_program(path)
}

/// For creating a file only this account may read.
pub fn private_file() -> std::fs::OpenOptions {
    imp::private_file()
}

/// Tries to take an exclusive lock while leaving the first `prefix_len` bytes readable by
/// other handles. Contenders must use the same prefix length. Closing the file releases it.
pub fn try_lock_with_readable_prefix(
    file: &std::fs::File,
    prefix_len: u32,
) -> Result<(), std::fs::TryLockError> {
    imp::try_lock_with_readable_prefix(file, prefix_len)
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

#[cfg(test)]
mod atomic_tests {
    use super::*;

    /// Settings files the window writes: whole, no temporary left, a failure said.
    #[test]
    fn a_replaced_file_leaves_no_temporary_behind() {
        let dir = std::env::temp_dir().join(format!("fibo-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("theme-dark");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        write_private(&dir.join("key"), b"k1").unwrap();
        write_private(&dir.join("key"), b"k2").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::read(dir.join("key")).unwrap(), b"k2");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::create_dir(dir.join("gone")).unwrap();
        assert!(
            write_atomic(&dir.join("gone"), b"x").is_err(),
            "a failed write says so"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Two threads writing one file each finish with a whole file: they once shared a
    /// temporary named after the process and removed each other's.
    #[test]
    fn writers_at_once_do_not_share_a_temporary() {
        let dir = std::env::temp_dir().join(format!("fibo-private-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("devices.json");
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        write_private(&path, format!("writer {i}").as_bytes()).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .starts_with("writer ")
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
