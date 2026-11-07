use std::io;
use std::path::Path;

/// Needs Developer Mode or the right to make links; without them this reports the error.
pub fn link_dir(to: &Path, at: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(to, at)
}

/// Where a Windows install puts the command is not decided yet (docs/windows/plan.md).
pub fn link_program(_to: &Path, _at: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows 上还不能把 mooring 放到 PATH",
    ))
}

/// Not private by itself: it takes the rights of the directory it is in, which under the user
/// profile means this account, SYSTEM and administrators. Elsewhere it may mean more people.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// std cannot count a file's names here; such a file loses its other names when replaced.
pub fn hard_linked(_path: &Path) -> bool {
    false
}

/// Files take the rights of their directory; there is no mode to narrow at creation.
pub fn create_no_wider(_options: &mut std::fs::OpenOptions, _than: &std::fs::Permissions) {}
