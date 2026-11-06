//! What one turn did to a file, read from the edits the Agent logged: not the steps but
//! their sum, so a line changed twice shows once and a change undone shows not at all.
//!
//! Each edit only shows a window of the file, so the sum is kept as blocks: a window as it
//! was before the turn and as it is now. Outside the blocks the two are the same file, off
//! by a known number of lines. An edit is laid over the blocks it reaches, then each block
//! is diffed afresh. Line numbers that disagree with what is known mean something else
//! wrote to the file meanwhile: the edits are then shown one by one, and the file says so.
use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffOp, capture_diff_slices_deadline, group_diff_ops};
use std::time::{Duration, Instant};

/// Lines of unchanged text kept around a change.
const CONTEXT: usize = 3;
/// Lines one file sends to the window; its counts still cover all of it.
const SHOWN: usize = 2000;
/// What one block may spend finding the smallest diff; past it the diff is still right, only
/// larger.
const DIFF_FOR: Duration = Duration::from_millis(200);

/// One logged hunk: `lines` start with ` `, `-` or `+`. `old_start` is where it applies in
/// the file before its edit, `new_start` in the file after.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawHunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<String>,
}

/// One logged edit of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Created(String),
    Patched(Vec<RawHunk>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    Same,
    Removed,
    Added,
}

/// `old` and `new` are 1-based; 0 where the line is not on that side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old: u32,
    pub new: u32,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub lines: Vec<DiffLine>,
}

/// How far the file on disk still is what the edits left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Disk {
    #[default]
    Same,
    Changed,
    Gone,
}

/// A file as the turn left it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnFile {
    pub path: String,
    /// The file did not exist before this turn.
    pub created: bool,
    /// Edits the log holds of it; 0: the log has none, only the counts its card keeps.
    pub edits: u32,
    /// Net lines, over all of it.
    pub added: u32,
    pub removed: u32,
    pub hunks: Vec<Hunk>,
    /// Some lines past [`SHOWN`] are not in `hunks`.
    pub cut: bool,
    /// The edits disagree with each other: `hunks` are the edits one by one.
    pub one_by_one: bool,
    pub disk: Disk,
}

/// A file as a list shows it. What it changed is asked for when it is chosen, from `lines`:
/// a turn may touch thousands of files and only the one on screen costs anything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSummary {
    pub path: String,
    pub created: bool,
    /// Edits the log holds of it; 0: the log has none, only the counts its card keeps.
    pub edits: u32,
    pub added: u32,
    pub removed: u32,
    /// Where in the log its edits are, oldest first.
    pub lines: Vec<u64>,
}

/// The files of one turn, newest change first. `log` is the inode of the log `lines` index.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnChanges {
    pub log: u64,
    pub files: Vec<FileSummary>,
}

#[derive(Debug)]
struct Block {
    old_start: usize,
    old: Vec<String>,
    new_start: usize,
    new: Vec<String>,
}
impl Block {
    fn new_end(&self) -> usize {
        self.new_start + self.new.len()
    }
}

/// The sum so far of one file's edits, oldest first.
#[derive(Debug, Default)]
pub struct Composer {
    blocks: Vec<Block>,
    /// Every hunk as logged, for when the sum falls apart.
    raw: Vec<RawHunk>,
    created: bool,
    edits: u32,
    broken: bool,
}

fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}
fn lacks_sign(line: &str) -> &str {
    line.get(1..).unwrap_or("")
}

impl Composer {
    pub fn apply(&mut self, edit: Edit) {
        self.edits += 1;
        match edit {
            // A file made anew after this turn's other edits of it: they no longer add up.
            Edit::Created(_) if self.edits > 1 => self.broken = true,
            Edit::Created(text) => {
                self.created = true;
                let new = lines_of(&text);
                self.raw.push(RawHunk {
                    old_start: 0,
                    new_start: 1,
                    lines: new.iter().map(|l| format!("+{l}")).collect(),
                });
                self.blocks.push(Block {
                    old_start: 1,
                    old: Vec::new(),
                    new_start: 1,
                    new,
                });
            }
            Edit::Patched(hunks) => {
                // Earlier hunks of the same edit have already moved this one.
                let mut moved = 0isize;
                for hunk in hunks {
                    if !self.broken {
                        let before: Vec<&str> = (hunk.lines.iter())
                            .filter(|l| !l.starts_with('+'))
                            .map(|l| lacks_sign(l))
                            .collect();
                        let after: Vec<&str> = (hunk.lines.iter())
                            .filter(|l| !l.starts_with('-'))
                            .map(|l| lacks_sign(l))
                            .collect();
                        // A hunk with no old lines names the line before where it goes.
                        let at = hunk.old_start + usize::from(before.is_empty());
                        let at = at.checked_add_signed(moved).unwrap_or(1).max(1);
                        self.broken |= !self.lay(at, &before, &after);
                        moved += after.len() as isize - before.len() as isize;
                    }
                    self.raw.push(hunk);
                }
            }
        }
    }

    /// Lays `after` over `before` at line `at` of the file as it is now; false when `before`
    /// disagrees with what the blocks say is there.
    fn lay(&mut self, at: usize, before: &[&str], after: &[&str]) -> bool {
        let end = at + before.len();
        let grow = after.len() as isize - before.len() as isize;
        let first = self.blocks.partition_point(|b| b.new_end() < at);
        let last = first + self.blocks[first..].partition_point(|b| b.new_start <= end);
        let shift: isize = (self.blocks[..first].iter())
            .map(|b| b.new.len() as isize - b.old.len() as isize)
            .sum();
        let old_of_now = |line: usize| line.checked_add_signed(-shift).unwrap_or(1).max(1);
        let block = if first == last {
            Block {
                old_start: old_of_now(at),
                old: before.iter().map(|l| (*l).to_owned()).collect(),
                new_start: at,
                new: after.iter().map(|l| (*l).to_owned()).collect(),
            }
        } else {
            let reached = &self.blocks[first..last];
            let start = at.min(reached[0].new_start);
            let stop = end.max(reached[reached.len() - 1].new_end());
            let mut old = Vec::new();
            let mut line = start;
            let mut next = reached.iter().peekable();
            while line < stop || next.peek().is_some() {
                match next.peek() {
                    Some(b) if b.new_start == line => {
                        let shared = line.max(at)..b.new_end().min(end);
                        if shared
                            .clone()
                            .any(|n| b.new[n - b.new_start] != before[n - at])
                        {
                            return false;
                        }
                        old.extend(b.old.iter().cloned());
                        line = b.new_end();
                        next.next();
                    }
                    _ => {
                        old.push(before[line - at].to_owned());
                        line += 1;
                    }
                }
            }
            let head = &reached[0];
            let tail = &reached[reached.len() - 1];
            let keep_head = at.saturating_sub(head.new_start).min(head.new.len());
            let keep_tail = end.saturating_sub(tail.new_start).min(tail.new.len());
            let mut new = head.new[..keep_head].to_vec();
            new.extend(after.iter().map(|l| (*l).to_owned()));
            new.extend_from_slice(&tail.new[keep_tail..]);
            Block {
                old_start: if head.new_start <= at {
                    head.old_start
                } else {
                    old_of_now(at)
                },
                old,
                new_start: start,
                new,
            }
        };
        self.blocks.splice(first..last, [block]);
        for b in &mut self.blocks[first + 1..] {
            b.new_start = b.new_start.saturating_add_signed(grow);
        }
        true
    }

    /// Whether `text`, the file now, still holds what the edits left where they left it.
    /// The edits one by one say nothing about where things are now.
    fn holds(&self, text: &str) -> bool {
        if self.broken {
            return true;
        }
        let now: Vec<&str> = text.lines().collect();
        self.blocks.iter().all(|b| {
            let from = b.new_start - 1;
            now.get(from..from + b.new.len())
                .is_some_and(|there| there.iter().zip(&b.new).all(|(a, b)| a == b))
        })
    }

    /// What a list shows of the file. `net` diffs each block as [`Self::finish`] would, so the
    /// two agree; without it the counts are the logged edits' own, for when time has run out.
    pub fn summary(&self, path: &str, lines: Vec<u64>, net: bool) -> FileSummary {
        let (added, removed) = if net && !self.broken {
            self.blocks.iter().fold((0, 0), |(a, r), b| {
                let (x, y) = net_counts(b);
                (a + x, r + y)
            })
        } else {
            let of = |sign: char| {
                (self.raw.iter().flat_map(|h| &h.lines))
                    .filter(|l| l.starts_with(sign))
                    .count() as u32
            };
            (of('+'), of('-'))
        };
        FileSummary {
            path: path.to_owned(),
            created: self.created,
            edits: self.edits,
            added,
            removed,
            lines,
        }
    }

    /// The file as the turn left it; `current` is what it holds now, `None` when it is gone.
    pub fn finish(self, path: &str, current: Option<&str>) -> TurnFile {
        let disk = match current {
            None => Disk::Gone,
            Some(text) if self.holds(text) => Disk::Same,
            Some(_) => Disk::Changed,
        };
        self.finish_as(path, disk)
    }

    /// The same, for a file too large to compare.
    pub fn finish_as(self, path: &str, disk: Disk) -> TurnFile {
        let mut file = TurnFile {
            path: path.to_owned(),
            created: self.created,
            edits: self.edits,
            one_by_one: self.broken,
            disk,
            ..Default::default()
        };
        let mut left = SHOWN;
        if self.broken {
            for hunk in &self.raw {
                add_logged(&mut file, hunk, &mut left);
            }
        } else {
            for block in &self.blocks {
                add_block(&mut file, block, &mut left);
            }
        }
        file
    }
}

/// A file the log says nothing about: all that is known is its counts.
pub fn unrecorded(path: &str, added: u32, removed: u32) -> FileSummary {
    FileSummary {
        path: path.to_owned(),
        added,
        removed,
        ..Default::default()
    }
}

fn net_counts(block: &Block) -> (u32, u32) {
    if block.old.is_empty() || block.new.is_empty() {
        return (block.new.len() as u32, block.old.len() as u32);
    }
    let deadline = Some(Instant::now() + DIFF_FOR);
    let ops = capture_diff_slices_deadline(Algorithm::Patience, &block.old, &block.new, deadline);
    ops.iter()
        .filter(|op| !matches!(op, DiffOp::Equal { .. }))
        .fold((0, 0), |(a, r), op| {
            (
                a + op.new_range().len() as u32,
                r + op.old_range().len() as u32,
            )
        })
}

fn push(hunk: &mut Hunk, file: &mut TurnFile, left: &mut usize, line: DiffLine) {
    match line.kind {
        LineKind::Added => file.added += 1,
        LineKind::Removed => file.removed += 1,
        LineKind::Same => {}
    }
    if *left == 0 {
        file.cut = true;
    } else {
        *left -= 1;
        hunk.lines.push(line);
    }
}

fn add_logged(file: &mut TurnFile, raw: &RawHunk, left: &mut usize) {
    let (mut old, mut new) = (raw.old_start.max(1) as u32, raw.new_start.max(1) as u32);
    let mut hunk = Hunk::default();
    for line in &raw.lines {
        let text = lacks_sign(line).to_owned();
        let (kind, o, n) = match line.as_bytes().first() {
            Some(b'-') => (LineKind::Removed, old, 0),
            Some(b'+') => (LineKind::Added, 0, new),
            _ => (LineKind::Same, old, new),
        };
        old += u32::from(o > 0);
        new += u32::from(n > 0);
        #[rustfmt::skip]
        push(&mut hunk, file, left, DiffLine { kind, old: o, new: n, text });
    }
    if !hunk.lines.is_empty() {
        file.hunks.push(hunk);
    }
}

fn add_block(file: &mut TurnFile, block: &Block, left: &mut usize) {
    let deadline = Some(Instant::now() + DIFF_FOR);
    let ops = capture_diff_slices_deadline(Algorithm::Patience, &block.old, &block.new, deadline);
    let at_old = |i: usize| (block.old_start + i) as u32;
    let at_new = |i: usize| (block.new_start + i) as u32;
    for group in group_diff_ops(ops, CONTEXT) {
        let mut hunk = Hunk::default();
        for op in group {
            let (old_range, new_range) = (op.old_range(), op.new_range());
            if matches!(op, DiffOp::Equal { .. }) {
                for (o, n) in old_range.zip(new_range) {
                    let line = DiffLine {
                        kind: LineKind::Same,
                        old: at_old(o),
                        new: at_new(n),
                        text: block.new[n].clone(),
                    };
                    push(&mut hunk, file, left, line);
                }
                continue;
            }
            for o in old_range {
                let line = DiffLine {
                    kind: LineKind::Removed,
                    old: at_old(o),
                    new: 0,
                    text: block.old[o].clone(),
                };
                push(&mut hunk, file, left, line);
            }
            for n in new_range {
                let line = DiffLine {
                    kind: LineKind::Added,
                    old: 0,
                    new: at_new(n),
                    text: block.new[n].clone(),
                };
                push(&mut hunk, file, left, line);
            }
        }
        if !hunk.lines.is_empty() {
            file.hunks.push(hunk);
        }
    }
}

/// `@@ -a,b +c,d @@` and the lines it counts. Anything else is skipped.
pub fn parse_unified(text: &str) -> Vec<RawHunk> {
    let mut hunks = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some((old_start, old_len, new_start, new_len)) = hunk_header(line) else {
            continue;
        };
        let (mut old, mut new) = (old_len, new_len);
        let mut body = Vec::new();
        while old > 0 || new > 0 {
            let Some(line) = lines.next() else { break };
            match line.as_bytes().first() {
                Some(b'\\') => continue,
                Some(b'-') => old = old.saturating_sub(1),
                Some(b'+') => new = new.saturating_sub(1),
                _ => {
                    old = old.saturating_sub(1);
                    new = new.saturating_sub(1);
                }
            }
            // A context line that was blank may have lost its space.
            body.push(if line.is_empty() {
                " ".to_owned()
            } else {
                line.to_owned()
            });
        }
        hunks.push(RawHunk {
            old_start,
            new_start,
            lines: body,
        });
    }
    hunks
}

fn hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let range = |r: &str| -> Option<(usize, usize)> {
        match r.split_once(',') {
            Some((start, len)) => Some((start.parse().ok()?, len.parse().ok()?)),
            None => Some((r.parse().ok()?, 1)),
        }
    };
    let (a, b) = range(old)?;
    let (c, d) = range(new)?;
    Some((a, b, c, d))
}

#[cfg(test)]
#[path = "turn_diff_tests.rs"]
mod tests;
