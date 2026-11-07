//! Replacing a file another program owns (a CLI's settings, its skills folder) without
//! changing anything but its bytes: its permissions may be guarding a key, and a link may
//! be how the user keeps it in a dotfiles repository.
use super::imp;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Links followed before giving up on a loop; the same number the kernel allows.
const MOST_LINKS: u32 = 32;

/// Returns whether anything was written. The same bytes leave the file untouched, since a
/// new modification time wakes whatever watches it. A crash leaves the old file or the new
/// one whole, except a hard-linked file: only writing in place reaches its other names.
pub fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    let real = through_links(path)?;
    match fs::read(&real) {
        Ok(now) if now == bytes => return Ok(false),
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    if let Some(dir) = real.parent() {
        fs::create_dir_all(dir)?;
    }
    if imp::hard_linked(&real) {
        match fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&real)
        {
            Ok(mut file) => {
                file.write_all(bytes)?;
                file.sync_all()?;
                return Ok(true);
            }
            // Read-only: the rename below still works, and is all that can.
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {}
            Err(e) => return Err(e),
        }
    }
    let temporary = temporary_beside(&real);
    let written = write_new(&temporary, &real, bytes).and_then(|()| fs::rename(&temporary, &real));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written.map(|()| true)
}

/// Where a chain of symlinks ends, which may not exist yet. Only the last name can be a
/// link that matters: a rename through a linked folder lands in the real folder anyway.
fn through_links(path: &Path) -> io::Result<PathBuf> {
    let mut at = path.to_path_buf();
    for _ in 0..MOST_LINKS {
        match fs::read_link(&at) {
            Ok(next) => {
                at = match at.parent() {
                    Some(dir) => dir.join(next),
                    None => next,
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::InvalidInput | io::ErrorKind::NotFound
                ) =>
            {
                return Ok(at);
            }
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other(format!(
        "{} 的符号链接层数太多，未改动",
        path.display()
    )))
}

/// Unique per call: two installs at once must not write into one temporary file.
fn temporary_beside(real: &Path) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let name = real
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    real.with_file_name(format!(
        ".{name}.mooring-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// The new file is never readable by more people than the one it replaces, not even before
/// the rename. A file with nothing to replace gets the usual permissions.
fn write_new(temporary: &Path, real: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let keep = fs::metadata(real).ok().map(|m| m.permissions());
    if let Some(keep) = &keep {
        imp::create_no_wider(&mut options, keep);
    }
    let mut file = options.open(temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    // The mode above went through the umask; this one does not.
    if let Some(keep) = keep {
        fs::set_permissions(temporary, keep)?;
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    fn scratch() -> PathBuf {
        static SCRATCH: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mooring-replace-{}-{}",
            std::process::id(),
            SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_private_file_stays_private() {
        let dir = scratch();
        let file = dir.join("settings.json");
        fs::write(&file, "old").unwrap();
        for private in [0o600, 0o640, 0o664] {
            fs::set_permissions(&file, fs::Permissions::from_mode(private)).unwrap();
            assert!(replace_file(&file, format!("{private}").as_bytes()).unwrap());
            assert_eq!(mode(&file), private);
        }
    }

    #[test]
    fn a_symlink_stays_and_its_target_gets_the_bytes() {
        let dir = scratch();
        let (target, link) = (dir.join("dotfiles.json"), dir.join("settings.json"));
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        symlink("dotfiles.json", &link).unwrap();
        assert!(replace_file(&link, b"new").unwrap());
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(mode(&target), 0o600);
    }

    #[test]
    fn a_chain_of_links_and_a_link_to_nothing_yet_end_at_the_real_file() {
        let dir = scratch();
        let real = dir.join("sub/real.json");
        symlink(&real, dir.join("b.json")).unwrap();
        symlink("b.json", dir.join("a.json")).unwrap();
        assert!(replace_file(&dir.join("a.json"), b"new").unwrap());
        assert_eq!(fs::read(&real).unwrap(), b"new");
        assert!(
            fs::symlink_metadata(dir.join("a.json"))
                .unwrap()
                .is_symlink()
        );
    }

    #[test]
    fn a_link_loop_is_refused_and_writes_nothing() {
        let dir = scratch();
        symlink("b", dir.join("a")).unwrap();
        symlink("a", dir.join("b")).unwrap();
        assert!(replace_file(&dir.join("a"), b"new").is_err());
        assert_eq!(names(&dir), ["a", "b"]);
    }

    #[test]
    fn a_hard_link_keeps_both_names_on_one_file() {
        let dir = scratch();
        let (one, two) = (dir.join("one"), dir.join("two"));
        fs::write(&one, "old").unwrap();
        fs::hard_link(&one, &two).unwrap();
        assert!(replace_file(&two, b"new").unwrap());
        assert_eq!(fs::read(&one).unwrap(), b"new");
        assert_eq!(fs::metadata(&one).unwrap().nlink(), 2);
    }

    #[test]
    fn the_same_bytes_leave_the_file_alone() {
        let dir = scratch();
        let file = dir.join("settings.json");
        fs::write(&file, "same").unwrap();
        let before = fs::metadata(&file).unwrap().ino();
        assert!(!replace_file(&file, b"same").unwrap());
        assert_eq!(fs::metadata(&file).unwrap().ino(), before);
    }

    #[test]
    fn a_new_file_is_made_with_its_folder_and_nothing_is_left_beside_it() {
        let dir = scratch();
        let file = dir.join("new/settings.json");
        assert!(replace_file(&file, b"new").unwrap());
        assert_eq!(names(&dir.join("new")), ["settings.json"]);
        assert_eq!(fs::read(&file).unwrap(), b"new");
    }

    #[test]
    fn a_failed_write_keeps_the_old_file_and_leaves_no_temporary() {
        let dir = scratch();
        let locked = dir.join("locked");
        fs::create_dir(&locked).unwrap();
        let file = locked.join("settings.json");
        fs::write(&file, "old").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
        let result = replace_file(&file, b"new");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read(&file).unwrap(), b"old");
        assert_eq!(names(&locked), ["settings.json"]);
    }
}
