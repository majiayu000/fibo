use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const SYSTEM_PATH: &str = r"C:\Windows\System32;C:\Windows";
pub const PATH_SEPARATOR: &str = ";";

/// Windows has it only with Git for Windows. Found once per process.
pub fn sh() -> OsString {
    static SH: OnceLock<OsString> = OnceLock::new();
    SH.get_or_init(|| {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let dirs: Vec<_> = std::env::split_paths(&path).collect();
        let installed = std::env::var_os("ProgramFiles").map(|p| Path::new(&p).join("Git"));
        super::sh_among(&dirs, installed.as_deref()).map_or_else(|| "sh".into(), Into::into)
    })
    .clone()
}

/// Windows gives a program the account's whole `PATH`, so there is no login shell to ask; a
/// `$SHELL` there comes from something like Git Bash and writes `PATH` in its own form.
pub fn login_shell() -> Option<PathBuf> {
    None
}

/// PowerShell, the newer one if it is on `PATH`. Every supported Windows has the older one.
/// `$SHELL` is not asked, for the reason [`login_shell`] gives. cmd is never chosen: it has no
/// quoting that keeps text from elsewhere from being read as a command.
pub fn interactive() -> String {
    let on_path = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("pwsh.exe"))
            .find(|pwsh| pwsh.is_file())
    });
    let system = || {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
    };
    on_path
        .unwrap_or_else(system)
        .to_string_lossy()
        .into_owned()
}

pub fn stay(shell: &str) -> Vec<String> {
    vec![shell.into(), "-NoLogo".into()]
}

/// Single quotes take everything literally, and the curly ones count as single quotes.
pub fn quote(word: &str) -> String {
    let mut out = String::from("'");
    for c in word.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// npm's launchers here are scripts as well as programs, and asking for the two kinds passes over
/// a function of the same name.
pub fn past_functions(program: &str) -> String {
    format!(
        "& (Get-Command {} -CommandType ExternalScript,Application | Select-Object -First 1)",
        quote(program)
    )
}

/// `-Command` takes its script as text, so unlike `$1` the argument is written into it, quoted.
/// Windows PowerShell 5 drops double quotes from an argument it hands to a program.
pub fn run_then_stay(shell: &str, command: &str, argument: Option<&str>) -> Vec<String> {
    let script = match argument {
        Some(argument) => format!("{command} {}", quote(argument)),
        None => command.to_owned(),
    };
    vec![
        shell.into(),
        "-NoLogo".into(),
        "-NoExit".into(),
        "-Command".into(),
        script,
    ]
}

/// Windows builds an account's environment from the registry, and no shell profile rebuilds it
/// later, so this asks for the same thing: dropping to a list of names would lose whatever the
/// account set for itself. What says where this account's files go still passes, as `HOME` does
/// elsewhere. If the system will not say, this process's own is the best there is.
pub fn login_env(vars: Vec<(OsString, OsString)>) -> Vec<(OsString, OsString)> {
    let passes = |k: &OsStr| {
        k.to_str().is_some_and(|k| {
            let k = k.to_ascii_uppercase();
            matches!(k.as_str(), "HOME" | "USERPROFILE" | "TEMP" | "TMP" | "LANG")
                || k.starts_with("LC_")
        })
    };
    let Some(fresh) = fresh_environment() else {
        return with_path_spelled(vars);
    };
    let mut env: Vec<_> = fresh
        .into_iter()
        .filter(|(k, _)| !passes(k))
        .chain(vars.into_iter().filter(|(k, _)| passes(k)))
        .collect();
    env.sort_by_key(|(k, _)| k.to_string_lossy().to_ascii_uppercase());
    with_path_spelled(env)
}

/// Windows calls it `Path`; names here do not care about case, but a list of them can hold both.
fn with_path_spelled(env: Vec<(OsString, OsString)>) -> Vec<(OsString, OsString)> {
    env.into_iter()
        .map(|(k, v)| {
            if k.eq_ignore_ascii_case("PATH") {
                ("PATH".into(), v)
            } else {
                (k, v)
            }
        })
        .collect()
}

/// What a new login of this account would be given.
fn fresh_environment() -> Option<Vec<(OsString, OsString)>> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{TOKEN_DUPLICATE, TOKEN_IMPERSONATE, TOKEN_QUERY};
    use windows_sys::Win32::System::Environment::{
        CreateEnvironmentBlock, DestroyEnvironmentBlock,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token: HANDLE = std::ptr::null_mut();
    let mut block = std::ptr::null_mut();
    // SAFETY: both out-pointers are valid; the block is read up to the empty string that ends
    // it, before it is destroyed, and the token is closed once.
    unsafe {
        let access = TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_IMPERSONATE;
        if OpenProcessToken(GetCurrentProcess(), access, &mut token) == 0 {
            return None;
        }
        let made = CreateEnvironmentBlock(&mut block, token, 0);
        CloseHandle(token);
        if made == 0 {
            return None;
        }
        let mut env = vec![];
        let mut at = block.cast::<u16>().cast_const();
        loop {
            let mut len = 0;
            while *at.add(len) != 0 {
                len += 1;
            }
            if len == 0 {
                break;
            }
            let entry = std::slice::from_raw_parts(at, len);
            // Past the first character: `=C:=C:\\x` is how a drive's directory is kept.
            if let Some(eq) = entry.iter().skip(1).position(|c| *c == u16::from(b'=')) {
                env.push((
                    OsString::from_wide(&entry[..=eq]),
                    OsString::from_wide(&entry[eq + 2..]),
                ));
            }
            at = at.add(len + 1);
        }
        DestroyEnvironmentBlock(block);
        Some(env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_of_every_shape_are_doubled() {
        assert_eq!(quote("it's"), "'it''s'");
        assert_eq!(quote("it\u{2019}s; $x"), "'it\u{2019}\u{2019}s; $x'");
    }

    #[test]
    fn the_account_s_environment_is_read_and_path_has_one_spelling() {
        let env = login_env(vec![("Path".into(), "from-this-process".into())]);
        let paths: Vec<_> = env
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("PATH"))
            .collect();
        assert_eq!(paths.len(), 1, "{env:?}");
        assert_eq!(paths[0].0, "PATH");
        assert_ne!(paths[0].1, "from-this-process");
        assert!(
            env.iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("SystemRoot"))
        );
    }
}
