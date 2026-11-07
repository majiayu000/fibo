use std::ffi::OsString;
use std::path::PathBuf;

/// macOS's own default.
pub const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

pub const PATH_SEPARATOR: &str = ":";

pub fn sh() -> OsString {
    "/bin/sh".into()
}

/// `$SHELL`, else the account's: an app started with a clean environment has no `$SHELL`.
pub fn login_shell() -> Option<PathBuf> {
    if let Some(shell) = std::env::var_os("SHELL").filter(|s| !s.is_empty()) {
        return Some(shell.into());
    }
    // SAFETY: getpwuid returns a pointer to static storage or null, read at once here.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_shell.is_null() {
            return None;
        }
        let shell = std::ffi::CStr::from_ptr((*entry).pw_shell);
        Some(std::ffi::OsStr::from_encoded_bytes_unchecked(shell.to_bytes()).into())
    }
}

/// `$SHELL`, else the first of the usual ones here: zsh is macOS's own, and most Linux has none.
pub fn interactive() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            ["/bin/zsh", "/bin/bash"]
                .into_iter()
                .find(|s| std::path::Path::new(s).exists())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "/bin/sh".into())
}

pub fn stay(shell: &str) -> Vec<String> {
    vec![shell.into(), "-l".into()]
}

pub fn quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

pub fn past_functions(program: &str) -> String {
    format!("command {program}")
}

/// The command and its argument never share a string: the argument stays `$1`.
pub fn run_then_stay(shell: &str, command: &str, argument: Option<&str>) -> Vec<String> {
    let then = format!("exec {} -l", quote(shell));
    let script = match argument {
        Some(_) => format!("{command} \"$1\"; {then}"),
        None => format!("{command}; {then}"),
    };
    let mut argv = vec![shell.into(), "-l".into(), "-i".into(), "-c".into(), script];
    if let Some(argument) = argument {
        argv.extend([shell.to_owned(), argument.to_owned()]);
    }
    argv
}

/// The login identity and locale pass; the login shell's profile adds the rest, `PATH` included.
pub fn login_env(vars: Vec<(OsString, OsString)>) -> Vec<(OsString, OsString)> {
    const PASS: [&str; 8] = [
        "HOME",
        "USER",
        "LOGNAME",
        "SHELL",
        "TMPDIR",
        "LANG",
        "SSH_AUTH_SOCK",
        "__CF_USER_TEXT_ENCODING",
    ];
    let mut env: Vec<_> = vars
        .into_iter()
        .filter(|(k, _)| {
            k.to_str()
                .is_some_and(|k| PASS.contains(&k) || k.starts_with("LC_"))
        })
        .collect();
    env.push(("PATH".into(), SYSTEM_PATH.into()));
    env
}
