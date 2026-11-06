//! The `mooring` command at one fixed place on PATH. A terminal Mooring opens finds it by
//! itself, but an Agent may run its commands from a daemon started somewhere else, whose
//! PATH knows nothing of the app. A link there lets it be called by name, wherever the app is.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Where a link is put: directories a Mac's PATH has for every process, first one that takes it.
pub const DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];
const NAME: &str = "mooring";

/// A `mooring` inside an app bundle: only those get the name, never a build under test.
pub fn in_bundle(exe: &Path) -> bool {
    exe.ends_with("Contents/MacOS/mooring")
}

fn ours(target: &Path) -> bool {
    in_bundle(target)
}

/// Points `mooring` at `exe` in the first directory that takes it, and says where. Anything
/// there that Mooring did not put there is left alone, and then no link is made at all: a
/// later directory would not be reached first.
pub fn link(dirs: &[impl AsRef<Path>], exe: &Path) -> io::Result<Option<PathBuf>> {
    for dir in dirs.iter().map(AsRef::as_ref).filter(|d| d.is_dir()) {
        let entry = dir.join(NAME);
        match fs::read_link(&entry) {
            Ok(target) if target == exe => return Ok(Some(entry)),
            Ok(target) if !ours(&target) => return Ok(None),
            Ok(_) => {}
            Err(_) if entry.symlink_metadata().is_ok() => return Ok(None),
            Err(_) => {}
        }
        // Replaced in one step: an Agent running it right now finds the old or the new one.
        let fresh = dir.join(format!(".{NAME}.{}", std::process::id()));
        let _ = fs::remove_file(&fresh);
        match std::os::unix::fs::symlink(exe, &fresh) {
            Ok(()) => {
                fs::rename(&fresh, &entry)?;
                return Ok(Some(entry));
            }
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

/// Whether the name `mooring` reaches `exe`: the first directory that has one decides.
pub fn reaches(dirs: &[impl AsRef<Path>], exe: &Path) -> bool {
    let first = dirs
        .iter()
        .map(|d| d.as_ref().join(NAME))
        .find(|entry| entry.symlink_metadata().is_ok());
    match (
        first.and_then(|e| e.canonicalize().ok()),
        exe.canonicalize(),
    ) {
        (Some(found), Ok(exe)) => found == exe,
        _ => false,
    }
}

/// Whether `runtime` is where the command looks when told nothing.
pub fn is_default_runtime(runtime: &Path) -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    let default = crate::legacy::runtime_dir(Path::new(&home));
    match (runtime.canonicalize(), default.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Whether plain `mooring`, typed anywhere, is this app's command talking to this host.
pub fn by_name_reaches(exe: &Path, runtime: &Path) -> bool {
    reaches(&DIRS, exe) && is_default_runtime(runtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mooring-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn bundle(root: &Path, name: &str) -> PathBuf {
        let mac = root.join(name).join("Contents/MacOS");
        fs::create_dir_all(&mac).unwrap();
        let exe = mac.join("mooring");
        fs::write(&exe, "").unwrap();
        exe
    }

    #[test]
    fn a_link_follows_the_app_and_leaves_others_alone() {
        let root = scratch();
        let bin = root.as_path().join("bin");
        fs::create_dir(&bin).unwrap();
        let dirs = [bin.clone()];
        let (a, b) = (
            bundle(root.as_path(), "A.app"),
            bundle(root.as_path(), "B.app"),
        );

        assert_eq!(link(&dirs, &a).unwrap(), Some(bin.join("mooring")));
        assert!(reaches(&dirs, &a));
        assert!(!reaches(&dirs, &b), "not the running app");
        assert_eq!(link(&dirs, &b).unwrap(), Some(bin.join("mooring")));
        assert!(reaches(&dirs, &b), "the app moved, the name follows");
        assert!(
            fs::read_dir(&bin).unwrap().count() == 1,
            "no temporary link left"
        );

        // Someone else's `mooring`, a file or a link, stays.
        fs::remove_file(bin.join("mooring")).unwrap();
        fs::write(bin.join("mooring"), "mine").unwrap();
        assert_eq!(link(&dirs, &a).unwrap(), None);
        assert_eq!(fs::read(bin.join("mooring")).unwrap(), b"mine");
        assert!(
            !reaches(&dirs, &a),
            "the name is not ours: spell the path out"
        );
        fs::remove_file(bin.join("mooring")).unwrap();
        std::os::unix::fs::symlink("/somewhere/else", bin.join("mooring")).unwrap();
        assert_eq!(link(&dirs, &a).unwrap(), None);
        assert_eq!(
            fs::read_link(bin.join("mooring")).unwrap(),
            Path::new("/somewhere/else")
        );
    }

    #[test]
    fn a_missing_directory_is_skipped_and_only_a_bundle_counts() {
        let root = scratch();
        let bin = root.as_path().join("bin");
        fs::create_dir(&bin).unwrap();
        let exe = bundle(root.as_path(), "A.app");
        let dirs = [root.as_path().join("absent"), bin.clone()];
        assert_eq!(link(&dirs, &exe).unwrap(), Some(bin.join("mooring")));
        assert!(in_bundle(&exe));
        assert!(!in_bundle(Path::new("/repo/target/debug/mooring")));
    }
}
