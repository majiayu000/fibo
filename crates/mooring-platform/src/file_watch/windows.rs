//! `ReadDirectoryChangesW` on a completion port. A root is one recursive watch, so a tree costs
//! one handle however many folders it holds. Its parent (or, for a root not there yet, the
//! nearest ancestor that is) gets a watch of names only: nothing else says a root moved, or
//! arrived. There is no journal: see `MUST_SCAN` in the interface.
//!
//! NTFS updates a directory's entry for a file lazily, so an append through a handle its writer
//! keeps open may not be reported until the writer closes it, which for an Agent's log is when
//! the conversation ends. Followed files are therefore looked at on a timer, and only while
//! there are any: with none the thread waits for a change alone, and an idle host never wakes.
use super::{Batch, Callback, Event, deliver, flag};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    io,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FALSE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, TRUE},
    Storage::FileSystem::{
        CreateFileW, FILE_ACTION_ADDED, FILE_ACTION_MODIFIED, FILE_ACTION_REMOVED,
        FILE_ACTION_RENAMED_NEW_NAME, FILE_ACTION_RENAMED_OLD_NAME, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_DIR_NAME,
        FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
        FILE_NOTIFY_INFORMATION, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING, ReadDirectoryChangesW,
    },
    System::IO::{
        CancelIoEx, CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED,
        PostQueuedCompletionStatus,
    },
};

const INFINITE: u32 = u32::MAX;
const STOP: usize = 0;
/// `follow` changed what there is to look at: the thread's next wait may be a different one.
const LOOK: usize = 1;
const FIRST_DIR: usize = 2;
/// How late an append through an open handle can be reported.
const FOLLOW_EVERY: Duration = Duration::from_millis(250);
const BUFFER_WORDS: usize = 16 * 1024;
const STOP_WITHIN: Duration = Duration::from_secs(5);

/// Ids count up from the clock, so one from an earlier run is older than any from this one.
static LAST_ID: AtomicU64 = AtomicU64::new(0);
/// What `current_id` gave out and nothing has started from yet. Starting from one loses
/// nothing, once: starting from it again means time passed unwatched.
static NOW_IDS: Mutex<Option<HashSet<u64>>> = Mutex::new(None);

fn next_id() -> u64 {
    let clock = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    LAST_ID.fetch_max(clock, Ordering::Relaxed);
    LAST_ID.fetch_add(1, Ordering::Relaxed) + 1
}

pub fn current_id() -> u64 {
    let id = next_id();
    let mut ids = NOW_IDS.lock().unwrap_or_else(|p| p.into_inner());
    ids.get_or_insert_default().insert(id);
    id
}

struct Port(HANDLE);
// SAFETY: a completion port is made for use from any thread.
unsafe impl Send for Port {}
unsafe impl Sync for Port {}
impl Drop for Port {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// One directory handle with its read outstanding. Boxed parts stay where the system was told
/// they are while `self` moves.
struct Dir {
    handle: HANDLE,
    path: PathBuf,
    recursive: bool,
    buffer: Box<[u32; BUFFER_WORDS]>,
    overlapped: Box<OVERLAPPED>,
}
// SAFETY: owned by the one thread that runs the watch, and only moved to it.
unsafe impl Send for Dir {}

impl Dir {
    fn open(path: &Path, recursive: bool, port: &Port, key: usize) -> io::Result<Dir> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: the name ends in a zero, and the flag is what opens a directory at all.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut dir = Dir {
            handle,
            path: path.to_owned(),
            recursive,
            buffer: Box::new([0; BUFFER_WORDS]),
            overlapped: Box::new(unsafe { std::mem::zeroed() }),
        };
        // SAFETY: both handles are open.
        if unsafe { CreateIoCompletionPort(handle, port.0, key, 0) }.is_null() {
            return Err(io::Error::last_os_error());
        }
        dir.read()?;
        Ok(dir)
    }

    fn read(&mut self) -> io::Result<()> {
        let filter = if self.recursive {
            FILE_NOTIFY_CHANGE_FILE_NAME
                | FILE_NOTIFY_CHANGE_DIR_NAME
                | FILE_NOTIFY_CHANGE_SIZE
                | FILE_NOTIFY_CHANGE_LAST_WRITE
        } else {
            FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME
        };
        // SAFETY: the buffer is 4-byte aligned, and it and the OVERLAPPED are boxed and live
        // until `drop` has seen this read end.
        let ok = unsafe {
            ReadDirectoryChangesW(
                self.handle,
                self.buffer.as_mut_ptr().cast(),
                (BUFFER_WORDS * 4) as u32,
                if self.recursive { TRUE } else { FALSE },
                filter,
                std::ptr::null_mut(),
                &mut *self.overlapped,
                None,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// What the read that just finished brought, as (action, full path).
    fn changes(&self, bytes: usize) -> Vec<(u32, PathBuf)> {
        let mut out = vec![];
        let start = self.buffer.as_ptr().cast::<u8>();
        let mut at = 0;
        let header = std::mem::offset_of!(FILE_NOTIFY_INFORMATION, FileName);
        while bytes >= at + header {
            // SAFETY: the system wrote whole, aligned records into the first `bytes` bytes,
            // each saying where the next one starts.
            let record = unsafe { &*start.add(at).cast::<FILE_NOTIFY_INFORMATION>() };
            let units = record.FileNameLength as usize / 2;
            // SAFETY: `FileNameLength` bytes of name follow the header.
            let name =
                unsafe { std::slice::from_raw_parts(start.add(at + header).cast::<u16>(), units) };
            out.push((record.Action, self.path.join(OsString::from_wide(name))));
            if record.NextEntryOffset == 0 {
                break;
            }
            at += record.NextEntryOffset as usize;
        }
        out
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        // SAFETY: nothing is outstanding any more: see `Worker::retire`.
        unsafe { CloseHandle(self.handle) };
    }
}

type Stamp = Option<(u64, Option<SystemTime>)>;

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

pub struct Watch {
    port: Arc<Port>,
    followed: Arc<Mutex<HashMap<PathBuf, Stamp>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Watch {
    pub fn start(callback: Arc<Callback>, since: u64) -> io::Result<Self> {
        // SAFETY: asks for a new port with no file on it yet.
        let raw =
            unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 1) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let port = Arc::new(Port(raw));
        let followed = Arc::new(Mutex::new(HashMap::new()));
        let mut worker = Worker {
            port: port.clone(),
            callback: callback.clone(),
            followed: followed.clone(),
            dirs: HashMap::new(),
            retired: HashMap::new(),
            next_key: FIRST_DIR,
            present: HashSet::new(),
        };
        let mut arrived = vec![];
        worker.catch_up(&mut arrived)?;
        // Only now that the watches are in place, so a change cannot fall between this look
        // and the first event.
        let mut ids = NOW_IDS.lock().unwrap_or_else(|p| p.into_inner());
        if !ids.get_or_insert_default().remove(&since) {
            report_scan(&callback, flag::MUST_SCAN);
        }
        drop(ids);
        let thread = std::thread::spawn(move || worker.run());
        Ok(Self {
            port,
            followed,
            thread: Some(thread),
        })
    }

    pub fn follow(&self, paths: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
        let mut followed = self
            .followed
            .lock()
            .map_err(|_| io::Error::other("file watch lock poisoned"))?;
        followed.retain(|path, _| paths.contains(path));
        let mut added = vec![];
        for path in paths {
            if followed.contains_key(path) {
                continue;
            }
            match std::fs::symlink_metadata(path) {
                Ok(_) => (),
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            }
            followed.insert(path.clone(), stamp(path));
            added.push(path.clone());
        }
        drop(followed);
        post(&self.port, LOOK);
        Ok(added)
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        post(&self.port, STOP);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn post(port: &Port, key: usize) {
    // SAFETY: the port is open for as long as `port` is alive.
    unsafe { PostQueuedCompletionStatus(port.0, 0, key, std::ptr::null()) };
}

fn report_scan(callback: &Callback, flags: u32) {
    let events = callback
        .roots
        .iter()
        .map(|path| Event {
            path: path.clone(),
            flags,
            id: 0,
        })
        .collect();
    deliver(callback, Batch { events });
}

struct Worker {
    port: Arc<Port>,
    callback: Arc<Callback>,
    followed: Arc<Mutex<HashMap<PathBuf, Stamp>>>,
    dirs: HashMap<usize, Dir>,
    /// Cancelled, and the system still holds their buffers until the port says the read ended.
    retired: HashMap<usize, Dir>,
    next_key: usize,
    /// Roots that have been seen to exist, so one arriving later can be told from these.
    present: HashSet<PathBuf>,
}

impl Worker {
    fn watching(&self, path: &Path, recursive: bool) -> bool {
        self.dirs
            .values()
            .any(|d| d.recursive == recursive && d.path == path)
    }

    fn open(&mut self, path: &Path, recursive: bool) -> io::Result<()> {
        if self.watching(path, recursive) {
            return Ok(());
        }
        let key = self.next_key;
        self.next_key += 1;
        let dir = Dir::open(path, recursive, &self.port, key)?;
        self.dirs.insert(key, dir);
        Ok(())
    }

    /// Puts each root's watches where they now belong, and closes those no root needs. A root
    /// may appear while this looks, so it goes on until a look changes nothing. Arrived roots
    /// go in `arrived`: what they already hold has made no event.
    fn catch_up(&mut self, arrived: &mut Vec<Event>) -> io::Result<()> {
        let first = self.dirs.is_empty() && self.present.is_empty();
        loop {
            let mut wanted: Vec<(PathBuf, bool)> = vec![];
            for root in self.callback.roots.clone() {
                if root.is_dir() {
                    wanted.push((root.clone(), true));
                    if let Some(parent) = root.parent() {
                        wanted.push((parent.to_owned(), false));
                    }
                    if self.present.insert(root.clone()) && !first {
                        arrived.push(Event {
                            path: root,
                            flags: flag::CREATED | flag::IS_DIR,
                            id: next_id(),
                        });
                    }
                } else {
                    self.present.remove(&root);
                    if let Some(existing) = root.ancestors().find(|p| p.is_dir()) {
                        wanted.push((existing.to_owned(), false));
                    }
                }
            }
            let mut changed = false;
            for (path, recursive) in &wanted {
                if self.watching(path, *recursive) {
                    continue;
                }
                match self.open(path, *recursive) {
                    Ok(()) => changed = true,
                    // Went away since the look: the next one sees that.
                    Err(e) if e.kind() == io::ErrorKind::NotFound => changed = true,
                    Err(e) => return Err(e),
                }
            }
            self.dirs.retain(|_, d| {
                wanted
                    .iter()
                    .any(|(p, r)| *p == d.path && *r == d.recursive)
            });
            if !changed {
                return Ok(());
            }
        }
    }

    /// Asks a watch's read to end. Its one completion packet says when it has; until then the
    /// system may still write into its buffer, so the `Dir` waits in `retired`.
    fn retire(&mut self, key: usize) {
        if let Some(dir) = self.dirs.remove(&key) {
            // SAFETY: the handle and the OVERLAPPED are the live read's.
            unsafe { CancelIoEx(dir.handle, &*dir.overlapped) };
            self.retired.insert(key, dir);
        }
    }

    /// Ends every read before the buffers go. A system that never answers costs their memory
    /// rather than this thread, or memory the system still writes to.
    fn stop(mut self) {
        let keys: Vec<usize> = self.dirs.keys().copied().collect();
        for key in keys {
            self.retire(key);
        }
        let until = Instant::now() + STOP_WITHIN;
        while !self.retired.is_empty() && Instant::now() < until {
            let (mut moved, mut key) = (0u32, 0usize);
            let mut overlapped = std::ptr::null_mut();
            // SAFETY: the port outlives this thread, and the rest are this thread's.
            let got = unsafe {
                GetQueuedCompletionStatus(self.port.0, &mut moved, &mut key, &mut overlapped, 100)
            };
            if got != 0 || !overlapped.is_null() {
                self.retired.remove(&key);
            }
        }
        for (_, dir) in self.retired.drain() {
            std::mem::forget(dir);
        }
    }

    fn run(mut self) {
        let mut last_look = Instant::now();
        loop {
            let any_followed = !self
                .followed
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .is_empty();
            let wait = if any_followed {
                FOLLOW_EVERY.saturating_sub(last_look.elapsed()).as_millis() as u32
            } else {
                INFINITE
            };
            let (mut moved, mut key) = (0u32, 0usize);
            let mut overlapped = std::ptr::null_mut();
            // SAFETY: the port outlives this thread, and the rest are this thread's.
            let ok = unsafe {
                GetQueuedCompletionStatus(self.port.0, &mut moved, &mut key, &mut overlapped, wait)
            };
            if !overlapped.is_null() || ok != 0 {
                if key == STOP && overlapped.is_null() {
                    return self.stop();
                }
                if self.retired.remove(&key).is_some() {
                    continue;
                }
                if key >= FIRST_DIR {
                    let error = (ok == 0).then(|| unsafe { GetLastError() });
                    self.finished(key, moved as usize, error);
                }
            }
            if any_followed && last_look.elapsed() >= FOLLOW_EVERY {
                last_look = Instant::now();
                self.look_at_followed();
            }
        }
    }

    fn finished(&mut self, key: usize, moved: usize, error: Option<u32>) {
        let Some(dir) = self.dirs.get(&key) else {
            return;
        };
        let (recursive, path) = (dir.recursive, dir.path.clone());
        let mut events = vec![];
        match error {
            // Our own cancel, for a watch no longer wanted.
            Some(windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED) => return,
            // The buffer could not hold it: the changes are lost, not their existence.
            Some(windows_sys::Win32::Foundation::ERROR_NOTIFY_ENUM_DIR) | None if moved == 0 => {
                if recursive {
                    events.push(Event {
                        path: path.clone(),
                        flags: flag::KERNEL_DROPPED,
                        id: next_id(),
                    });
                }
            }
            None => {
                let changes = dir.changes(moved);
                self.translate(changes, recursive, &mut events);
            }
            // The directory itself went away (delete gives access denied).
            Some(code) => {
                if recursive {
                    events.push(Event {
                        path: path.clone(),
                        flags: flag::ROOT_CHANGED,
                        id: next_id(),
                    });
                } else {
                    eprintln!("file watch of {} ended: {code}", path.display());
                }
                self.dirs.remove(&key);
            }
        }
        if let Some(dir) = self.dirs.get_mut(&key)
            && let Err(e) = dir.read()
        {
            eprintln!("file watch of {} ended: {e}", path.display());
            events.push(Event {
                path: path.clone(),
                flags: flag::ROOT_CHANGED,
                id: next_id(),
            });
            self.dirs.remove(&key);
        }
        // What moved near a root may be the root, or the next step on the way to one.
        if !recursive && let Err(e) = self.catch_up(&mut events) {
            eprintln!("file watch failed: {e}");
            report_scan(&self.callback, flag::ROOT_CHANGED);
        }
        events.retain(|e| self.callback.wants(e));
        deliver(&self.callback, Batch { events });
    }

    fn translate(&self, changes: Vec<(u32, PathBuf)>, recursive: bool, events: &mut Vec<Event>) {
        let mut seen = HashSet::new();
        for (action, path) in changes {
            if !seen.insert((action, path.clone())) {
                continue;
            }
            let id = next_id();
            if !recursive {
                let left = action == FILE_ACTION_REMOVED || action == FILE_ACTION_RENAMED_OLD_NAME;
                if left && self.callback.roots.contains(&path) {
                    events.push(Event {
                        path,
                        flags: flag::ROOT_CHANGED,
                        id,
                    });
                }
                continue;
            }
            let is_dir = || std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir());
            let flags = match action {
                FILE_ACTION_ADDED => flag::CREATED | if is_dir() { flag::IS_DIR } else { 0 },
                FILE_ACTION_REMOVED => flag::REMOVED,
                FILE_ACTION_RENAMED_OLD_NAME => flag::RENAMED,
                FILE_ACTION_RENAMED_NEW_NAME => {
                    flag::RENAMED | if is_dir() { flag::IS_DIR } else { 0 }
                }
                // A directory "changes" whenever a file in it does; that file says so itself.
                FILE_ACTION_MODIFIED if is_dir() => continue,
                FILE_ACTION_MODIFIED => flag::MODIFIED,
                _ => continue,
            };
            events.push(Event { path, flags, id });
        }
    }

    fn look_at_followed(&self) {
        let mut followed = self.followed.lock().unwrap_or_else(|p| p.into_inner());
        let mut events = vec![];
        followed.retain(|path, last| {
            let now = stamp(path);
            if now == *last {
                return true;
            }
            *last = now;
            let id = next_id();
            let flags = if now.is_none() {
                flag::REMOVED
            } else {
                flag::MODIFIED
            };
            events.push(Event {
                path: path.clone(),
                flags,
                id,
            });
            now.is_some()
        });
        drop(followed);
        events.retain(|e| self.callback.wants(e));
        deliver(&self.callback, Batch { events });
    }
}
