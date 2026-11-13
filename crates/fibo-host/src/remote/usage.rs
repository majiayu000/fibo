//! `usage`: the tokens each Agent's own logs say it used since a moment (the phone's
//! midnight), counted line by line by its adapter (`agents::usage_lines`). No price: the
//! machine keeps no price table it could vouch for, and on a subscription a token has no
//! price anyway.
//!
//! A log can run to gigabytes, so each file is read once and only its growth after that,
//! under a time limit; what is left waits for the next ask (`partial`).
use fibo_core::agent::Agent;
use fibo_host::{usage_line, usage_marker, UsageState};
use fibo_remote::protocol as rp;
use memchr::memmem;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, UNIX_EPOCH};

/// A reply within this keeps the phone's 10-second request limit and its link alive.
const READ_FOR: Duration = Duration::from_secs(4);
/// A full read this recent answers again: walking the folders costs more than it tells.
const FRESH_FOR: Duration = Duration::from_secs(20);
/// How far back a `since` may reach.
const MOST_DAYS: u64 = 7;
const DEPTH: usize = 8;

#[derive(Default)]
struct Tally {
    since: u64,
    files: HashMap<PathBuf, Read>,
    /// Replies already counted, across files.
    seen: HashSet<String>,
    totals: BTreeMap<String, (u64, u64)>,
    done_at: Option<Instant>,
}
#[derive(Default)]
struct Read {
    offset: u64,
    /// The running total a CLI logs, at `offset`.
    total: u64,
}
static TALLY: Mutex<Option<Tally>> = Mutex::new(None);

pub(super) fn usage(request: &Value) -> io::Result<Value> {
    let asked = request["since"]
        .as_u64()
        .ok_or_else(|| io::Error::other("usage 需要 since"))?;
    let now = fibo_core::content::now();
    let since = asked.max(now.saturating_sub(MOST_DAYS * 86_400));
    let home = fibo_platform::dirs::home().ok_or_else(|| io::Error::other("没有 HOME"))?;
    let roots: Vec<(Agent, PathBuf)> = fibo_host::agent_log_roots(&home)
        .into_iter()
        .filter(|(a, _)| usage_marker(*a).is_some())
        .collect();
    let mut slot = TALLY
        .lock()
        .map_err(|_| io::Error::other("usage lock poisoned"))?;
    // A new day starts over.
    if slot.as_ref().is_none_or(|t| t.since != since) {
        *slot = Some(Tally {
            since,
            ..Default::default()
        });
    }
    let tally = slot.as_mut().expect("just set");
    let partial = match tally.done_at {
        Some(at) if at.elapsed() < FRESH_FOR => false,
        _ => !tally.read(&roots, Instant::now() + READ_FOR),
    };
    tally.done_at = (!partial).then(Instant::now);
    Ok(json!(rp::Usage {
        since,
        partial,
        agents: (tally.totals.iter())
            .map(|(agent, (tokens, cached))| rp::AgentUsage {
                agent: agent.clone(),
                agent_name: Agent::all()
                    .into_iter()
                    .find(|a| crate::control::agent_label(*a) == *agent)
                    .map_or_else(|| agent.clone(), |a| a.name().to_owned()),
                tokens: *tokens,
                cached: *cached,
            })
            .collect(),
    }))
}

impl Tally {
    /// Reads what is new; false when time ran out first.
    fn read(&mut self, roots: &[(Agent, PathBuf)], until: Instant) -> bool {
        for (agent, root) in roots {
            let mut files = Vec::new();
            logs(root, self.since, 0, &mut files);
            for path in files {
                if !self.read_file(*agent, path, until) {
                    return false;
                }
            }
        }
        true
    }
    fn read_file(&mut self, agent: Agent, path: PathBuf, until: Instant) -> bool {
        let (Some(marker), Ok(mut file)) = (usage_marker(agent), std::fs::File::open(&path)) else {
            return true;
        };
        let at = self.files.entry(path).or_default();
        let (mut offset, mut total) = (at.offset, at.total);
        let len = file.metadata().map_or(0, |m| m.len());
        if len == offset {
            return true;
        }
        // A file that shrank was rewritten; counted replies stay counted by their ids.
        if len < offset {
            (offset, total) = (0, 0);
        }
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return true;
        }
        let mut lines = BufReader::with_capacity(1 << 18, file);
        let mut line = Vec::new();
        let mut finished = true;
        loop {
            if Instant::now() >= until {
                finished = false;
                break;
            }
            line.clear();
            match lines.read_until(b'\n', &mut line) {
                // A line still being written waits for the next read.
                Ok(n) if n > 0 && line.ends_with(b"\n") => offset += n as u64,
                _ => break,
            }
            if memmem::find(&line, marker).is_none() {
                continue;
            }
            let mut state = UsageState {
                seen: &mut self.seen,
                running: &mut total,
            };
            if let Some((tokens, cached)) = usage_line(agent, &line, self.since, &mut state) {
                let sum = self.totals.entry(agent.key().to_lowercase()).or_default();
                sum.0 += tokens;
                sum.1 += cached;
            }
        }
        (at.offset, at.total) = (offset, total);
        finished
    }
}

/// `.jsonl` files under `dir` changed since `since`.
fn logs(dir: &Path, since: u64, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            if depth < DEPTH {
                logs(&path, since, depth + 1, out);
            }
        } else if path.extension().is_some_and(|x| x == "jsonl") {
            let changed = (entry.metadata().and_then(|m| m.modified()).ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .is_some_and(|d| d.as_secs() >= since);
            if changed {
                out.push(path);
            }
        }
    }
}
