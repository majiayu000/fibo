//! `TurnChanges`: what a session's current turn did to its files, read when someone opens
//! 看改动 and never kept. It stays out of the workspace snapshot, which every change resends
//! to the window. The log is read from the turn's opening message to its end, on this
//! connection's thread and without the host's lock, so a log of gigabytes costs what the
//! turn wrote and no terminal waits.
use super::*;
use mooring_core::model::Agent;
use mooring_core::turn_diff::{unrecorded, Composer, Disk, TurnChanges};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// One log line kept in memory at most. Past this it holds a pasted file or an image.
const MOST_LINE: u64 = 32 << 20;
/// What one read may take: files it has not reached say they only have counts.
const READ_FOR: Duration = Duration::from_secs(3);
/// A file larger than this is not compared with what the edits left.
const MOST_COMPARED: u64 = 16 << 20;

/// The current turn's files of `session`, newest change first.
pub(super) fn read(shared: &Shared, session: &str) -> io::Result<Reply> {
    let (log, agent, cwd, since, touched, index) = {
        let host = lock(shared)?;
        let s = (host.workspace.agent_sessions.iter())
            .find(|s| s.id == session)
            .ok_or_else(|| io::Error::other("会话不存在"))?;
        let content = host.workspace.content.get(session);
        let touched: Vec<_> = (content.into_iter())
            .flat_map(|c| c.turn_files().rev())
            .map(|t| (t.path.clone(), t.added, t.removed))
            .collect();
        (
            s.transcript.clone(),
            s.runtime,
            s.cwd.clone(),
            content.map_or(0, |c| c.turn_key().0),
            touched,
            host.index.fork_current()?,
        )
    };
    let start = index.turn_start(&log, since)?;
    let mut edits = read_edits(
        agent,
        Path::new(&log),
        start,
        since,
        Instant::now() + READ_FOR,
    )?;
    let mut files = Vec::with_capacity(touched.len());
    for (path, added, removed) in touched {
        let full = absolute(&cwd, &path);
        files.push(match edits.remove(&full) {
            Some(sum) => finish(sum, &full),
            None => unrecorded(&full, added, removed),
        });
    }
    Ok(Reply::TurnChanges(TurnChanges { files }))
}

fn absolute(cwd: &str, path: &str) -> String {
    if Path::new(path).is_absolute() {
        path.to_owned()
    } else {
        Path::new(cwd).join(path).to_string_lossy().into_owned()
    }
}

fn finish(sum: Composer, path: &str) -> mooring_core::turn_diff::TurnFile {
    match std::fs::metadata(path) {
        Err(_) => sum.finish(path, None),
        Ok(m) if m.len() > MOST_COMPARED => sum.finish_as(path, Disk::Same),
        Ok(_) => match std::fs::read(path) {
            Ok(bytes) => sum.finish(path, Some(&String::from_utf8_lossy(&bytes))),
            Err(_) => sum.finish(path, None),
        },
    }
}

/// Every file's edits logged at or after `since`, summed. `start` is the log and offset of
/// the turn's opening message; another log there, or none, means from the top.
fn read_edits(
    agent: Agent,
    log: &Path,
    start: Option<(u64, u64)>,
    since: u64,
    until: Instant,
) -> io::Result<HashMap<String, Composer>> {
    let mut sums: HashMap<String, Composer> = HashMap::new();
    let Some(marker) = crate::agents::edit_marker(agent) else {
        return Ok(sums);
    };
    let file = std::fs::File::open(log)?;
    let inode = file.metadata()?.ino();
    let mut lines = BufReader::with_capacity(1 << 16, file);
    if let Some((_, offset)) = start.filter(|(i, _)| *i == inode) {
        lines.seek(SeekFrom::Start(offset))?;
    }
    let mut line = Vec::new();
    loop {
        line.clear();
        if (&mut lines).take(MOST_LINE).read_until(b'\n', &mut line)? == 0 {
            return Ok(sums);
        }
        if line.len() as u64 == MOST_LINE && !line.ends_with(b"\n") {
            lines.skip_until(b'\n')?;
            continue;
        }
        if Instant::now() > until {
            return Ok(sums);
        }
        if !holds(&line, marker) {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&line) else {
            continue;
        };
        if crate::content::seconds(&value["timestamp"]) < since {
            continue;
        }
        for (path, edit) in crate::agents::logged_edits(agent, &value) {
            sums.entry(path).or_default().apply(edit);
        }
    }
}

fn holds(line: &[u8], marker: &[u8]) -> bool {
    line.windows(marker.len()).any(|w| w == marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooring_core::turn_diff::LineKind;
    use serde_json::json;
    use std::io::Write;

    fn patch(at: &str, path: &str, line: usize, was: &str, now: &str) -> String {
        json!({"type":"user","timestamp":at,"toolUseResult":{"filePath":path,
            "structuredPatch":[{"oldStart":line,"newStart":line,
                "lines":[format!("-{was}"),format!("+{now}")]}]}})
        .to_string()
    }

    /// A folder that goes when the test ends.
    struct Dir(std::path::PathBuf);
    impl Dir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("turn-changes-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn log(lines: &[String]) -> (Dir, std::path::PathBuf, Vec<u64>) {
        let dir = Dir::new();
        let path = dir.0.join("log.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        let mut offsets = Vec::new();
        let mut at = 0;
        for line in lines {
            offsets.push(at);
            writeln!(file, "{line}").unwrap();
            at += line.len() as u64 + 1;
        }
        (dir, path, offsets)
    }
    fn later() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }
    const NOON: &str = "2026-11-06T12:00:00Z";
    const ONE: &str = "2026-11-06T12:00:01Z";
    const NOON_SECS: u64 = 1_793_966_400;

    #[test]
    fn only_edits_of_this_turn_count_and_they_add_up() {
        let (_d, path, at) = log(&[
            patch("2026-11-06T11:00:00Z", "/r/a", 1, "before", "older"),
            json!({"type":"user","timestamp":NOON,"message":{"content":"go"}}).to_string(),
            patch(ONE, "/r/a", 5, "x", "y"),
            patch(ONE, "/r/a", 5, "y", "z"),
            json!({"type":"user","timestamp":ONE,"toolUseResult":"Error"}).to_string(),
        ]);
        let inode = std::fs::metadata(&path).unwrap().ino();
        let sums = read_edits(
            Agent::Claude,
            &path,
            Some((inode, at[1])),
            NOON_SECS,
            later(),
        )
        .unwrap();
        let file = sums.into_iter().next().unwrap().1.finish("/r/a", None);
        let lines: Vec<_> = file.hunks.iter().flat_map(|h| &h.lines).collect();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            (lines[0].kind, lines[0].text.as_str()),
            (LineKind::Removed, "x")
        );
        assert_eq!(
            (lines[1].kind, lines[1].text.as_str()),
            (LineKind::Added, "z")
        );
        assert_eq!(file.edits, 2);
    }

    #[test]
    fn the_time_still_rules_out_older_edits_when_the_start_is_unknown() {
        let (_d, path, _) = log(&[
            patch("2026-11-06T11:00:00Z", "/r/old", 1, "a", "b"),
            patch(ONE, "/r/new", 1, "a", "b"),
        ]);
        // Another log's offset: read from the top.
        let sums = read_edits(Agent::Claude, &path, Some((1, 999)), NOON_SECS, later()).unwrap();
        assert_eq!(sums.keys().collect::<Vec<_>>(), ["/r/new"]);
        let sums = read_edits(Agent::Claude, &path, None, NOON_SECS, later()).unwrap();
        assert_eq!(sums.keys().collect::<Vec<_>>(), ["/r/new"]);
    }

    #[test]
    fn a_cli_whose_log_holds_no_edits_reads_nothing() {
        let (_d, path, _) = log(&[patch(ONE, "/r/a", 1, "a", "b")]);
        let sums = read_edits(Agent::Grok, &path, None, 0, later()).unwrap();
        assert!(sums.is_empty());
    }

    #[test]
    fn out_of_time_keeps_what_was_read() {
        let (_d, path, _) = log(&[patch(ONE, "/r/a", 1, "a", "b")]);
        let past = Instant::now() - Duration::from_secs(1);
        assert!(read_edits(Agent::Claude, &path, None, 0, past)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_file_on_disk_is_compared_a_gone_one_says_so() {
        let dir = Dir::new();
        let there = dir.0.join("f");
        std::fs::write(&there, "z\n").unwrap();
        let (_d, path, _) = log(&[patch(ONE, &there.to_string_lossy(), 1, "a", "z")]);
        let mut sums = read_edits(Agent::Claude, &path, None, 0, later()).unwrap();
        let sum = sums.remove(&*there.to_string_lossy()).unwrap();
        assert_eq!(finish(sum, &there.to_string_lossy()).disk, Disk::Same);
        std::fs::write(&there, "changed\n").unwrap();
        let mut sums = read_edits(Agent::Claude, &path, None, 0, later()).unwrap();
        let sum = sums.remove(&*there.to_string_lossy()).unwrap();
        assert_eq!(finish(sum, &there.to_string_lossy()).disk, Disk::Changed);
        std::fs::remove_file(&there).unwrap();
        let mut sums = read_edits(Agent::Claude, &path, None, 0, later()).unwrap();
        let sum = sums.remove(&*there.to_string_lossy()).unwrap();
        assert_eq!(finish(sum, &there.to_string_lossy()).disk, Disk::Gone);
    }

    #[test]
    fn relative_paths_resolve_against_the_session() {
        assert_eq!(absolute("/r", "a/b"), "/r/a/b");
        assert_eq!(absolute("/r", "/x/y"), "/x/y");
    }
}
