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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

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
