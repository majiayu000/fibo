use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

pub fn link_dir(to: &Path, at: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(to, at)
}

pub fn link_program(to: &Path, at: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(to, at)
}

pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let temp = path.with_extension(format!("tmp{}", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| file.write_all(bytes));
    let renamed = written.and_then(|()| std::fs::rename(&temp, path));
    if renamed.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    renamed
}

pub fn hard_linked(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).is_ok_and(|m| m.nlink() > 1)
}

pub fn create_no_wider(options: &mut std::fs::OpenOptions, than: &std::fs::Permissions) {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    options.mode(than.mode() & 0o777);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_private_file_is_this_accounts_alone_even_over_a_readable_one() {
        let root = std::env::temp_dir().join(format!("kl-privfile-{}", std::process::id()));
        create_private_dir(&root).unwrap();
        let file = root.join("settings.before.json");
        std::fs::write(&file, "old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&file, b"new").unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(std::fs::read(&file).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_private_dir_is_this_accounts_alone() {
        let root = std::env::temp_dir().join(format!("kl-private-{}", std::process::id()));
        let dir = root.join("a").join("b");
        create_private_dir(&dir).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(mode & 0o777, 0o700);
    }
}
