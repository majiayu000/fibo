//! `dirs`: this machine's folders for the phone's 新会话 picker. Lists one folder, or
//! finds folders by name under home; skips hidden ones and the usual build trees. Each
//! says whether it is a git repository (branch, changed files, linked work tree), how
//! many live conversations work in it, and whether a terminal is in it now. Full role
//! only: it shows the machine's layout.
use mooring_core::{model::*, protocol::*};
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SKIPPED: &[&str] = &["node_modules", ".git", "target"];
const FOUND: usize = 50;
const DEPTH: usize = 6;
/// How long a `query` walks before it answers with what it found.
const WALK: Duration = Duration::from_secs(1);
/// How long counting changes in all listed repositories may take; slower ones say none.
const STATUS: Duration = Duration::from_secs(3);
const AT_ONCE: usize = 8;

fn home() -> io::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("没有家目录"))
}
/// `~` and `~/…` from home; anything else must be absolute.
pub(super) fn expand(path: &str) -> io::Result<PathBuf> {
    let path = path.trim();
    let full = match path.strip_prefix('~') {
        Some("") => home()?,
        Some(rest) if rest.starts_with('/') => home()?.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(path),
    };
    if !full.is_absolute() {
        return Err(io::Error::other("路径要从 / 或 ~ 开始"));
    }
    Ok(full)
}

pub(super) fn dirs(socket: &Path, request: &Value) -> io::Result<Value> {
    let w = match rpc(socket, &Request::Workspace)? {
        Reply::Workspace(w) => w,
        other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    };
    let home = home()?;
    let live = Live::of(&w);
    let query = request["query"]
        .as_str()
        .map(str::trim)
        .filter(|q| !q.is_empty());
    let mut truncated = false;
    let (path, parent, found) = match query {
        Some(q) => {
            let (found, cut) = find(&home, q, WALK);
            truncated = cut;
            (String::new(), None, found)
        }
        None => {
            let path = match request["path"].as_str().filter(|p| !p.trim().is_empty()) {
                Some(p) => expand(p)?,
                None => home.clone(),
            };
            if !path.is_dir() {
                return Err(io::Error::other("文件夹不存在"));
            }
            let parent = path.parent().map(|p| p.to_string_lossy().into_owned());
            (
                path.to_string_lossy().into_owned(),
                parent,
                children(&path)?,
            )
        }
    };
    let mut dirs = describe(found, &live);
    // Where the user works first, then repositories, then by name.
    dirs.sort_by_cached_key(|d| (!d.recent, d.git.is_none(), d.name.to_lowercase()));
    Ok(json!(rp::Dirs {
        path,
        parent,
        home: home.to_string_lossy().into_owned(),
        dirs,
        recent: live.recent,
        truncated,
    }))
}

fn listed(name: &str) -> bool {
    !name.starts_with('.') && !SKIPPED.contains(&name)
}
/// The folders directly in `dir`, by name.
fn children(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_str().is_some_and(listed))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    Ok(out)
}
/// Folders under `home` whose name holds `query`, nearest first, and whether `walk` ran
/// out with folders still unread.
fn find(home: &Path, query: &str, walk: Duration) -> (Vec<PathBuf>, bool) {
    let (query, until) = (query.to_lowercase(), Instant::now() + walk);
    let mut queue = VecDeque::from([(home.to_owned(), 0)]);
    let mut found = Vec::new();
    while let Some((dir, depth)) = queue.pop_front() {
        if found.len() >= FOUND {
            break;
        }
        if Instant::now() >= until {
            return (found, true);
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name();
            let Some(name) = name.to_str().filter(|n| listed(n)) else {
                continue;
            };
            // macOS keeps its app data here: deep, and no project lives in it.
            if depth == 0 && name == "Library" {
                continue;
            }
            // Not followed: a link loop must not hold the walk.
            if !e.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            if name.to_lowercase().contains(&query) && found.len() < FOUND {
                found.push(e.path());
            }
            if depth + 1 < DEPTH {
                queue.push_back((e.path(), depth + 1));
            }
        }
    }
    (found, false)
}

/// What the workspace says about folders: live conversations' directories, and the
/// directories terminals work in now, most recently changed first.
struct Live {
    sessions: Vec<PathBuf>,
    recent: Vec<String>,
}
impl Live {
    fn of(w: &Workspace) -> Self {
        let running = || {
            w.terminals
                .iter()
                .filter(|t| t.state == ProcessState::Running)
        };
        let sessions = running()
            .filter(|t| w.current_agent(&t.id).is_some())
            .map(|t| PathBuf::from(w.terminal_cwd(t)))
            .collect();
        let mut by_time: Vec<_> = running()
            .map(|t| {
                let at = w.current_agent(&t.id).map_or(0, |s| w.session_updated(s));
                (at, w.terminal_cwd(t))
            })
            .filter(|(_, cwd)| !cwd.is_empty())
            .collect();
        by_time.sort_by_key(|t| std::cmp::Reverse(t.0));
        let mut recent: Vec<String> = Vec::new();
        for (_, cwd) in by_time {
            if !recent.contains(&cwd) {
                recent.push(cwd);
            }
        }
        Self { sessions, recent }
    }
}

fn describe(found: Vec<PathBuf>, live: &Live) -> Vec<rp::Dir> {
    let repos: Vec<Option<Repo>> = found.iter().map(|p| Repo::at(p)).collect();
    let changes = changes(&found, &repos);
    (found.iter().zip(repos).zip(changes))
        .map(|((path, repo), changes)| rp::Dir {
            name: path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            path: path.to_string_lossy().into_owned(),
            git: repo.map(|r| rp::GitInfo {
                branch: r.branch,
                changes,
                worktree: r.worktree,
            }),
            sessions: live.sessions.iter().filter(|s| s.starts_with(path)).count() as u32,
            recent: live.recent.iter().any(|r| Path::new(r).starts_with(path)),
        })
        .collect()
}

/// A repository's top, read from its files without running git.
struct Repo {
    branch: String,
    worktree: bool,
}
impl Repo {
    fn at(dir: &Path) -> Option<Self> {
        let dot = dir.join(".git");
        let meta = std::fs::symlink_metadata(&dot).ok()?;
        let (gitdir, worktree) = if meta.is_dir() {
            (dot, false)
        } else {
            // A linked work tree (or a submodule): `gitdir: <path>`.
            let text = std::fs::read_to_string(&dot).ok()?;
            let target = PathBuf::from(text.strip_prefix("gitdir:")?.trim());
            let target = if target.is_absolute() {
                target
            } else {
                dir.join(target)
            };
            let linked = target.parent().is_some_and(|p| p.ends_with("worktrees"));
            (target, linked)
        };
        let head = std::fs::read_to_string(gitdir.join("HEAD")).unwrap_or_default();
        let branch = (head.trim().strip_prefix("ref: refs/heads/"))
            .unwrap_or_default()
            .to_owned();
        Some(Self { branch, worktree })
    }
}

/// Changed and untracked files in each repository, a few at a time however many a folder
/// holds; those not done within `STATUS` are left uncounted.
fn changes(found: &[PathBuf], repos: &[Option<Repo>]) -> Vec<Option<u32>> {
    let (tx, rx) = mpsc::channel();
    let until = Instant::now() + STATUS;
    let mut waiting = (found.iter().zip(repos).enumerate())
        .filter(|(_, (_, repo))| repo.is_some())
        .map(|(i, (dir, _))| (i, dir));
    let mut running: HashMap<usize, Child> = HashMap::new();
    let mut out = vec![None; found.len()];
    loop {
        while running.len() < AT_ONCE {
            let Some((i, dir)) = waiting.next() else {
                break;
            };
            let Ok(mut child) = status_command(dir).spawn() else {
                continue;
            };
            // Read beside the others, so a long listing never fills its pipe and stalls.
            let mut stdout = child.stdout.take().expect("piped");
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let read = stdout.read_to_end(&mut bytes).is_ok();
                let _ = tx.send((i, read.then_some(bytes)));
            });
            running.insert(i, child);
        }
        if running.is_empty() {
            break;
        }
        let left = until.saturating_duration_since(Instant::now());
        let Ok((i, bytes)) = rx.recv_timeout(left) else {
            break;
        };
        if let Some(mut child) = running.remove(&i) {
            let ok = child.wait().is_ok_and(|s| s.success());
            out[i] = bytes.filter(|_| ok).map(|b| count(&b));
        }
    }
    for child in running.values_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    out
}

fn status_command(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C")
        .arg(dir)
        .args(["-c", "core.fsmonitor=false", "status", "--porcelain", "-z"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    c
}

/// Entries in `git status --porcelain -z`; a rename's old path follows it unmarked.
fn count(bytes: &[u8]) -> u32 {
    let mut parts = bytes.split(|b| *b == 0).filter(|p| !p.is_empty());
    let mut n = 0;
    while let Some(p) = parts.next() {
        n += 1;
        if p.starts_with(b"R") || p.starts_with(b"C") {
            parts.next();
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A walk out of time says so, for the phone's 「没搜完」.
    #[test]
    fn a_walk_out_of_time_says_it_did_not_finish() {
        let home = std::env::temp_dir().join(format!("kl-find-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(home.join("sub/wanted")).unwrap();
        let (found, cut) = find(&home, "wanted", Duration::ZERO);
        assert!(cut && found.is_empty(), "{found:?}");
        let (found, cut) = find(&home, "wanted", Duration::from_secs(30));
        assert!(!cut && found == [home.join("sub/wanted")], "{found:?}");
        std::fs::remove_dir_all(home).unwrap();
    }
}
