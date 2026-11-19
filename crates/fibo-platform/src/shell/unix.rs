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

pub use super::quote_posix as quote;

pub fn past_functions(program: &str) -> String {
    format!("command {program}")
}

/// The command and its argument never share a string: the argument stays `$1`.
pub fn run_then_stay(shell: &str, command: &str, argument: Option<&str>) -> Vec<String> {
    let then = match after_exit(shell) {
        Some(report) => format!("{report}; exec {} -l", quote(shell)),
        None => format!("exec {} -l", quote(shell)),
    };
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

/// Run once the command has exited, before the shell takes over: its exit status as an
/// OSC 133 end mark the host keeps (`fibo=launch`: no start mark pairs with it), then the
/// modes a full-screen program turns on and leaves behind when it dies, so the shell after it
/// does not take mouse moves and keys as typed text (2026-11-19: a dead Codex left a prompt
/// full of `35;54;20M`). Mouse reporting, focus reports, the kitty keyboard stack, the cursor.
/// None for a shell whose language is not known here: the line then stays as it was.
fn after_exit(shell: &str) -> Option<String> {
    let status = match shell.rsplit('/').next().unwrap_or(shell) {
        "zsh" | "bash" | "sh" | "dash" | "ksh" | "mksh" => "$?",
        "fish" => "$status",
        _ => return None,
    };
    Some(format!("printf '{LAUNCH_EXIT}{RESET_MODES}' {status}"))
}

/// The end mark `run_then_stay` writes; `%d` is the exit status.
const LAUNCH_EXIT: &str = r"\033]133;D;%d;fibo=launch\007";
const RESET_MODES: &str =
    r"\033[?1000l\033[?1002l\033[?1003l\033[?1006l\033[?1015l\033[?1004l\033[<99u\033[?25h";

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

#[cfg(test)]
mod tests {
    use super::*;

    /// What runs in people's shells, and what older conversations were resumed with.
    #[test]
    fn commands_are_written_as_they_always_were() {
        assert_eq!(past_functions("codex"), "command codex");
        assert_eq!(quote("a'b"), r"'a'\''b'");
        assert_eq!(stay("/bin/zsh"), ["/bin/zsh", "-l"]);
        let report = after_exit("/bin/zsh").unwrap();
        assert_eq!(
            run_then_stay("/bin/zsh", "codex", None),
            [
                "/bin/zsh",
                "-l",
                "-i",
                "-c",
                &format!("codex; {report}; exec '/bin/zsh' -l")
            ]
        );
        assert_eq!(
            run_then_stay("/bin/zsh", "codex", Some("hi")),
            [
                "/bin/zsh",
                "-l",
                "-i",
                "-c",
                &format!("codex \"$1\"; {report}; exec '/bin/zsh' -l"),
                "/bin/zsh",
                "hi"
            ]
        );
        // A shell whose language is unknown gets the line it always got.
        assert_eq!(
            run_then_stay("/usr/bin/nu", "codex", None),
            [
                "/usr/bin/nu",
                "-l",
                "-i",
                "-c",
                "codex; exec '/usr/bin/nu' -l"
            ]
        );
    }

    /// The status reaches the mark in each shell that gets one, and the modes are reset after it.
    #[test]
    fn the_exit_status_and_the_reset_come_out_of_each_shell() {
        for shell in ["/bin/sh", "/bin/bash", "/bin/zsh", "/opt/homebrew/bin/fish"] {
            if !std::path::Path::new(shell).exists() {
                continue;
            }
            let argv = run_then_stay(shell, "(exit 7)", None);
            // Stop before the shell that stays: everything up to `exec` is what runs.
            let script = argv[4].split("; exec ").next().unwrap();
            let script = if shell.ends_with("fish") {
                script.replace("(exit 7)", "sh -c 'exit 7'")
            } else {
                script.to_owned()
            };
            let out = std::process::Command::new(shell)
                .args(["-c", &script])
                .output()
                .unwrap();
            let out = String::from_utf8_lossy(&out.stdout);
            assert!(
                out.starts_with("\x1b]133;D;7;fibo=launch\x07\x1b[?1000l"),
                "{shell}: {out:?}"
            );
            assert!(out.ends_with("\x1b[<99u\x1b[?25h"), "{shell}: {out:?}");
        }
    }
}
