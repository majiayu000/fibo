//! Being told when files under some directories change, without polling.
//!
//! macOS keeps a journal, so a watch can replay what changed since a checkpoint. The others
//! have none: starting from anything but `current_id` reports `flag::MUST_SCAN` for every
//! root instead, and looking through the roots makes up for it.
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
};

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

/// What an event says happened. The numbers are the ones FSEvents uses, so macOS passes its
/// own along untouched; other platforms report the same meanings.
pub mod flag {
    /// Something below this path changed and cannot be named: look through it.
    pub const MUST_SCAN: u32 = 0x1;
    /// Events were lost here, on the watcher's side and on the system's.
    pub const USER_DROPPED: u32 = 0x2;
    pub const KERNEL_DROPPED: u32 = 0x4;
    /// The ids started over, so a checkpoint no longer identifies what was lost.
    pub const IDS_WRAPPED: u32 = 0x8;
    /// Not a change: the end of a replay.
    pub const HISTORY_DONE: u32 = 0x10;
    /// A watched root itself moved or went away: the watch has to start again.
    pub const ROOT_CHANGED: u32 = 0x20;
    pub const CREATED: u32 = 0x100;
    pub const REMOVED: u32 = 0x200;
    pub const RENAMED: u32 = 0x800;
    pub const MODIFIED: u32 = 0x1000;
    pub const IS_DIR: u32 = 0x20000;
    /// Whatever means a look through the path is needed.
    pub const SCAN: u32 = MUST_SCAN | USER_DROPPED | KERNEL_DROPPED | ROOT_CHANGED;
}

#[derive(Debug)]
pub struct Event {
    pub path: PathBuf,
    pub flags: u32,
    pub id: u64,
}
pub struct Batch {
    pub events: Vec<Event>,
}
pub(crate) struct Callback {
    tx: SyncSender<Batch>,
    overflow: Arc<Mutex<Vec<PathBuf>>>,
    pub(crate) roots: Vec<PathBuf>,
}
impl Callback {
    /// Whether an event is about the roots rather than their watched surroundings.
    pub(crate) fn wants(&self, event: &Event) -> bool {
        event.flags & (flag::SCAN | flag::IDS_WRAPPED) != 0
            || self.roots.iter().any(|root| event.path.starts_with(root))
    }
    /// Storing scopes for what a batch that does not fit left out.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn overflow(&self) -> std::sync::MutexGuard<'_, Vec<PathBuf>> {
        self.overflow.lock().unwrap_or_else(|p| p.into_inner())
    }
}
#[derive(Clone)]
pub struct Notifier(Arc<Callback>);
impl Notifier {
    pub fn file(&self, path: PathBuf) {
        deliver(
            &self.0,
            Batch {
                events: vec![Event {
                    path,
                    flags: 0,
                    id: 0,
                }],
            },
        );
    }
}
pub struct Watch {
    imp: imp::Watch,
    callback: Arc<Callback>,
    pub rx: Receiver<Batch>,
}
impl Watch {
    /// The checkpoint for "from now on".
    pub fn current_id() -> u64 {
        imp::current_id()
    }
    /// Watches `paths`, which need not exist yet, and reports what changed since `since`.
    pub fn start(paths: &[PathBuf], since: u64) -> io::Result<Self> {
        let (tx, rx) = sync_channel(16);
        let callback = Arc::new(Callback {
            tx,
            overflow: Arc::new(Mutex::new(vec![])),
            roots: paths.to_vec(),
        });
        let imp = imp::Watch::start(Arc::clone(&callback), since)?;
        Ok(Self { imp, callback, rx })
    }
    pub fn notifier(&self) -> Notifier {
        Notifier(self.callback.clone())
    }
    /// Reports these files' changes at once, even while a writer keeps them open, and stops
    /// reporting the ones followed earlier that are not listed. Returns the newly followed:
    /// read them now, because what was written before this call is not reported.
    pub fn follow(&self, paths: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
        self.imp.follow(paths)
    }
    /// What did not fit the queue, as directories to look through.
    pub fn take_overflow(&self) -> Vec<PathBuf> {
        std::mem::take(&mut *self.callback.overflow())
    }
}
pub(crate) fn deliver(context: &Callback, batch: Batch) {
    if batch.events.is_empty()
        && context
            .overflow
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_empty()
    {
        return;
    }
    if let Err(error) = context.tx.try_send(batch) {
        let mut scopes = context.overflow.lock().unwrap_or_else(|p| p.into_inner());
        let batch = match error {
            std::sync::mpsc::TrySendError::Full(batch)
            | std::sync::mpsc::TrySendError::Disconnected(batch) => batch,
        };
        for event in batch.events {
            let path = if event.flags & flag::IS_DIR != 0 {
                event.path.as_path()
            } else {
                event.path.parent().unwrap_or(&event.path)
            };
            add_scope(&mut scopes, path, &context.roots);
        }
        // A full queue already contains a wakeup. The worker consumes these scopes atomically.
    }
}
/// Keep disjoint scopes inside configured roots. An event for an ancestor is clipped to its CLI root.
/// Queue memory is bounded; only overflow of scope storage widens recovery to the affected CLI root.
pub fn add_scope(scopes: &mut Vec<PathBuf>, path: &Path, roots: &[PathBuf]) {
    for root in roots {
        let scope = if path.starts_with(root) {
            path.to_owned()
        } else if root.starts_with(path) {
            root.clone()
        } else {
            continue;
        };
        if scopes.iter().any(|old| scope.starts_with(old)) {
            continue;
        }
        scopes.retain(|old| !old.starts_with(&scope));
        scopes.push(scope);
        if scopes.len() > 64 {
            scopes.retain(|old| !old.starts_with(root));
            scopes.push(root.clone());
        }
    }
}

/// Canonicalize existing ancestors too, e.g. /tmp -> /private/tmp, even before roots exist.
pub fn normalized(path: &Path) -> io::Result<PathBuf> {
    match path.canonicalize() {
        Ok(p) => Ok(p),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or(e)?;
            Ok(normalized(parent)?.join(
                path.file_name()
                    .ok_or_else(|| io::Error::other("missing path name"))?,
            ))
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overflow_keeps_only_affected_subtree_and_does_not_latch() {
        let (tx, rx) = sync_channel(1);
        let context = Callback {
            tx,
            overflow: Arc::new(Mutex::new(vec![])),
            roots: vec![PathBuf::from("/logs")],
        };
        let event = |path: &str| Batch {
            events: vec![Event {
                path: PathBuf::from(path),
                flags: 0,
                id: 1,
            }],
        };
        deliver(&context, event("/logs/a/one.jsonl"));
        deliver(&context, event("/logs/b/two.jsonl"));
        deliver(&context, event("/logs/b/deep/three.jsonl"));
        let scopes = std::mem::take(&mut *context.overflow.lock().unwrap());
        assert_eq!(scopes, vec![PathBuf::from("/logs/b")]);
        rx.recv().unwrap();
        deliver(&context, event("/logs/c/four.jsonl"));
        assert_eq!(
            rx.recv().unwrap().events[0].path,
            PathBuf::from("/logs/c/four.jsonl")
        );
        assert!(context.overflow.lock().unwrap().is_empty());
    }
}
