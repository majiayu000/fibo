use fibo_platform::{shell, terminal as term};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where this host wrote the shell integration; unset when it is turned off.
static INTEGRATION: OnceLock<PathBuf> = OnceLock::new();
/// This host's runtime directory and executable, handed to every terminal for hooks.
static HOST: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();

/// Lets every later terminal tell its Agents' hooks where this host is.
pub fn announce_host(root: &Path) -> io::Result<()> {
    // As given, only made absolute: resolving links can push the socket past SUN_LEN.
    let root = std::path::absolute(root)?;
    let _ = HOST.set((root, std::env::current_exe()?));
    Ok(())
}

/// What a terminal's programs learn about the host: that they run in Fibo, in which
/// terminal, the runtime to reach, and the executable whose `hook` subcommand reports to it.
/// That executable's directory also goes on `PATH`, after `login_path`, so an
/// Agent in the terminal can run `fibo list` / `split` by name, as its usage says.
/// The login profile keeps it: macOS's path_helper keeps entries it does not know, and
/// profiles add to `PATH` rather than replace it.
fn host_env(
    terminal: &str,
    host: Option<&(PathBuf, PathBuf)>,
    login_path: &std::ffi::OsStr,
) -> Vec<(OsString, OsString)> {
    use fibo_core::protocol::{HOOK_ENV, MARKER_ENV, RUNTIME_ENV, TERMINAL_ENV};
    let mut env: Vec<(OsString, OsString)> = vec![
        (MARKER_ENV.into(), "1".into()),
        (TERMINAL_ENV.into(), terminal.into()),
    ];
    if let Some((root, exe)) = host {
        env.push((RUNTIME_ENV.into(), root.as_os_str().to_owned()));
        env.push((HOOK_ENV.into(), exe.as_os_str().to_owned()));
        if let Some(dir) = exe.parent().filter(|d| !d.as_os_str().is_empty()) {
            env.push(("PATH".into(), shell::on_path(login_path, dir)));
        }
    }
    // Agents and hooks set up under the old name (Keepline) read these too, and what is
    // written for Fibo reads `FIBO_*`.
    let old: Vec<_> = env
        .iter()
        .flat_map(|(k, v)| {
            let names = k.to_str().map(fibo_core::legacy::other_names);
            names
                .unwrap_or_default()
                .into_iter()
                .map(|o| (OsString::from(o), v.clone()))
        })
        .collect();
    env.extend(old);
    env
}

/// Writes the shell integration into the runtime, so the scripts always match this
/// host. `MOORING_NO_SHELL_INTEGRATION` in the host's environment turns it off.
pub fn install_shell_integration(root: &Path) -> io::Result<()> {
    if std::env::var_os("MOORING_NO_SHELL_INTEGRATION").is_some() {
        return Ok(());
    }
    let dir = root.join("shell-integration");
    std::fs::create_dir_all(dir.join("zsh"))?;
    std::fs::write(
        dir.join("zsh/.zshenv"),
        include_str!("../shell-integration/zsh/.zshenv"),
    )?;
    std::fs::write(
        dir.join("zsh/fibo-integration"),
        include_str!("../shell-integration/zsh/fibo-integration"),
    )?;
    std::fs::create_dir_all(dir.join("bash"))?;
    std::fs::write(
        dir.join("bash/fibo.bash"),
        include_str!("../shell-integration/bash/fibo.bash"),
    )?;
    std::fs::create_dir_all(dir.join("fish"))?;
    std::fs::write(
        dir.join("fish/fibo.fish"),
        include_str!("../shell-integration/fish/fibo.fish"),
    )?;
    let _ = INTEGRATION.set(dir);
    Ok(())
}

/// How an interactive shell starts with the integration: its argv and environment.
/// zsh goes through the integration's ZDOTDIR, which restores the user's own before any
/// of their startup files run; bash reads the integration as its init file, which loads
/// the user's files itself; fish sources it after the user's config. A shell given
/// anything beyond `-l` / `-i` (a command, a script) starts untouched.
fn integration(argv: &[String], dir: Option<&Path>) -> (Vec<String>, Vec<(OsString, OsString)>) {
    let untouched = (argv.to_vec(), Vec::new());
    let Some(dir) = dir else {
        return untouched;
    };
    let Some(name) = argv
        .first()
        .and_then(|p| Path::new(p).file_name())
        .and_then(|n| n.to_str())
    else {
        return untouched;
    };
    let flags = &argv[1..];
    let interactive = flags
        .iter()
        .all(|f| matches!(f.as_str(), "-l" | "--login" | "-i"));
    let login = flags.iter().any(|f| f == "-l" || f == "--login");
    let shared = (
        OsString::from("MOORING_SHELL_INTEGRATION_DIR"),
        dir.as_os_str().to_owned(),
    );
    match name {
        "zsh" => (
            argv.to_vec(),
            vec![("ZDOTDIR".into(), dir.join("zsh").into_os_string()), shared],
        ),
        // bash ignores --init-file in a login shell, so the file plays the login part.
        "bash" if interactive => {
            let mut env = vec![shared];
            if login {
                env.push(("MOORING_BASH_LOGIN".into(), "1".into()));
            }
            (
                vec![
                    argv[0].clone(),
                    "--init-file".into(),
                    dir.join("bash/fibo.bash").to_string_lossy().into_owned(),
                    "-i".into(),
                ],
                env,
            )
        }
        "fish" if interactive => {
            let script = dir.join("fish/fibo.fish").to_string_lossy().into_owned();
            let mut argv = argv.to_vec();
            argv.push("--init-command".into());
            // fish's single quotes only treat \\ and \' specially.
            argv.push(format!(
                "source '{}'",
                script.replace('\\', "\\\\").replace('\'', "\\'")
            ));
            (argv, vec![shared])
        }
        _ => untouched,
    }
}

pub fn spawn(
    argv: &[String],
    cwd: &Path,
    cols: u16,
    rows: u16,
    terminal: &str,
    colorfgbg: &str,
) -> io::Result<(term::Child, term::Master)> {
    let (argv, shell_env) = integration(argv, INTEGRATION.get().map(PathBuf::as_path));
    let mut env: BTreeMap<OsString, OsString> =
        baseline_env(std::env::vars_os()).into_iter().collect();
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("COLORTERM".into(), "truecolor".into());
    env.insert("COLORFGBG".into(), colorfgbg.into());
    env.extend(shell_env);
    let login_path = env.get(std::ffi::OsStr::new("PATH")).cloned();
    env.extend(host_env(
        terminal,
        HOST.get(),
        &login_path.unwrap_or_default(),
    ));
    env.remove(std::ffi::OsStr::new("COLUMNS"));
    env.remove(std::ffi::OsStr::new("LINES"));
    term::spawn(&term::Spec {
        argv: &argv,
        cwd,
        env: env.into_iter().collect(),
        cols,
        rows,
    })
}

/// A shell starts from the same environment however this host was started, the way a
/// terminal opened from the Dock does: the login identity and locale pass, the rest is
/// left to the login shell's profile. The host is long-lived and inherits whatever
/// launched it, so passing its environment through would freeze a launcher's one-off
/// state into every later shell. Started from inside Claude Code, for example,
/// `CLAUDE_CODE_CHILD_SESSION` reached every shell; Claude launched there believed it
/// was a nested child and stopped saving its transcript, so Fibo lost track of it.
/// A launcher's `PATH` is its own state too, so it falls back to the login's.
/// Agent home overrides pass: this host reads transcripts where they point, so the
/// Agents it starts must write there too.
fn baseline_env(vars: impl Iterator<Item = (OsString, OsString)>) -> Vec<(OsString, OsString)> {
    const AGENT_HOMES: [&str; 3] = ["CLAUDE_CONFIG_DIR", "CODEX_HOME", "GROK_HOME"];
    let vars: Vec<_> = vars.collect();
    let homes = vars
        .iter()
        .filter(|(k, _)| k.to_str().is_some_and(|k| AGENT_HOMES.contains(&k)))
        .cloned()
        .collect::<Vec<_>>();
    let mut env = shell::login_env(vars.into_iter());
    env.extend(homes);
    env
}

#[cfg(test)]
mod env_tests {
    #[test]
    fn only_the_login_baseline_passes_whatever_launched_the_host() {
        let launcher = [
            ("HOME", "/Users/u"),
            ("LANG", "zh_CN.UTF-8"),
            ("LC_CTYPE", "UTF-8"),
            ("PATH", "/agent/bin:/usr/bin"),
            ("CLAUDE_CODE_CHILD_SESSION", "1"),
            ("CLAUDECODE", "1"),
            ("CODEX_SANDBOX", "seatbelt"),
            ("SOME_FUTURE_AGENT_MARKER", "1"),
            ("COLUMNS", "80"),
            ("GROK_HOME", "/tmp/grok"),
        ]
        .map(|(k, v)| (k.into(), v.into()));
        let env = super::baseline_env(launcher.into_iter());
        let found = |name: &str| env.iter().find(|(k, _)| k == name).map(|(_, v)| v);
        for kept in ["HOME", "LANG", "LC_CTYPE", "GROK_HOME"] {
            assert!(found(kept).is_some(), "{kept}: {env:?}");
        }
        for dropped in [
            "CLAUDE_CODE_CHILD_SESSION",
            "CLAUDECODE",
            "CODEX_SANDBOX",
            "SOME_FUTURE_AGENT_MARKER",
            "COLUMNS",
        ] {
            assert!(found(dropped).is_none(), "{dropped}: {env:?}");
        }
        assert_ne!(found("PATH").unwrap(), "/agent/bin:/usr/bin");
    }
    #[test]
    fn a_spawned_shell_does_not_see_the_hosts_launcher_state() {
        std::env::set_var("CLAUDE_CODE_CHILD_SESSION", "1");
        // The shell outlives `env` so its output is read before the terminal closes: when
        // the only process exits first, macOS may close the terminal with its output unread
        // and the first read returns end of file with nothing (seen on a CI runner).
        let argv = [
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            "/usr/bin/env; sleep 1".to_owned(),
        ];
        let colorfgbg = fibo_core::terminal::Scheme::default().colors().colorfgbg();
        let (mut child, out) = super::spawn(
            &argv,
            std::path::Path::new("/"),
            80,
            24,
            "scheme-test",
            colorfgbg,
        )
        .unwrap();
        // Read while it runs: macOS holds a closing terminal open until its output is read.
        let mut reader = out.reader().unwrap();
        let mut text = String::new();
        let mut buf = [0u8; 4096];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match reader.next(&mut buf, Some(std::time::Duration::from_millis(200))) {
                Ok(super::term::Read::Data(n)) => {
                    text.push_str(&String::from_utf8_lossy(&buf[..n]));
                }
                Ok(super::term::Read::Quiet) => (),
                Ok(super::term::Read::Closed) | Err(_) => break,
            }
        }
        let _ = child.kill();
        assert!(text.contains("TERM=xterm-256color"), "{text}");
        assert!(
            text.contains("PATH=/usr/bin:/bin:/usr/sbin:/sbin"),
            "{text}"
        );
        assert!(!text.contains("CLAUDE_CODE_CHILD_SESSION"), "{text}");
    }
}

#[cfg(test)]
mod integration_tests {
    use std::path::Path;

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn interactive_shells_start_through_the_integration_and_commands_do_not() {
        let dir = Path::new("/rt/shell-integration");
        let (zsh, env) = super::integration(&argv(&["/bin/zsh", "-l"]), Some(dir));
        assert_eq!(zsh, ["/bin/zsh", "-l"]);
        assert_eq!(
            env[0],
            ("ZDOTDIR".into(), "/rt/shell-integration/zsh".into())
        );
        assert_eq!(env[1].1, "/rt/shell-integration");

        let (bash, env) = super::integration(&argv(&["/opt/homebrew/bin/bash", "-l"]), Some(dir));
        assert_eq!(
            bash,
            [
                "/opt/homebrew/bin/bash",
                "--init-file",
                "/rt/shell-integration/bash/fibo.bash",
                "-i"
            ]
        );
        assert!(env.contains(&("MOORING_BASH_LOGIN".into(), "1".into())));
        let (_, env) = super::integration(&argv(&["/bin/bash"]), Some(dir));
        assert!(!env.iter().any(|(k, _)| k == "MOORING_BASH_LOGIN"));

        let odd = Path::new("/rt/it's here");
        let (fish, _) = super::integration(&argv(&["/opt/homebrew/bin/fish", "-l"]), Some(odd));
        assert_eq!(
            fish[1..],
            [
                "-l",
                "--init-command",
                "source '/rt/it\\'s here/fish/fibo.fish'"
            ]
        );

        for untouched in [
            argv(&["/bin/bash", "-l", "-i", "-c", "codex"]),
            argv(&["/opt/homebrew/bin/fish", "-c", "ls"]),
            argv(&["/bin/sh", "-l"]),
        ] {
            assert_eq!(
                super::integration(&untouched, Some(dir)),
                (untouched, vec![])
            );
        }
        assert!(super::integration(&argv(&["/bin/zsh"]), None).1.is_empty());
    }

    /// Runs a real shell through its integration with `cwd` as the start, and returns
    /// the directory its first OSC 7 reports, or None when the shell is not installed.
    fn reported(shell: &str, cwd: &Path, home: &Path) -> Option<String> {
        if !Path::new(shell).exists() {
            return None;
        }
        let root = std::env::temp_dir().join(format!("kl-shint-{}", std::process::id()));
        super::install_shell_integration(&root).unwrap();
        let (argv, env) =
            super::integration(&argv(&[shell, "-l"]), Some(&root.join("shell-integration")));
        let out = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(cwd)
            .env_clear()
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin")
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                c.stdin.take().unwrap().write_all(b"cd .\nexit\n")?;
                c.wait_with_output()
            })
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let start = text.find("\x1b]7;file://")? + "\x1b]7;file://".len();
        let rest = &text[start..];
        let rest = &rest[rest.find('/')?..];
        Some(rest[..rest.find('\x1b')?].to_owned())
    }

    #[test]
    fn bash_and_fish_report_their_directory_and_still_load_the_users_files() {
        let base = std::env::temp_dir().join(format!("kl-shint-home-{}", std::process::id()));
        let home = base.join("home");
        let cwd = base.join("目录 a");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cwd).unwrap();
        let cwd = cwd.canonicalize().unwrap();
        // A login bash reads the profile; mooring must not skip it.
        std::fs::write(home.join(".bash_profile"), "echo PROFILE_LOADED\n").unwrap();
        let encoded = cwd
            .to_str()
            .unwrap()
            .replace("目录", "%E7%9B%AE%E5%BD%95")
            .replace(' ', "%20");
        for shell in [
            "/bin/bash",
            "/opt/homebrew/bin/bash",
            "/opt/homebrew/bin/fish",
        ] {
            if Path::new(shell).exists() {
                let dir = reported(shell, &cwd, &home);
                assert_eq!(dir.as_deref(), Some(encoded.as_str()), "{shell}");
            }
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
mod host_env_tests {
    #[test]
    fn every_terminal_names_itself_and_the_host_once_announced() {
        let names = |env: Vec<(std::ffi::OsString, std::ffi::OsString)>| {
            env.into_iter()
                .map(|(k, v)| format!("{}={}", k.to_string_lossy(), v.to_string_lossy()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(super::host_env("t1", None, "/sys".as_ref())),
            [
                "MOORING_ENV=1",
                "MOORING_TERMINAL=t1",
                // Under the old name too, for Agents and hooks set up under Keepline,
                // and under the product's name for what is written for Fibo.
                "KEEPLINE_ENV=1",
                "FIBO_ENV=1",
                "KEEPLINE_TERMINAL=t1",
                "FIBO_TERMINAL=t1"
            ]
        );
        let host = ("/run/k".into(), "/app/mooring".into());
        let path = format!(
            "PATH={}",
            super::shell::on_path("/sys".as_ref(), "/app".as_ref()).to_string_lossy()
        );
        assert_eq!(
            names(super::host_env("t2", Some(&host), "/sys".as_ref())),
            [
                "MOORING_ENV=1",
                "MOORING_TERMINAL=t2",
                "MOORING_RUNTIME=/run/k",
                "MOORING_HOOK=/app/mooring",
                &path,
                "KEEPLINE_ENV=1",
                "FIBO_ENV=1",
                "KEEPLINE_TERMINAL=t2",
                "FIBO_TERMINAL=t2",
                "KEEPLINE_RUNTIME=/run/k",
                "FIBO_RUNTIME=/run/k",
                "KEEPLINE_HOOK=/app/mooring",
                "FIBO_HOOK=/app/mooring"
            ]
        );
    }
}
