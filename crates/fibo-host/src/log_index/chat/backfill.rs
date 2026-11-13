//! Reading back what a log's first read skipped. A first read takes the last 128 KiB, so a
//! long conversation starts part way; when someone scrolls to that point, this reads the
//! stretch before it, newest first, a segment at a time. It has its own connection, holds no
//! lock of the host's, and never touches what the card says: it only adds the rows the
//! skipped lines would have made, keyed by where they are, so nothing is read twice.
use super::super::{err, Elide, Index};
use crate::content::{save_records, Parser};
use fibo_core::content::Source;
use fibo_core::model::Agent;
use rusqlite::{params, OptionalExtension};
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::time::{Duration, Instant};

/// One stretch read and committed at a time: memory holds this much of a log's records at most.
const SEGMENT: u64 = 8 << 20;
/// A single call stops at whichever comes first, so a phone can show progress and ask again.
const CALL_BYTES: u64 = 64 << 20;
const CALL_TIME: Duration = Duration::from_millis(800);
const WANT_MESSAGES: u32 = 50;
/// Between segments: other readers and the index's own writes get the disk and the database.
const PAUSE: Duration = Duration::from_millis(20);
/// A line longer than this past where a segment ends is dropped, like any line too long to keep.
const PAST_END: u64 = SEGMENT;

/// What one call read back.
#[derive(Debug, PartialEq, Eq)]
pub struct Backfilled {
    /// Messages found, user's and Agent's.
    pub added: u32,
    /// The log has been read back to its start.
    pub done: bool,
}

/// The rows of the lines that begin in `[from, end)`, and where the stretch before them ends.
/// The line a segment starts inside belongs to the segment before; the one it ends inside,
/// read on past `end`, to this one.
fn read_segment(
    file: &mut File,
    runtime: Agent,
    path: &str,
    inode: u64,
    from: u64,
    end: u64,
) -> io::Result<(Parser, u64)> {
    let mut parser = Parser::default();
    let mut at = from;
    // Whether the first bytes are the tail of a line that began earlier.
    let mut skipping = false;
    if from > 0 {
        file.seek(SeekFrom::Start(from - 1))?;
        let mut before = [0];
        file.read_exact(&mut before)?;
        skipping = before[0] != b'\n';
    } else {
        file.seek(SeekFrom::Start(0))?;
    }
    let mut first = skipping.then_some(from);
    let mut line_at = from;
    let (mut pending, mut elide, mut discard) = (Vec::new(), Elide::default(), false);
    let mut buffer = [0u8; 16 * 1024];
    'read: loop {
        if at >= end && (skipping || discard || pending.is_empty() || at >= end + PAST_END) {
            break;
        }
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        for byte in &buffer[..n] {
            if at >= end && (skipping || discard || pending.is_empty()) {
                break 'read;
            }
            at += 1;
            if skipping {
                if *byte == b'\n' {
                    skipping = false;
                    line_at = at;
                    first = Some(at);
                }
            } else if *byte == b'\n' {
                if !discard {
                    let source = Source {
                        path: path.to_owned(),
                        inode,
                        offset: line_at,
                    };
                    parser.add(runtime, &pending, source);
                }
                pending.clear();
                elide = Elide::default();
                discard = false;
                line_at = at;
            } else if !discard {
                if pending.len() == 256 * 1024 {
                    pending.clear();
                    discard = true;
                } else if elide.keep(*byte) {
                    pending.push(*byte);
                }
            }
        }
    }
    // No line began here: all of it is the inside of a line longer than a segment. The stretch
    // before still ends where this one began.
    let first = first.filter(|f| *f < end).unwrap_or(from);
    Ok((parser, first))
}

impl Index {
    /// Where a log first read at `origin` is read from by now: 0 once it has all been read.
    pub fn read_from(&self, path: &str, origin: u64) -> io::Result<u64> {
        let start: Option<i64> = self
            .db
            .prepare_cached("SELECT start FROM log_backfill WHERE path=?1 AND origin=?2")
            .map_err(err)?
            .query_row(params![path, origin as i64], |r| r.get(0))
            .optional()
            .map_err(err)?;
        Ok(start.map_or(origin, |s| s as u64))
    }

    /// Reads back the next stretch of a log, ending as soon as there is a page of messages.
    /// A log replaced since, or read from a new place, starts over from there.
    pub fn backfill(&self, runtime: Agent, path: &str) -> io::Result<Backfilled> {
        let Some(card) = self.card(path)?.filter(|c| c.partial_history) else {
            return Ok(Backfilled {
                added: 0,
                done: true,
            });
        };
        let origin = card.from_offset;
        let mut file = File::open(path)
            .map_err(|_| io::Error::other("这份日志已经不在了，读不回更早的部分"))?;
        let meta = file.metadata()?;
        if !fs::symlink_metadata(path)?.is_file() || meta.len() < origin {
            return Err(io::Error::other("这份日志已经变了，读不回更早的部分"));
        }
        let inode = fibo_platform::fs::open_file_id(&file)?.inode;
        let kept: Option<i64> = self
            .db
            .prepare_cached(
                "SELECT start FROM log_backfill WHERE path=?1 AND inode=?2 AND origin=?3",
            )
            .map_err(err)?
            .query_row(params![path, inode as i64, origin as i64], |r| r.get(0))
            .optional()
            .map_err(err)?;
        let mut start = kept.map_or(origin, |s| s as u64);
        let (mut added, mut read) = (0u32, 0u64);
        let began = Instant::now();
        while start > 0 && added < WANT_MESSAGES && read < CALL_BYTES && began.elapsed() < CALL_TIME
        {
            let from = start.saturating_sub(SEGMENT);
            let (parser, first) = read_segment(&mut file, runtime, path, inode, from, start)?;
            let tx = self.db.unchecked_transaction().map_err(err)?;
            // Replaced after this began: these rows belong to a file that is gone.
            let now: Option<i64> = tx
                .prepare_cached("SELECT inode FROM log_files WHERE path=?1")
                .map_err(err)?
                .query_row([path], |r| r.get(0))
                .optional()
                .map_err(err)?;
            if now.is_some_and(|n| n != inode as i64) {
                return Err(io::Error::other("这份日志已经变了，读不回更早的部分"));
            }
            save_records(&tx, path, &parser)?;
            tx.prepare_cached("INSERT OR REPLACE INTO log_backfill VALUES(?1,?2,?3,?4)")
                .map_err(err)?
                .execute(params![path, inode as i64, origin as i64, first as i64])
                .map_err(err)?;
            tx.commit().map_err(err)?;
            added += (parser.records.iter())
                .filter(|r| r.kind == "user" || r.kind == "assistant")
                .count() as u32;
            read += start - first;
            start = first;
            if start > 0 {
                std::thread::sleep(PAUSE);
            }
        }
        Ok(Backfilled {
            added,
            done: start == 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    struct Log {
        dir: PathBuf,
        path: PathBuf,
        index: Index,
    }
    impl Drop for Log {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    impl Log {
        fn key(&self) -> String {
            self.path.to_string_lossy().into_owned()
        }
        fn from(&self) -> u64 {
            let card = self.index.card(&self.key()).unwrap().unwrap();
            self.index.read_from(&self.key(), card.from_offset).unwrap()
        }
        fn chat(&self, before: Option<i64>, limit: usize) -> (Vec<(i64, String)>, bool) {
            let (m, more) = (self.index)
                .chat(&self.key(), self.from(), None, before, limit, false)
                .unwrap();
            (m.into_iter().map(|m| (m.seq, m.text)).collect(), more)
        }
        /// Every message, paging backwards the way a phone does.
        fn pages(&self) -> Vec<String> {
            let mut all = Vec::new();
            let mut before = None;
            loop {
                let (page, more) = self.chat(before, 7);
                before = page.first().map(|m| m.0);
                let mut words: Vec<_> = page.into_iter().map(|m| m.1).collect();
                words.append(&mut all);
                all = words;
                if !more {
                    return all;
                }
            }
        }
        fn read_all(&self) -> u32 {
            let mut total = 0;
            for _ in 0..1000 {
                let got = self.index.backfill(Agent::Codex, &self.key()).unwrap();
                total += got.added;
                if got.done {
                    return total;
                }
            }
            panic!("never done");
        }
    }

    #[test]
    fn a_hook_signal_leaves_the_messages_readable() {
        let l = log((0..5).map(|n| said(n, 0)));
        let before = l.chat(None, 50).0;
        assert!(!before.is_empty());
        let facts = fibo_core::protocol::HookFacts::default();
        l.index.hook(&l.key(), "Stop", &facts).unwrap();
        let signals: i64 = (l.index.db)
            .query_row(
                "SELECT COUNT(*) FROM session_records WHERE path=?1 AND inode=0",
                [l.key()],
                |r| r.get(0),
            )
            .unwrap();
        assert!(signals > 0, "the signal should be stored without a place");
        assert_eq!(l.chat(None, 50).0, before);
    }

    fn said(n: usize, pad: usize) -> String {
        let text = format!("第 {n} 句{}", "。".repeat(pad));
        format!("{{\"type\":\"event_msg\",\"timestamp\":\"2026-11-05T00:00:00Z\",\"payload\":{{\"type\":\"user_message\",\"message\":\"{text}\"}}}}\n")
    }
    const META: &str = "{\"type\":\"session_meta\",\"payload\":{\"id\":\"s1\",\"cwd\":\"/tmp\"}}\n";

    /// A log the index has read only the end of, `lines` long.
    fn log(lines: impl Iterator<Item = String>) -> Log {
        let dir = std::env::temp_dir().join(format!("kl-backfill-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("logs")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let path = dir.join("logs/s1.jsonl");
        let mut file = File::create(&path).unwrap();
        file.write_all(META.as_bytes()).unwrap();
        for line in lines {
            file.write_all(line.as_bytes()).unwrap();
        }
        let mut index = Index::open(&dir, vec![(Agent::Codex, dir.join("logs"))]).unwrap();
        index.update(Agent::Codex, &path, true).unwrap();
        index.flush().unwrap();
        Log { dir, path, index }
    }

    fn words(n: std::ops::Range<usize>) -> Vec<String> {
        n.map(|i| format!("第 {i} 句")).collect()
    }
    fn trimmed(all: Vec<String>) -> Vec<String> {
        all.iter()
            .map(|t| t.trim_end_matches('。').to_owned())
            .collect()
    }

    /// Every message once, in the log's order, on every page, across segments, however the
    /// first read cut the log; and what is older stays older than what was there already.
    #[test]
    fn reading_back_fills_in_older_messages_in_the_logs_order() {
        let n = 20_000;
        let l = log((0..n).map(|i| said(i, 900)));
        let shown = trimmed(l.pages());
        // Only the unbroken end: the first ask kept from its head waits on the far side.
        assert!(shown.len() < n / 10, "{} read at first", shown.len());
        assert_eq!(shown.last().unwrap(), &format!("第 {} 句", n - 1));
        assert!(!shown.contains(&"第 0 句".to_owned()));
        assert!(l.index.card(&l.key()).unwrap().unwrap().partial_history);
        // Each stretch joins on: what was shown, with more before it, nothing between.
        let mut seen = shown.len();
        for _ in 0..1000 {
            let got = l.index.backfill(Agent::Codex, &l.key()).unwrap();
            let now = trimmed(l.pages());
            assert_eq!(now, words(n - now.len()..n), "no hole after a stretch");
            assert!(now.len() >= seen);
            seen = now.len();
            if got.done {
                break;
            }
        }
        assert_eq!(trimmed(l.pages()), words(0..n));
        assert_eq!(l.from(), 0);
    }

    /// The segments stop for a phone at a page of messages: the older ones come when asked.
    #[test]
    fn one_call_brings_about_a_page_and_says_when_it_is_done() {
        let l = log((0..4000).map(|i| said(i, 900)));
        let first = l.index.backfill(Agent::Codex, &l.key()).unwrap();
        assert!(first.added >= WANT_MESSAGES, "{first:?}");
        assert!(!first.done);
        assert!(l.from() > 0);
        l.read_all();
        assert_eq!(l.from(), 0);
        let again = l.index.backfill(Agent::Codex, &l.key()).unwrap();
        assert_eq!((again.added, again.done), (0, true));
    }

    /// What a phone already holds keeps its place: a seq names a place in the log, not the
    /// moment it was read, and paging from it finds what was read back since.
    #[test]
    fn a_cursor_taken_before_reading_back_still_pages_older() {
        let l = log((0..4000).map(|i| said(i, 900)));
        let (page, _) = l.chat(None, 5);
        let (seq, text) = page[0].clone();
        l.read_all();
        let (page2, _) = l.chat(None, 5);
        assert_eq!(
            page2[0],
            (seq, text.clone()),
            "the same message, the same seq"
        );
        let (older, more) = l.chat(Some(seq), 3);
        assert!(more);
        assert!(older.iter().all(|m| m.0 < seq));
        let at = |t: &str| {
            trimmed(vec![t.to_owned()])[0]
                .trim_start_matches("第 ")
                .trim_end_matches(" 句")
                .parse::<usize>()
                .unwrap()
        };
        assert_eq!(at(&older[2].1) + 1, at(&text));
    }

    /// A line far longer than a segment: skipped whole, and what is on both sides of it reads.
    #[test]
    fn a_line_longer_than_a_segment_does_not_stop_it() {
        let huge = format!(
            "{{\"type\":\"blob\",\"x\":\"{}\"}}\n",
            "z".repeat(3 * SEGMENT as usize)
        );
        let lines = (0..50)
            .map(|i| said(i, 10))
            .chain([huge])
            .chain((50..2000).map(|i| said(i, 900)));
        let l = log(lines);
        l.read_all();
        assert_eq!(trimmed(l.pages()), words(0..2000));
    }

    /// The line the first read landed inside is not lost, nor counted twice.
    #[test]
    fn the_line_the_first_read_began_inside_is_read_back() {
        for pad in [7usize, 450, 901] {
            let l = log((0..1500).map(|i| said(i, pad + i % 3)));
            l.read_all();
            assert_eq!(trimmed(l.pages()), words(0..1500), "pad {pad}");
        }
    }

    /// Nothing for a log read whole, or whose card says it read all of it.
    #[test]
    fn a_log_read_whole_has_nothing_to_read_back() {
        let l = log((0..3).map(|i| said(i, 1)));
        let got = l.index.backfill(Agent::Codex, &l.key()).unwrap();
        assert_eq!((got.added, got.done), (0, true));
        assert_eq!(l.from(), 0);
    }

    /// A card that does not say where its first read began has nothing to read back from:
    /// done at once, nothing added, and no guessing from gaps (docs/plan.md: no old formats).
    #[test]
    fn a_card_that_never_said_where_it_began_is_left_as_it_is() {
        let l = log((0..4000).map(|i| said(i, 900)));
        let before = l.pages();
        l.index
            .db
            .execute(
                "UPDATE session_content SET projection=json_set(projection,'$.from_offset',0) WHERE path=?1",
                [l.key()],
            )
            .unwrap();
        let got = l.index.backfill(Agent::Codex, &l.key()).unwrap();
        assert_eq!((got.added, got.done), (0, true));
        assert!(l.pages().len() >= before.len());
    }

    /// A log replaced mid-way starts over: rows of the old file are gone, progress too.
    #[test]
    fn a_replaced_log_starts_over() {
        let mut l = log((0..4000).map(|i| said(i, 900)));
        l.index.backfill(Agent::Codex, &l.key()).unwrap();
        let tmp = l.path.with_extension("new");
        let mut file = File::create(&tmp).unwrap();
        file.write_all(META.as_bytes()).unwrap();
        for i in 0..3000 {
            file.write_all(said(i + 100_000, 900).as_bytes()).unwrap();
        }
        drop(file);
        fs::rename(&tmp, &l.path).unwrap();
        l.index.update(Agent::Codex, &l.path.clone(), true).unwrap();
        l.index.flush().unwrap();
        let origin = l.index.card(&l.key()).unwrap().unwrap().from_offset;
        assert_eq!(l.from(), origin);
        assert!(origin > 0);
        l.read_all();
        assert_eq!(trimmed(l.pages()), words(100_000..103_000));
    }
}
