//! Running one plugin command: the only place that starts one, so its time limit, process
//! group, pipes and `PATH` are decided once. Setup and teardown, hooks, decorations and
//! quota commands differ only in what they hand it.
//!
//! - It runs in its own process group: at its time limit the group ends, not only the shell.
//! - The limit covers the whole run. Output is read on threads of its own and collected as
//!   it comes, so what a command left in the background holding the pipes open cannot keep
//!   the caller waiting past the command's own end.
//! - `PATH` is what a login shell finds, once per process: an app opened from the Dock
//!   inherits only the system directories, where `node` and `gh` are missing though the
//!   terminal the plugin was written in has them.
use crate::process::Group;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

/// How long, once the command has ended, its output is waited for: a child still holding
/// the pipe is not worth more than this.
const DRAIN: Duration = Duration::from_millis(200);
/// How long a login shell gets to say its `PATH`.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(3);
/// macOS's own default, for a process started with no `PATH` at all.
#[cfg(unix)]
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
#[cfg(windows)]
const SYSTEM_PATH: &str = r"C:\Windows\System32;C:\Windows";
/// What runs a declared command. Windows has it only with Git for Windows, and only if that
/// is on `PATH`; without it the command fails to start and says so.
#[cfg(unix)]
const SHELL: &str = "/bin/sh";
#[cfg(windows)]
const SHELL: &str = "sh";

/// What is done with whatever the command leaves running once it has ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leftovers {
    /// It ends too: a command asked for one answer has no business running on.
    End,
    /// It stays: a hook or a setup may mean to start something.
    Keep,
}

pub struct Run<'a> {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<&'a Path>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    pub timeout: Duration,
    /// Kept of each of stdout and stderr; the rest is read and dropped, so a chatty command
    /// is never stopped by a full or closed pipe.
    pub keep: usize,
    pub leftovers: Leftovers,
}
impl<'a> Run<'a> {
    /// `sh -c command`, as every declared command runs.
    pub fn shell(command: &str, timeout: Duration, leftovers: Leftovers) -> Self {
        Self {
            program: SHELL.into(),
            args: vec!["-c".into(), command.into()],
            cwd: None,
            env: vec![],
            stdin: None,
            timeout,
            keep: 64 * 1024,
            leftovers,
        }
    }
}

#[derive(Debug)]
pub struct Finished {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
impl Finished {
    /// The shell could not run or find what it was told to: a broken command, whatever a
    /// kind makes of other non-zero exits.
    pub fn not_runnable(&self) -> bool {
        matches!(self.status.code(), Some(126 | 127))
    }
    /// The last of what it said on stderr, for a report.
    pub fn said(&self) -> String {
        let text = String::from_utf8_lossy(&self.stderr);
        crate::content::short(text.trim(), 300)
    }
    /// How it ended, for a report: the exit code or the signal.
    pub fn how(&self) -> String {
        self.status
            .code()
            .map_or("被信号停止".into(), |c| format!("退出码 {c}"))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Failed {
    /// It could not be started.
    Start(String),
    /// It ran out of time and its process group was ended.
    TimedOut,
}

/// Collects a pipe as it comes; `done` says the pipe closed.
fn collect(
    pipe: impl Read + Send + 'static,
    keep: usize,
) -> (Arc<Mutex<Vec<u8>>>, mpsc::Receiver<()>) {
    let kept = Arc::new(Mutex::new(Vec::new()));
    let (done, closed) = mpsc::channel();
    let into = kept.clone();
    std::thread::spawn(move || {
        let mut pipe = pipe;
        let mut chunk = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let mut kept = into.lock().unwrap_or_else(|e| e.into_inner());
            let room = keep.saturating_sub(kept.len());
            kept.extend_from_slice(&chunk[..n.min(room)]);
        }
        let _ = done.send(());
    });
    (kept, closed)
}

/// Runs it to its end or its time limit. Blocks: never call it on the UI thread.
pub fn run(spec: Run) -> Result<Finished, Failed> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .env("PATH", path())
        .envs(spec.env)
        .stdin(if spec.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = spec.cwd {
        cmd.current_dir(cwd);
    }
    let (mut child, group) = Group::spawn(&mut cmd).map_err(|e| Failed::Start(e.to_string()))?;
    if let (Some(mut stdin), Some(body)) = (child.stdin.take(), spec.stdin) {
        // A command that ignores its input closes it early; that is not a failure.
        std::thread::spawn(move || {
            let _ = stdin.write_all(&body);
        });
    }
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let (out, out_closed) = collect(stdout, spec.keep);
    let (err, err_closed) = collect(stderr, spec.keep);
    let deadline = Instant::now() + spec.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                group.kill();
                let _ = child.wait();
                return Err(Failed::TimedOut);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                group.kill();
                return Err(Failed::Start(e.to_string()));
            }
        }
    };
    if spec.leftovers == Leftovers::End {
        group.kill();
    }
    // One shared wait for both pipes; a closed one answers at once.
    let until = Instant::now() + DRAIN;
    for closed in [out_closed, err_closed] {
        let left = until.saturating_duration_since(Instant::now());
        let _ = closed.recv_timeout(left);
    }
    let take = |kept: Arc<Mutex<Vec<u8>>>| kept.lock().map(|k| k.clone()).unwrap_or_default();
    Ok(Finished {
        status,
        stdout: take(out),
        stderr: take(err),
    })
}

/// The `PATH` plugin commands run with: the login shell's first, then what this process
/// inherited. Asked once; the first command waits for it (commands are never on the UI
/// thread), and a shell that does not answer in time leaves the inherited one.
pub fn path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let inherited = std::env::var("PATH").unwrap_or_else(|_| SYSTEM_PATH.to_owned());
        // Tests must not depend on, or spend a second in, the machine's own shell profile.
        if cfg!(test) || std::env::var_os("MOORING_NO_LOGIN_PATH").is_some() {
            return inherited;
        }
        match user_shell().and_then(|shell| login_path(&shell, LOGIN_TIMEOUT)) {
            Some(login) => merged(&login, &inherited),
            None => inherited,
        }
    })
}

fn merged(first: &str, then: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for dir in first.split(':').chain(then.split(':')) {
        if !dir.is_empty() && !out.contains(&dir) {
            out.push(dir);
        }
    }
    out.join(":")
}

/// `$SHELL`, else the account's: an app started with a clean environment has no `$SHELL`.
#[cfg(unix)]
fn user_shell() -> Option<std::path::PathBuf> {
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

/// Windows gives a program the account's whole `PATH`, so there is no login shell to ask; a
/// `$SHELL` there comes from something like Git Bash and writes `PATH` in its own form.
#[cfg(windows)]
fn user_shell() -> Option<std::path::PathBuf> {
    None
}

/// What `shell` reports as `PATH` when started as a login shell. Anything its profile prints
/// is told apart by a marker on the line.
fn login_path(shell: &Path, timeout: Duration) -> Option<String> {
    const MARK: &str = "__MOORING_LOGIN_PATH__";
    let name = shell.file_name()?.to_str()?;
    let (args, script): (&[&str], String) = match name {
        "zsh" | "bash" | "sh" | "dash" | "ksh" => (
            &["-l", "-i", "-c"],
            format!(r#"printf '\n{MARK}%s\n' "$PATH""#),
        ),
        // fish's $PATH is a list.
        "fish" => (
            &["-l", "-i", "-c"],
            format!(r#"printf '\n{MARK}%s\n' (string join : $PATH)"#),
        ),
        _ => return None,
    };
    let mut cmd = Command::new(shell);
    cmd.args(args)
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let (mut child, group) = Group::spawn(&mut cmd).ok()?;
    let (out, closed) = collect(child.stdout.take()?, 1024 * 1024);
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                group.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    group.kill();
    let _ = closed.recv_timeout(DRAIN);
    let out = out.lock().ok()?;
    String::from_utf8_lossy(&out)
        .lines()
        .find_map(|l| l.strip_prefix(MARK).map(str::to_owned))
        .filter(|p| !p.trim().is_empty())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    /// What it left holding the pipes does not hold the caller past the command's end.
    #[test]
    fn a_background_child_cannot_hold_the_caller() {
        let at = Instant::now();
        let done = run(Run::shell("echo hi; sleep 6 &", secs(5), Leftovers::Keep)).unwrap();
        assert!(at.elapsed() < Duration::from_secs(2), "{:?}", at.elapsed());
        assert!(done.status.success());
        assert_eq!(String::from_utf8_lossy(&done.stdout).trim(), "hi");
    }

    /// At the limit the whole group ends, not only the shell.
    #[test]
    fn the_time_limit_ends_what_it_started() {
        let f = std::env::temp_dir().join(format!("kl-pc-child-{}", std::process::id()));
        let _ = std::fs::remove_file(&f);
        let at = Instant::now();
        let got = run(Run::shell(
            &format!("sleep 30 & echo $! > {}; wait", f.display()),
            Duration::from_millis(300),
            Leftovers::Keep,
        ));
        assert_eq!(got.unwrap_err(), Failed::TimedOut);
        assert!(at.elapsed() < secs(2));
        let pid: i32 = std::fs::read_to_string(&f).unwrap().trim().parse().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if alive {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        let _ = std::fs::remove_file(&f);
        assert!(!alive, "child {pid} outlived the limit");
    }

    #[test]
    fn leftovers_end_only_when_asked() {
        let f = std::env::temp_dir().join(format!("kl-pc-left-{}", std::process::id()));
        for (leftovers, should_live) in [(Leftovers::End, false), (Leftovers::Keep, true)] {
            let _ = std::fs::remove_file(&f);
            run(Run::shell(
                &format!("sleep 30 >/dev/null 2>&1 & echo $! > {}", f.display()),
                secs(5),
                leftovers,
            ))
            .unwrap();
            let pid: i32 = std::fs::read_to_string(&f).unwrap().trim().parse().unwrap();
            std::thread::sleep(Duration::from_millis(100));
            let alive = unsafe { libc::kill(pid, 0) } == 0;
            if alive {
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
            assert_eq!(alive, should_live, "{leftovers:?}");
        }
        let _ = std::fs::remove_file(&f);
    }

    /// A chatty command is cut in what is kept, not by a closed pipe.
    #[test]
    fn output_past_the_limit_is_dropped_without_breaking_the_command() {
        let mut spec = Run::shell(
            "yes | head -c 300000; echo end >&2; exit 4",
            secs(5),
            Leftovers::Keep,
        );
        spec.keep = 1000;
        let done = run(spec).unwrap();
        assert_eq!(done.status.code(), Some(4));
        assert_eq!(done.stdout.len(), 1000);
        assert_eq!(done.said(), "end");
    }

    #[test]
    fn a_command_it_cannot_find_is_told_apart() {
        let done = run(Run::shell("no-such-tool-xyz -v", secs(5), Leftovers::End)).unwrap();
        assert!(done.not_runnable());
        assert!(done.said().contains("not found"), "{}", done.said());
        let fine = run(Run::shell("exit 1", secs(5), Leftovers::End)).unwrap();
        assert!(!fine.not_runnable());
    }

    #[test]
    fn input_reaches_it_and_ignoring_it_is_fine() {
        let mut spec = Run::shell("cat", secs(5), Leftovers::End);
        spec.stdin = Some(b"payload".to_vec());
        assert_eq!(run(spec).unwrap().stdout, b"payload");
        let mut spec = Run::shell("exit 0", secs(5), Leftovers::End);
        spec.stdin = Some(vec![b'x'; 1 << 20]);
        assert!(run(spec).unwrap().status.success());
    }

    fn stub_shell(name: &str, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("kl-pc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("zsh");
        std::fs::write(&exe, format!("#!/bin/sh\n{body}\n")).unwrap();
        Command::new("chmod").arg("+x").arg(&exe).status().unwrap();
        (dir, exe)
    }

    /// A just-written script can be briefly busy (ETXTBSY) while another test's fork still
    /// holds it open; `login_path` reads that as no answer.
    fn login_path_retrying(exe: &Path, timeout: Duration) -> Option<String> {
        (0..20).find_map(|_| {
            login_path(exe, timeout).or_else(|| {
                std::thread::sleep(Duration::from_millis(50));
                None
            })
        })
    }

    /// A profile that prints a banner around it, one that hangs, one that says nothing.
    #[test]
    fn the_login_path_is_told_from_what_a_profile_prints_and_never_hangs() {
        let (dir, exe) = stub_shell(
            "chatty",
            "echo 欢迎; printf '\\n__MOORING_LOGIN_PATH__/opt/x/bin:/usr/bin\\n'; echo bye",
        );
        assert_eq!(
            login_path_retrying(&exe, secs(2)).as_deref(),
            Some("/opt/x/bin:/usr/bin")
        );
        std::fs::remove_dir_all(&dir).unwrap();
        let (dir, exe) = stub_shell("hung", "sleep 30");
        let at = Instant::now();
        assert_eq!(login_path(&exe, Duration::from_millis(200)), None);
        assert!(at.elapsed() < secs(2));
        std::fs::remove_dir_all(&dir).unwrap();
        let (dir, exe) = stub_shell("quiet", "echo nothing here");
        assert_eq!(login_path(&exe, secs(2)), None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(login_path(Path::new("/bin/tcsh"), secs(1)), None);
    }

    #[test]
    fn the_login_path_comes_first_and_nothing_is_lost_or_doubled() {
        assert_eq!(
            merged("/opt/bin:/usr/bin", "/usr/bin:/bin:"),
            "/opt/bin:/usr/bin:/bin"
        );
    }
}
