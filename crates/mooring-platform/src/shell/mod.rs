//! What runs a declared command, and what a process falls back on.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

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

/// The shell a new terminal opens, for someone to work in. Where it cannot be found the name
/// still comes back, so starting it fails and says so.
pub fn interactive() -> String {
    imp::interactive()
}

/// How to start `shell` for someone to work in.
pub fn stay(shell: &str) -> Vec<String> {
    imp::stay(shell)
}

/// `word` in the quoting of [`interactive`], read back as itself whatever it holds.
pub fn quote(word: &str) -> String {
    imp::quote(word)
}

/// `program` for [`interactive`] to run as the program itself, past any function of that name:
/// such a wrapper usually adds flags the caller is about to give, and a CLI may reject one twice.
pub fn past_functions(program: &str) -> String {
    imp::past_functions(program)
}

/// How to have `shell` run `command`, a line in its own language, and then stay for someone to
/// work in. `argument` goes after the command as one word the shell does not interpret, which
/// makes it safe for text from elsewhere.
pub fn run_then_stay(shell: &str, command: &str, argument: Option<&str>) -> Vec<String> {
    imp::run_then_stay(shell, command, argument)
}

/// What a terminal's environment starts from: what logging in afresh gives, whatever started
/// this process, not what `vars` (this process's) picked up on the way. Holds `PATH` under that
/// spelling. What a login's own files set is left to the shell where the shell reads them.
pub fn login_env(vars: impl Iterator<Item = (OsString, OsString)>) -> Vec<(OsString, OsString)> {
    imp::login_env(vars.collect())
}

/// `path` with `dir` added at the end.
pub fn on_path(path: &OsStr, dir: &Path) -> OsString {
    let mut out = path.to_owned();
    if !out.is_empty() {
        out.push(imp::PATH_SEPARATOR);
    }
    out.push(dir);
    out
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
