//! The 「联网读 /usage」 setting: asks Claude Code what its own `/usage` says and prints the
//! windows for `quota.json`. Claude Code answers it without a model, at no cost, but goes online
//! to do so, which is why it is off until asked. The app runs this every minute; most runs only
//! print what was kept. A read is due 5 to 15 minutes after the last, drawn at random so nothing
//! runs on a clock (1 minute near the limit), and only once Claude Code has been used since: a
//! limit nobody used has not moved. The app and the host each run this; a lock lets one read.
use super::{settings, usage_text};
use fibo_core::model::Agent;
use fibo_core::plugin_command::{self, Failed, Leftovers, Run};
use fibo_platform::fs::write_private;
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WAIT_FROM: u64 = 300;
const WAIT_SPREAD: u64 = 601;
const WAIT_NEAR: u64 = 60;
/// Under the 90 seconds `quota.json` gives this command, so the read ends here, lock released.
/// It usually takes 6 seconds, and over 20 on a machine that is busy.
const READ_TIMEOUT: Duration = Duration::from_secs(80);
/// A lock this old was left by a run something stopped.
const LOCK_STALE: Duration = Duration::from_secs(120);

struct Files {
    saved: PathBuf,
    tried: PathBuf,
    failed: PathBuf,
    lock: PathBuf,
}

pub(super) fn run(data: &Path) -> io::Result<()> {
    let _ = settings::repair(data);
    let files = Files {
        saved: data.join("usage.json"),
        tried: data.join("usage.tried"),
        failed: data.join("usage.failed"),
        lock: data.join("usage.lock"),
    };
    if std::env::var("MOORING_OPTION_USAGE").as_deref() != Ok("1") {
        // Off leaves nothing behind: turned on again, it reads at once, nothing old first.
        for file in [&files.saved, &files.tried, &files.failed] {
            let _ = fs::remove_file(file);
        }
        return print("{}");
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if !due(&files, now) {
        return show(&files);
    }
    let Some(_lock) = Lock::take(&files.lock) else {
        return show(&files);
    };
    // Marked first: a read that hangs or fails waits its turn like any other.
    let wait = WAIT_FROM + u64::from(uuid::Uuid::new_v4().as_bytes()[0]) * WAIT_SPREAD / 256;
    write_private(&files.tried, format!("{now}\n{wait}\n").as_bytes())?;
    match ask(data, now as i64) {
        Ok(doc) => {
            write_private(&files.saved, doc.to_string().as_bytes())?;
            let _ = fs::remove_file(&files.failed);
        }
        Err(why) => fs::write(&files.failed, why)?,
    }
    show(&files)
}

/// A failure stays out loud until a read works: exiting non-zero is what keeps the app's last
/// reading on screen and puts the reason in the plugin's log.
fn show(files: &Files) -> io::Result<()> {
    if let Ok(why) = fs::read_to_string(&files.failed) {
        if !why.is_empty() {
            return Err(io::Error::other(why));
        }
    }
    match fs::read_to_string(&files.saved) {
        Ok(doc) if !doc.is_empty() => print(&doc),
        _ => print("{}"),
    }
}

fn print(text: &str) -> io::Result<()> {
    writeln!(io::stdout(), "{}", text.trim_end())
}

fn due(files: &Files, now: u64) -> bool {
    let Ok(text) = fs::read_to_string(&files.tried) else {
        return true;
    };
    let mut lines = text.lines().map(|l| l.trim().parse::<u64>());
    let (Some(Ok(last)), Some(Ok(wait))) = (lines.next(), lines.next()) else {
        return true;
    };
    let near = fs::read(&files.saved)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|doc| doc["near"] == true);
    let wait = if near { WAIT_NEAR } else { wait };
    if now.saturating_sub(last) < wait {
        return false;
    }
    fs::metadata(&files.tried)
        .and_then(|m| m.modified())
        .is_ok_and(used_since)
}

/// One of Claude Code's conversations was written to since: a `.jsonl` in a project's folder.
fn used_since(since: SystemTime) -> bool {
    let Some(home) = fibo_platform::dirs::home() else {
        return true;
    };
    let projects = crate::agents::hooks::target(Agent::Claude, &home)
        .ok()
        .and_then(|h| h.settings.parent().map(|d| d.join("projects")));
    let Some(Ok(folders)) = projects.map(fs::read_dir) else {
        return false;
    };
    folders.flatten().any(|folder| {
        fs::read_dir(folder.path()).is_ok_and(|mut files| {
            files.any(|f| {
                f.is_ok_and(|f| {
                    f.path().extension().is_some_and(|e| e == "jsonl")
                        && f.metadata()
                            .and_then(|m| m.modified())
                            .is_ok_and(|at| at > since)
                })
            })
        })
    })
}

struct Lock(PathBuf);
impl Lock {
    fn take(path: &Path) -> Option<Lock> {
        let stale = fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|at| at.elapsed().ok())
            .is_some_and(|age| age > LOCK_STALE);
        if stale {
            let _ = fs::remove_dir(path);
        }
        fs::create_dir(path).ok().map(|()| Lock(path.to_owned()))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

/// Claude Code's own installers, and npm's name for it on Windows.
fn find_claude() -> Option<PathBuf> {
    let home = fibo_platform::dirs::home();
    let installed = home
        .iter()
        .flat_map(|h| [h.join(".local/bin"), h.join(".claude/local")]);
    // The login shell's PATH comes up empty when it takes over 3 seconds, as on a busy machine.
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    dirs.extend(installed);
    dirs.iter().find_map(|dir| {
        ["claude", "claude.exe", "claude.cmd"]
            .iter()
            .map(|name| dir.join(name))
            .find(|file| file.is_file())
    })
}

fn ask(data: &Path, now: i64) -> Result<Value, String> {
    let claude = find_claude().ok_or("找不到 claude 命令")?;
    // A run from inside a Claude session must not read as nested in it.
    std::env::remove_var("CLAUDECODE");
    std::env::remove_var("CLAUDE_CODE_ENTRYPOINT");
    let mut env: Vec<(String, String)> = [
        ("DISABLE_TELEMETRY", "1"),
        ("DISABLE_ERROR_REPORTING", "1"),
        ("DISABLE_AUTOUPDATER", "1"),
    ]
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .to_vec();
    // Without USER Claude Code can't find its sign-in in the keychain and reads as signed out,
    // which would look like an account with no limits. A bare environment has none.
    if std::env::var_os("USER").is_none() {
        if let Some(name) = fibo_platform::dirs::user_name() {
            env.push(("USER".to_owned(), name));
        }
    }
    // No session file, hooks, tools or MCP: it must not show up as a conversation of the user's.
    // In its own folder, since Claude Code keeps a little per folder and this is not a project.
    let args = [
        "-p",
        "/usage",
        "--output-format",
        "json",
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--settings",
        r#"{"disableDeepLinkRegistration":"disable"}"#,
        "--no-session-persistence",
    ];
    let done = plugin_command::run(Run {
        program: claude.into_os_string(),
        args: args.iter().map(OsString::from).collect(),
        cwd: Some(data),
        env,
        stdin: None,
        timeout: READ_TIMEOUT,
        keep: 1024 * 1024,
        leftovers: Leftovers::End,
    })
    .map_err(|e| match e {
        Failed::Start(why) => format!("claude 启动不了：{why}"),
        Failed::TimedOut => "claude /usage 超时".to_owned(),
    })?;
    if !done.status.success() {
        return Err(format!("claude /usage 失败：{}", done.said()));
    }
    let said: Value = serde_json::from_slice(&done.stdout)
        .map_err(|e| format!("claude /usage 的输出读不懂：{e}"))?;
    let text = said["result"].as_str().unwrap_or_default();
    if said["is_error"] == true {
        let first = text
            .lines()
            .next()
            .unwrap_or("Claude Code 报错，没有说原因");
        return Err(first.to_owned());
    }
    usage_text::read(text, now)
}
