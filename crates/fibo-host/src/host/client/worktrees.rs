//! On-demand local disk inspection. A result is advisory until it is checked again by the host.
use super::*;
use crate::diagnostics::command_output;
use fibo_core::diagnostics::ReclaimableWorktree;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

const GIT_WAIT: Duration = Duration::from_secs(60);
static CLEANUP: Mutex<Option<PathBuf>> = Mutex::new(None);

struct Cleanup;

impl Cleanup {
    fn reserve(path: &Path) -> io::Result<Self> {
        let mut pending = lock(&CLEANUP)?;
        if pending.is_some() {
            return Err(io::Error::other(t!("另一个工作区正在清理")));
        }
        *pending = Some(path.to_owned());
        Ok(Self)
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(mut pending) = CLEANUP.lock() {
            *pending = None;
        }
    }
}

pub(super) fn check_launch(cwd: &Path) -> io::Result<()> {
    let pending = lock(&CLEANUP)?;
    if pending
        .as_ref()
        .is_some_and(|path| canonical(cwd.to_owned()).starts_with(path))
    {
        return Err(io::Error::other(t!("这个工作区正在清理，请稍后再打开终端")));
    }
    Ok(())
}

struct Worktree {
    path: PathBuf,
    head: String,
    branch: String,
}

struct Repository {
    root: PathBuf,
    upstream: String,
    linked: Vec<Worktree>,
}

struct WorkspaceDirs {
    roots: Vec<PathBuf>,
    spaces: Vec<(PathBuf, String)>,
    live: Vec<PathBuf>,
}

impl WorkspaceDirs {
    fn read(shared: &Shared) -> io::Result<Self> {
        let host = lock(shared)?;
        let w = &host.workspace;
        let spaces: Vec<_> = w
            .spaces
            .iter()
            .filter(|s| !s.cwd.is_empty())
            .map(|s| (PathBuf::from(&s.cwd), s.name.clone()))
            .collect();
        let live: Vec<_> = w
            .terminals
            .iter()
            .filter(|t| matches!(t.state, ProcessState::Starting | ProcessState::Running))
            .map(|t| PathBuf::from(w.terminal_cwd(t)))
            .collect();
        let roots = spaces
            .iter()
            .map(|(dir, _)| dir.clone())
            .chain(live.iter().cloned())
            .chain(w.space_last_cwd.values().map(PathBuf::from))
            .collect();
        drop(host);
        Ok(Self {
            roots,
            spaces: spaces
                .into_iter()
                .map(|(dir, name)| (canonical(dir), name))
                .collect(),
            live: live.into_iter().map(canonical).collect(),
        })
    }

    fn space(&self, path: &Path) -> Option<&str> {
        self.spaces
            .iter()
            .find(|(dir, _)| dir.starts_with(path))
            .map(|(_, name)| name.as_str())
    }
}

fn canonical(path: PathBuf) -> PathBuf {
    fibo_platform::fs::canonical(&path).unwrap_or(path)
}

fn process_dirs() -> io::Result<Vec<PathBuf>> {
    let pids = fibo_platform::inspect::all_pids();
    if !pids.contains(&std::process::id()) {
        return Err(io::Error::other(t!("无法读取进程目录，未检查工作区")));
    }
    Ok(pids
        .into_iter()
        .filter_map(fibo_platform::inspect::cwd)
        .map(PathBuf::from)
        .collect())
}

fn in_use(w: &WorkspaceDirs, processes: &[PathBuf], path: &Path) -> bool {
    w.live
        .iter()
        .chain(processes)
        .any(|cwd| cwd.starts_with(path))
}

fn git_command(cwd: &Path) -> Command {
    let mut command = fibo_core::git::command(cwd);
    // The host may have inherited a caller's Git context. Here only -C selects a repository.
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
    ] {
        command.env_remove(name);
    }
    command
}

fn git<I: IntoIterator<Item = impl AsRef<OsStr>>>(
    stream: &local::Stream,
    cwd: &Path,
    args: I,
) -> io::Result<String> {
    let mut command = git_command(cwd);
    command.args(args);
    String::from_utf8(command_output(&mut command, stream, Some(GIT_WAIT))?)
        .map_err(io::Error::other)
}

fn repositories(stream: &local::Stream, dirs: &WorkspaceDirs) -> io::Result<Vec<Repository>> {
    let mut seen = HashSet::new();
    let mut repos = Vec::new();
    let mut roots = dirs.roots.clone();
    roots.sort();
    roots.dedup();
    for dir in roots.into_iter().filter(|d| d.is_absolute() && d.is_dir()) {
        if !stream.is_open() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "scan cancelled",
            ));
        }
        let Ok(list) = git(stream, &dir, ["worktree", "list", "--porcelain", "-z"]) else {
            continue;
        };
        let mut entries = list.split("\0\0");
        let Some(root) = entries
            .next()
            .and_then(|s| s.split('\0').next())
            .and_then(|s| s.strip_prefix("worktree "))
            .map(PathBuf::from)
        else {
            continue;
        };
        if !seen.insert(root.clone()) {
            continue;
        }
        // A local default branch can be ahead of upstream. Without the remote's default
        // ref or ancestry proof, there is no cleanup candidate.
        let Ok(upstream) = git(
            stream,
            &dir,
            ["rev-parse", "--verify", "refs/remotes/origin/HEAD^{commit}"],
        ) else {
            continue;
        };
        let linked = entries
            .filter_map(|entry| {
                let mut fields = entry.split('\0');
                let path = PathBuf::from(fields.next()?.strip_prefix("worktree ")?);
                let head = fields.next()?.strip_prefix("HEAD ")?.to_owned();
                let branch = fields.next()?;
                if fields.any(|f| f.starts_with("locked") || f.starts_with("prunable")) {
                    return None;
                }
                let branch = if branch == "detached" {
                    head.chars().take(8).collect()
                } else {
                    branch.strip_prefix("branch refs/heads/")?.to_owned()
                };
                if !path.is_absolute()
                    || fibo_platform::fs::canonical(&path).ok().as_ref() != Some(&path)
                {
                    return None;
                }
                Some(Worktree { path, head, branch })
            })
            .collect();
        repos.push(Repository {
            root,
            upstream: upstream.trim().to_owned(),
            linked,
        });
    }
    Ok(repos)
}

struct Eligibility {
    target: bool,
    entire: bool,
}

fn eligibility(stream: &local::Stream, repo: &Repository, w: &Worktree) -> io::Result<Eligibility> {
    let path = &w.path;
    git(
        stream,
        path,
        ["merge-base", "--is-ancestor", &w.head, &repo.upstream],
    )?;
    let status = git(
        stream,
        path,
        [
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
            "--ignored=matching",
            "--ignore-submodules=none",
            "--no-renames",
        ],
    )?;
    let entries: Vec<_> = status.split_terminator('\0').collect();
    if entries.iter().any(|row| !row.starts_with("!! ")) {
        return Err(io::Error::other(
            "worktree has tracked or untracked changes",
        ));
    }
    let target_path = path.join("target");
    let target = entries.contains(&"!! target/")
        && target_path.symlink_metadata().is_ok_and(|m| m.is_dir())
        && has_cargo_cache_tag(&target_path)
        && git(stream, path, ["ls-files", "-z", "--", "target"])?.is_empty();
    let entire = entries.is_empty() || (target && entries.iter().all(|row| *row == "!! target/"));
    Ok(Eligibility { target, entire })
}

fn has_cargo_cache_tag(target: &Path) -> bool {
    use std::io::Read;
    const SIGNATURE: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55";
    let Ok(mut file) = fibo_platform::fs::open_regular_nofollow(&target.join("CACHEDIR.TAG"))
    else {
        return false;
    };
    let mut signature = [0; SIGNATURE.len()];
    file.read_exact(&mut signature).is_ok() && signature == SIGNATURE
}

fn bytes(stream: &local::Stream, path: &Path) -> io::Result<u64> {
    let mut du = fibo_platform::fs::disk_usage_command(path)?;
    let output = command_output(&mut du, stream, None)?;
    std::str::from_utf8(&output)
        .ok()
        .and_then(|s| s.split_once('\t'))
        .and_then(|(n, _)| n.parse::<u64>().ok())
        .and_then(|n| n.checked_mul(1024))
        .ok_or_else(|| io::Error::other("invalid du result"))
}

fn scan(stream: &local::Stream, shared: &Shared) -> io::Result<Reply> {
    let dirs = WorkspaceDirs::read(shared)?;
    let processes = process_dirs()?;
    let mut rows = Vec::new();
    for repo in repositories(stream, &dirs)? {
        for w in &repo.linked {
            if !stream.is_open() {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "scan cancelled",
                ));
            }
            if in_use(&dirs, &processes, &w.path) {
                continue;
            }
            let Ok(eligible) = eligibility(stream, &repo, w) else {
                continue;
            };
            let Ok(committed) = git(
                stream,
                &w.path,
                ["log", "-1", "--no-show-signature", "--format=%ct", &w.head],
            ) else {
                continue;
            };
            let Ok(committed_at) = committed.trim().parse() else {
                continue;
            };
            let target = w.path.join("target");
            let target_bytes = if eligible.target && dirs.space(&target).is_none() {
                let Ok(size) = bytes(stream, &target) else {
                    continue;
                };
                Some(size)
            } else {
                None
            };
            let Ok(total_bytes) = bytes(stream, &w.path) else {
                continue;
            };
            let space = dirs.space(&w.path).map(str::to_owned);
            rows.push(ReclaimableWorktree {
                path: w.path.to_string_lossy().into_owned(),
                head: w.head.clone(),
                branch: w.branch.clone(),
                committed_at,
                target_bytes,
                total_bytes,
                can_remove: eligible.entire && space.is_none(),
                space,
            });
        }
    }
    rows.sort_by_key(|w| std::cmp::Reverse(w.target_bytes.unwrap_or(0)));
    Ok(Reply::ReclaimableWorktrees(rows))
}

fn reclaim(
    stream: &local::Stream,
    shared: &Shared,
    path: &Path,
    head: &str,
    entire: bool,
) -> io::Result<Reply> {
    let dirs = WorkspaceDirs::read(shared)?;
    let blocked = || io::Error::other(t!("工作区已变化、在使用中或含有要保留的文件，请重新扫描"));
    for repo in repositories(stream, &dirs)? {
        let Some(w) = repo
            .linked
            .iter()
            .find(|w| w.path == path && w.head == head)
        else {
            continue;
        };
        // A terminal already starting is seen in the fresh workspace snapshot. New
        // launches into this path will be refused until the cleanup has finished.
        let _cleanup = Cleanup::reserve(path)?;
        let eligible = eligibility(stream, &repo, w).map_err(|_| blocked())?;
        // Git remove also refuses tracked and untracked changes and a newly locked worktree.
        if git(stream, path, ["rev-parse", "HEAD"])?.trim() != head {
            return Err(blocked());
        }
        let dirs = WorkspaceDirs::read(shared)?;
        if in_use(&dirs, &process_dirs()?, path) || !stream.is_open() {
            return Err(blocked());
        }
        if entire && eligible.entire && dirs.space(path).is_none() {
            // This action has already been confirmed. Do not kill git halfway through
            // unlinking files if the user closes the surface.
            let output = git_command(&repo.root)
                .args(["worktree", "remove", "--"])
                .arg(path)
                .stderr(std::process::Stdio::piped())
                .output()?;
            if !output.status.success() {
                return Err(io::Error::other(
                    String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                ));
            }
        } else if !entire && eligible.target && dirs.space(&path.join("target")).is_none() {
            std::fs::remove_dir_all(path.join("target"))?;
        } else {
            return Err(blocked());
        }
        return Ok(Reply::Ok);
    }
    Err(blocked())
}

pub(super) fn run(stream: &local::Stream, shared: &Shared, request: Request) -> io::Result<Reply> {
    if !fibo_platform::fs::DISK_USAGE_SUPPORTED {
        return Err(io::Error::other(t!("此平台暂不支持扫描工作区")));
    }
    match request {
        Request::ScanWorktrees => scan(stream, shared),
        Request::ReclaimWorktree { path, head, entire } => {
            reclaim(stream, shared, Path::new(&path), &head, entire)
        }
        _ => unreachable!(),
    }
}
