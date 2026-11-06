//! `diff`: what changed in a work tree against HEAD, untracked files included, for the
//! phone's 改动 view. Read only: git runs with `core.fsmonitor` off and no optional locks,
//! so it neither starts a monitor nor rewrites the index. A folder may hold what a stranger
//! wrote: a git directory checked into a project is no repository here, since its config
//! would run commands, and no textconv driver runs. Patches are capped per file and in all;
//! a file past the cap still lists, with its counts and `truncated`.
use mooring_core::model::Workspace;
use mooring_core::protocol::*;
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const FILE_PATCH: usize = 64 * 1024;
const ALL_PATCHES: usize = 512 * 1024;
/// Untracked files listed at most; a tree with more has a missing ignore rule.
const UNTRACKED: usize = 1000;
/// An untracked file larger than this is listed without its lines counted.
const COUNTED: u64 = 1024 * 1024;
/// Room in a line for what wraps the reply (`{"id":…,"ok":…}`).
const LINE_MARGIN: usize = 64;
/// The empty tree: the base for a repository with no commit yet.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

pub(super) fn diff(socket: &Path, request: &Value, role: super::Role) -> io::Result<Value> {
    let (cwd, checks) = match (request["cwd"].as_str(), request["terminal"].as_str()) {
        (Some(cwd), _) if role.allows("dirs") => (super::dirs::expand(cwd)?, Vec::new()),
        (Some(cwd), _) => (super::diff_scope::checked(socket, cwd)?, Vec::new()),
        (None, Some(t)) => match rpc(socket, &Request::Workspace)? {
            Reply::Workspace(w) => {
                let term = (w.terminals.iter().find(|x| x.id == t))
                    .ok_or_else(|| io::Error::other("终端不存在"))?;
                (PathBuf::from(w.terminal_cwd(term)), checks(&w, t))
            }
            other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        },
        (None, None) => return Err(io::Error::other("diff 需要 terminal 或 cwd")),
    };
    let mut diff = read(&cwd, role)?;
    diff.checks = checks;
    fit(&mut diff);
    Ok(json!(diff))
}

/// A reply is one protocol line, and a patch full of quotes and control characters grows
/// when it becomes JSON: patches go, last first, until it fits. Counts all stay.
fn fit(diff: &mut rp::Diff) {
    let mut len = json!(diff).to_string().len();
    for f in diff.files.iter_mut().rev() {
        if len <= rp::MAX_LINE - LINE_MARGIN {
            return;
        }
        if !f.patch.is_empty() {
            len -= json!(f.patch).to_string().len() - 2;
            f.patch.clear();
            f.truncated = true;
            diff.truncated = true;
        }
    }
}

fn git(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C")
        .arg(dir)
        .args(["-c", "core.fsmonitor=false", "-c", "core.quotepath=false"])
        .args(["-c", "safe.bareRepository=explicit"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    c
}
fn output(c: &mut Command) -> io::Result<Option<Vec<u8>>> {
    let out = c.output()?;
    Ok(out.status.success().then_some(out.stdout))
}
/// NUL-separated fields.
fn fields(bytes: &[u8]) -> Vec<String> {
    (bytes.split(|b| *b == 0))
        .filter(|f| !f.is_empty())
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect()
}

/// The diff of the work tree holding `cwd`.
pub(crate) fn read(cwd: &Path, role: super::Role) -> io::Result<rp::Diff> {
    let not_git = || io::Error::other("这里不是 git 仓库");
    if !cwd.is_dir() {
        return Err(io::Error::other("目录不存在"));
    }
    let lesser = !role.allows("dirs");
    // A lesser role's read fetches no object a partial clone lacks, by whatever its config
    // says to fetch with: a missing object reads as a failed git.
    let git = |dir: &Path| {
        let mut c = git(dir);
        if lesser {
            c.env("GIT_NO_LAZY_FETCH", "1");
        }
        c
    };
    // Before anything that could run a filter. All of it sounds the same as a plain folder.
    let foreign = |dir: &Path| -> io::Result<bool> {
        let list = output(git(dir).args(super::diff_config::LIST))?.ok_or_else(not_git)?;
        Ok(super::diff_config::foreign(&list))
    };
    if lesser && foreign(cwd)? {
        return Err(not_git());
    }
    let top = output(git(cwd).args(["rev-parse", "--show-toplevel"]))?.ok_or_else(not_git)?;
    // Only the line's end: a folder's name may itself end in a space.
    let top = String::from_utf8_lossy(&top);
    let top = PathBuf::from(top.strip_suffix('\n').unwrap_or(&top));
    // Git runs in `top` from here on, so `top` is what must be clean.
    if lesser && foreign(&top)? {
        return Err(not_git());
    }
    // A repository holding home would show `~/.ssh` as untracked files. To a role that may
    // not see the machine's folders, it reads as if home were no repository at all.
    if lesser && super::diff_scope::holds_home(&top) {
        return Err(not_git());
    }
    let branch = output(git(&top).args(["symbolic-ref", "--short", "-q", "HEAD"]))?
        .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
        .unwrap_or_default();
    let head = output(git(&top).args(["rev-parse", "--verify", "-q", "HEAD"]))?.is_some();
    let base = if head { "HEAD" } else { EMPTY_TREE };
    let diff = |extra: &[&str]| {
        let mut c = git(&top);
        c.args(["diff", "-M", "--no-color", "--no-ext-diff", "--no-textconv"])
            .args(extra);
        // A submodule is a repository with its own config: don't look inside it.
        if lesser {
            c.args(["--ignore-submodules=dirty", "--submodule=short"]);
        }
        c.args([base, "--"]);
        c
    };
    let mut files = tracked(
        &output(&mut diff(&["--name-status", "-z"]))?.ok_or_else(not_git)?,
        &output(&mut diff(&["--numstat", "-z"]))?.ok_or_else(not_git)?,
    );
    let mut budget = ALL_PATCHES;
    let mut truncated = patches(&mut diff(&[]), &mut files, &mut budget)?;
    let untracked = output(git(&top).args(["ls-files", "--others", "--exclude-standard", "-z"]))?
        .ok_or_else(not_git)?;
    let untracked = fields(&untracked);
    truncated |= untracked.len() > UNTRACKED;
    for path in untracked.into_iter().take(UNTRACKED) {
        let file = new_file(&top, path, &mut budget);
        truncated |= file.truncated;
        files.push(file);
    }
    Ok(rp::Diff {
        cwd: top.to_string_lossy().into_owned(),
        branch,
        added: files.iter().map(|f| f.added).sum(),
        removed: files.iter().map(|f| f.removed).sum(),
        files,
        truncated,
        checks: Vec::new(),
    })
}

/// How many commands the phone's summary names.
const CHECKS: usize = 3;

/// What the conversation in this terminal last tested, newest first, one per command: the
/// records the log made of its exit codes (`Evidence`), never what the Agent said of them.
fn checks(w: &Workspace, terminal: &str) -> Vec<rp::Check> {
    let Some(content) = w.current_agent(terminal).and_then(|s| w.content.get(&s.id)) else {
        return Vec::new();
    };
    let edited = content.files.iter().map(|t| t.at).max().unwrap_or(0);
    let mut out: Vec<rp::Check> = Vec::new();
    for e in content.evidence.iter().rev().filter(|e| e.kind == "test") {
        // `command：counts`, or `command（exit 0，未捕获计数）`.
        let (command, counts) = match e.summary.split_once('：') {
            Some((command, counts)) => (command, counts),
            None => (e.summary.split('（').next().unwrap_or_default(), ""),
        };
        if out.len() == CHECKS {
            break;
        }
        if !out.iter().any(|c| c.command == command) {
            out.push(rp::Check {
                command: command.to_owned(),
                counts: counts.to_owned(),
                ok: e.proves(),
                at: e.at,
                stale: e.at < edited,
            });
        }
    }
    out
}

/// Files `--name-status -z` names, with `--numstat -z`'s counts (same order, same options).
fn tracked(status: &[u8], numstat: &[u8]) -> Vec<rp::DiffFile> {
    let mut files = Vec::new();
    let mut s = fields(status).into_iter();
    while let Some(code) = s.next() {
        let renamed = code.starts_with('R') || code.starts_with('C');
        let path = if renamed { s.nth(1) } else { s.next() };
        let Some(path) = path else { break };
        let status = match code.chars().next() {
            Some('A') => "A",
            Some('D') => "D",
            Some('R') => "R",
            _ => "M",
        };
        files.push(rp::DiffFile {
            path,
            added: 0,
            removed: 0,
            status: status.into(),
            patch: String::new(),
            truncated: false,
        });
    }
    // `added\tremoved\tpath`, or for a rename `added\tremoved\t` then old and new apart.
    let mut n = numstat.split(|b| *b == 0).filter(|f| !f.is_empty());
    for file in &mut files {
        let Some(line) = n.next() else { break };
        let line = String::from_utf8_lossy(line);
        let mut parts = line.splitn(3, '\t');
        file.added = parts.next().and_then(|a| a.parse().ok()).unwrap_or(0);
        file.removed = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        if parts.next().is_some_and(str::is_empty) {
            n.nth(1);
        }
    }
    files
}

/// Each file's section of the full patch, in order, within the caps; a binary file's is
/// empty. Stops git once the whole budget is spent. Whether anything was cut.
fn patches(c: &mut Command, files: &mut [rp::DiffFile], budget: &mut usize) -> io::Result<bool> {
    let mut child = c.stdout(Stdio::piped()).spawn()?;
    let mut lines = BufReader::new(child.stdout.take().expect("piped")).split(b'\n');
    let (mut i, mut cut, mut spent) = (None::<usize>, false, false);
    let mut unread = None;
    while let Some(line) = lines.next().transpose()? {
        if line.starts_with(b"diff --git ") {
            let next = i.map_or(0, |i| i + 1);
            i = Some(next);
            if spent {
                unread = Some(next);
                break;
            }
            continue;
        }
        let Some(file) = i.and_then(|i| files.get_mut(i)) else {
            continue;
        };
        // The header (index, mode, ---/+++), and whatever follows a cut.
        if file.truncated || (file.patch.is_empty() && !line.starts_with(b"@@")) {
            continue;
        }
        let line = String::from_utf8_lossy(&line);
        let need = line.len() + 1;
        if need > *budget || file.patch.len() + need > FILE_PATCH {
            spent |= need > *budget;
            file.truncated = true;
            cut = true;
            continue;
        }
        *budget -= need;
        file.patch.push_str(&line);
        file.patch.push('\n');
    }
    let unread = unread.unwrap_or(files.len());
    for f in files.iter_mut().skip(unread) {
        f.truncated = true;
        cut = true;
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(cut)
}

/// An untracked file as all added: its lines, and its text as a patch while it fits.
fn new_file(top: &Path, path: String, budget: &mut usize) -> rp::DiffFile {
    let mut file = rp::DiffFile {
        path,
        added: 0,
        removed: 0,
        status: "?".into(),
        patch: String::new(),
        truncated: false,
    };
    let full = top.join(&file.path);
    // Not followed: a link must not show a file outside the tree (a key, say).
    let Ok(meta) = std::fs::symlink_metadata(&full) else {
        return file;
    };
    if !meta.is_file() || meta.len() > COUNTED {
        file.truncated = meta.is_file();
        return file;
    }
    let mut bytes = Vec::new();
    if std::fs::File::open(&full)
        .and_then(|f| f.take(COUNTED).read_to_end(&mut bytes))
        .is_err()
        || bytes.contains(&0)
    {
        return file; // unreadable or binary: listed, nothing to show
    }
    let text = String::from_utf8_lossy(&bytes);
    file.added = text.lines().count() as u32;
    if file.added == 0 {
        return file;
    }
    let mut patch = format!("@@ -0,0 +1,{} @@\n", file.added);
    for line in text.lines() {
        if patch.len() + line.len() + 2 > FILE_PATCH.min(*budget) {
            file.truncated = true;
            break;
        }
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    if patch.len() <= *budget {
        *budget -= patch.len();
        file.patch = patch;
    } else {
        file.truncated = true;
    }
    file
}
