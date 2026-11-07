//! What runs a declared command, and what a process falls back on.
use std::ffi::OsString;
use std::path::PathBuf;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

/// The `PATH` for a process started with none at all.
pub const SYSTEM_PATH: &str = imp::SYSTEM_PATH;

/// The POSIX shell that runs a declared command. Where there is none the name comes back
/// unresolved, so the command fails to start and says so.
pub fn sh() -> OsString {
    imp::sh()
}

/// The shell this account logs in with, to ask for the `PATH` it sets up. None where a program
/// already gets the account's whole `PATH`.
pub fn login_shell() -> Option<PathBuf> {
    imp::login_shell()
}

/// `sh.exe` on `PATH`, else beside a `git.exe` on it: Git's installer by default puts only
/// `Git\cmd` there and keeps `sh` in `Git\bin`. Else where that installer puts it.
#[cfg(any(windows, test))]
fn sh_among(path: &[PathBuf], installed: Option<&std::path::Path>) -> Option<PathBuf> {
    let on_path = path.iter().map(|dir| dir.join("sh.exe"));
    let by_git = path
        .iter()
        .filter(|dir| dir.join("git.exe").is_file())
        .filter_map(|dir| Some(dir.parent()?.join("bin").join("sh.exe")));
    let by_installer = installed.map(|git| git.join("bin").join("sh.exe"));
    on_path
        .chain(by_git)
        .chain(by_installer)
        .find(|sh| sh.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_is_found_on_path_then_beside_git_then_where_git_installs() {
        let root = std::env::temp_dir().join(format!("kl-sh-among-{}", std::process::id()));
        let touch = |rel: &[&str]| {
            let file = rel.iter().fold(root.clone(), |p, part| p.join(part));
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, "").unwrap();
            file
        };
        let cmd = root.join("Git").join("cmd");
        let elsewhere = root.join("elsewhere");
        touch(&["Git", "cmd", "git.exe"]);
        let beside_git = touch(&["Git", "bin", "sh.exe"]);
        let installed = touch(&["Installed", "bin", "sh.exe"]);
        let on_path = touch(&["elsewhere", "sh.exe"]);
        let installer = root.join("Installed");

        assert_eq!(
            sh_among(&[cmd.clone(), elsewhere], Some(&installer)),
            Some(on_path)
        );
        assert_eq!(
            sh_among(std::slice::from_ref(&cmd), Some(&installer)),
            Some(beside_git)
        );
        assert_eq!(sh_among(&[], Some(&installer)), Some(installed));
        assert_eq!(sh_among(&[], Some(&root.join("nowhere"))), None);
        assert_eq!(sh_among(&[], None), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
