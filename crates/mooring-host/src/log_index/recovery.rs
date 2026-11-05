//! Event-triggered subtree repair.
use super::Index;
use crate::recognition::Transcript;
use mooring_core::model::Agent;
use std::{fs, io, path::PathBuf, time::Duration};

/// Event-triggered subtree repair. Each step visits at most 128 entries or 20 ms;
/// file cursor logic still skips old bodies. No global scan and no retry timer.
pub struct Recovery {
    queue: std::collections::VecDeque<(Agent, PathBuf)>,
    current: Option<(Agent, fs::ReadDir)>,
}
impl Recovery {
    pub fn new(index: &Index, scopes: &[PathBuf]) -> Self {
        Self {
            queue: scopes
                .iter()
                .filter_map(|p| index.runtime(p).map(|a| (a, p.clone())))
                .collect(),
            current: None,
        }
    }
    pub fn step(&mut self, index: &mut Index) -> io::Result<(bool, Vec<Transcript>)> {
        let started = std::time::Instant::now();
        let mut transcripts = vec![];
        for _ in 0..128 {
            if started.elapsed() >= Duration::from_millis(20) {
                break;
            }
            if self.current.is_none() {
                let Some((runtime, path)) = self.queue.pop_front() else {
                    return Ok((true, transcripts));
                };
                if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                    continue;
                }
                index.counts.directories += 1;
                match fs::read_dir(&path) {
                    Ok(entries) => self.current = Some((runtime, entries)),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e),
                }
            }
            let (runtime, entries) = self.current.as_mut().unwrap();
            match entries.next() {
                Some(entry) => {
                    let entry = entry?;
                    let ty = entry.file_type()?;
                    let adapter = crate::agents::adapter(*runtime);
                    if ty.is_dir() {
                        if adapter.descend(&entry.path()) {
                            self.queue.push_back((*runtime, entry.path()));
                        }
                    } else if ty.is_file() && adapter.is_transcript(&entry.path()) {
                        if let Some(t) = index.update(*runtime, &entry.path(), true)? {
                            transcripts.push(t);
                        }
                    }
                }
                None => self.current = None,
            }
        }
        Ok((self.current.is_none() && self.queue.is_empty(), transcripts))
    }
}
