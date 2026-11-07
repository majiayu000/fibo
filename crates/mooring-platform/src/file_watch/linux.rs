//! inotify. It watches one directory at a time, so a tree is one watch per directory, and a
//! root that does not exist yet is a watch on the nearest ancestor that does. It keeps no
//! journal, so there is nothing to replay: see `MUST_SCAN` in the interface.
use super::{Batch, Callback, Event, deliver, flag};
use std::{
    collections::{HashMap, HashSet},
    ffi::CString,
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::ffi::{OsStrExt, OsStringExt},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

const TREE: u32 = libc::IN_CREATE
    | libc::IN_MODIFY
    | libc::IN_MOVED_FROM
    | libc::IN_MOVED_TO
    | libc::IN_DELETE
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF
    | libc::IN_ONLYDIR
    | libc::IN_DONT_FOLLOW;
const FILE: u32 =
    libc::IN_MODIFY | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_DONT_FOLLOW;

/// Ids count up from the clock, so one from an earlier run is older than any from this one.
static LAST_ID: AtomicU64 = AtomicU64::new(0);
/// What `current_id` gave out and nothing has started from yet. Starting from one loses
/// nothing, once: starting from it again means time passed unwatched.
static NOW_IDS: Mutex<Option<HashSet<u64>>> = Mutex::new(None);

fn next_id() -> u64 {
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
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

struct Watches {
    inotify: OwnedFd,
    /// Watch descriptor -> what it watches.
    paths: HashMap<i32, PathBuf>,
    followed: HashSet<PathBuf>,
}

impl Watches {
    fn add(&mut self, path: &Path, mask: u32) -> io::Result<()> {
        let text = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
        let wd = unsafe { libc::inotify_add_watch(self.inotify.as_raw_fd(), text.as_ptr(), mask) };
        if wd < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ENOSPC) {
                return Err(io::Error::other(format!(
                    "this account's inotify watches ran out at {} folders; raise \
                     fs.inotify.max_user_watches",
                    self.paths.len()
                )));
            }
            return Err(error);
        }
        self.paths.insert(wd, path.to_owned());
        Ok(())
    }

    /// A directory and everything below it. One that went away meanwhile is not an error,
    /// and neither is a link, which `TREE` will not follow: nor does the walk.
    fn add_tree(&mut self, dir: &Path) -> io::Result<()> {
        let mut pending = vec![dir.to_owned()];
        while let Some(dir) = pending.pop() {
            match self.add(&dir, TREE) {
                Ok(()) => (),
                Err(e) if gone(&e) => continue,
                Err(e) => return Err(e),
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    pending.push(entry.path());
                }
            }
        }
        Ok(())
    }

    /// Whichever applies to a directory that just appeared: the whole of it inside a root,
    /// itself on the way to one, nothing elsewhere. A step on the way may already hold the
    /// next one, made before this watch began and so with no event: `found` gets those that
    /// reach a root.
    fn add_new_dir(
        &mut self,
        dir: &Path,
        roots: &[PathBuf],
        found: &mut Vec<PathBuf>,
    ) -> io::Result<()> {
        if roots.iter().any(|root| dir.starts_with(root)) {
            return self.add_tree(dir);
        }
        if !roots.iter().any(|root| root.starts_with(dir)) {
            return Ok(());
        }
        match self.add(dir, TREE) {
            Err(e) if !gone(&e) => return Err(e),
            _ => (),
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                let next = entry.path();
                self.add_new_dir(&next, roots, found)?;
                if roots.iter().any(|root| next.starts_with(root)) {
                    found.push(next);
                }
            }
        }
        Ok(())
    }

    fn forget_below(&mut self, dir: &Path) {
        let fd = self.inotify.as_raw_fd();
        self.paths.retain(|wd, path| {
            let below = path.starts_with(dir);
            if below {
                unsafe { libc::inotify_rm_watch(fd, *wd) };
            }
            !below
        });
    }
}

fn gone(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::ENOENT) | Some(libc::ENOTDIR) | Some(libc::ELOOP)
    )
}

pub struct Watch {
    watches: Arc<Mutex<Watches>>,
    wake: Arc<OwnedFd>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Watch {
    pub fn start(callback: Arc<Callback>, since: u64) -> io::Result<Self> {
        let raw = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let inotify = unsafe { OwnedFd::from_raw_fd(raw) };
        let raw = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let wake = Arc::new(unsafe { OwnedFd::from_raw_fd(raw) });
        let mut watches = Watches {
            inotify,
            paths: HashMap::new(),
            followed: HashSet::new(),
        };
        for root in &callback.roots {
            if root.is_dir() {
                watches.add_tree(root)?;
            } else if let Some(existing) = root.ancestors().find(|p| p.is_dir()) {
                watches.add(existing, TREE)?;
            }
        }
        let watches = Arc::new(Mutex::new(watches));
        // Only now that the watches are in place, so a change cannot fall between this look
        // and the first event.
        let mut ids = NOW_IDS.lock().unwrap_or_else(|p| p.into_inner());
        if !ids.get_or_insert_default().remove(&since) {
            report_scan(&callback, flag::MUST_SCAN);
        }
        drop(ids);
        let (worker, worker_wake) = (watches.clone(), wake.clone());
        let thread = std::thread::spawn(move || run(&worker, &worker_wake, &callback));
        Ok(Self {
            watches,
            wake,
            thread: Some(thread),
        })
    }

    pub fn follow(&self, paths: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
        let mut watches = self
            .watches
            .lock()
            .map_err(|_| io::Error::other("file watch lock poisoned"))?;
        let dropped: Vec<PathBuf> = watches
            .followed
            .iter()
            .filter(|p| !paths.contains(p))
            .cloned()
            .collect();
        for path in dropped {
            watches.followed.remove(&path);
            // A file with a watch of its own; its directory's watch stays if it has one.
            watches.forget_below(&path);
        }
        let mut added = vec![];
        for path in paths {
            if watches.followed.contains(path) {
                continue;
            }
            match std::fs::symlink_metadata(path) {
                Ok(_) => (),
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            }
            // A watched directory already says when a file in it is written to, at once.
            let parent = path.parent().unwrap_or(path);
            if !watches.paths.values().any(|p| p == parent) {
                watches.add(path, FILE)?;
            }
            watches.followed.insert(path.clone());
            added.push(path.clone());
        }
        Ok(added)
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let one = 1u64;
        unsafe { libc::write(self.wake.as_raw_fd(), (&raw const one).cast(), 8) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
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

fn run(watches: &Mutex<Watches>, wake: &OwnedFd, callback: &Callback) {
    let inotify = match watches.lock() {
        Ok(w) => w.inotify.as_raw_fd(),
        Err(_) => return,
    };
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let mut fds = [
            libc::pollfd {
                fd: inotify,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wake.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // Infinite kernel wait: a change or the shutdown event are the only wakeups.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if n < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            eprintln!("file watch failed: {error}");
            report_scan(callback, flag::ROOT_CHANGED);
            return;
        }
        if fds[1].revents != 0 {
            return;
        }
        let n = unsafe { libc::read(inotify, buffer.as_mut_ptr().cast(), buffer.len()) };
        if n < 0 {
            let error = io::Error::last_os_error();
            if matches!(
                error.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            eprintln!("file watch failed: {error}");
            report_scan(callback, flag::ROOT_CHANGED);
            return;
        }
        let events = {
            let mut watches = watches.lock().unwrap_or_else(|p| p.into_inner());
            parse(&buffer[..n as usize], &mut watches, callback)
        };
        deliver(callback, Batch { events });
    }
}

const HEADER: usize = std::mem::size_of::<libc::inotify_event>();

fn parse(mut bytes: &[u8], watches: &mut Watches, callback: &Callback) -> Vec<Event> {
    let mut events = vec![];
    while bytes.len() >= HEADER {
        // SAFETY: at least a header is left, and the read is unaligned.
        let raw: libc::inotify_event = unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast()) };
        let end = (HEADER + raw.len as usize).min(bytes.len());
        let name = &bytes[HEADER..end];
        let name = &name[..name.iter().position(|b| *b == 0).unwrap_or(name.len())];
        bytes = &bytes[end..];
        translate(&raw, name, watches, callback, &mut events);
    }
    events.retain(|event| callback.wants(event));
    events
}

fn translate(
    raw: &libc::inotify_event,
    name: &[u8],
    watches: &mut Watches,
    callback: &Callback,
    events: &mut Vec<Event>,
) {
    let id = next_id();
    if raw.mask & libc::IN_Q_OVERFLOW != 0 {
        // No path to go with it: every root is affected.
        report_scan(callback, flag::KERNEL_DROPPED);
        return;
    }
    if raw.mask & libc::IN_IGNORED != 0 {
        watches.paths.remove(&raw.wd);
        return;
    }
    let Some(watched) = watches.paths.get(&raw.wd).cloned() else {
        return;
    };
    let path = if name.is_empty() {
        watched
    } else {
        watched.join(std::ffi::OsString::from_vec(name.to_vec()))
    };
    let is_dir = if raw.mask & libc::IN_ISDIR != 0 {
        flag::IS_DIR
    } else {
        0
    };
    if raw.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF) != 0 {
        watches.followed.remove(&path);
        let flags = if callback.roots.contains(&path) {
            flag::ROOT_CHANGED
        } else {
            flag::REMOVED
        };
        if raw.mask & libc::IN_MOVE_SELF != 0 {
            watches.forget_below(&path);
        }
        events.push(Event { path, flags, id });
        return;
    }
    let change = if raw.mask & libc::IN_CREATE != 0 {
        flag::CREATED
    } else if raw.mask & libc::IN_MODIFY != 0 {
        flag::MODIFIED
    } else if raw.mask & libc::IN_DELETE != 0 {
        flag::REMOVED
    } else {
        flag::RENAMED
    };
    if raw.mask & (libc::IN_DELETE | libc::IN_MOVED_FROM) != 0 {
        watches.followed.remove(&path);
        if is_dir != 0 {
            watches.forget_below(&path);
        }
    }
    if is_dir != 0 && raw.mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0 {
        let mut found = vec![];
        let added = watches.add_new_dir(&path, &callback.roots, &mut found);
        events.extend(found.into_iter().map(|path| Event {
            path,
            flags: flag::CREATED | flag::IS_DIR,
            id,
        }));
        if let Err(error) = added {
            eprintln!("file watch of {} failed: {error}", path.display());
            // Unwatched from here down: only a look through it can make up for that.
            events.push(Event {
                path,
                flags: flag::MUST_SCAN | is_dir,
                id,
            });
            return;
        }
    }
    events.push(Event {
        path,
        flags: change | is_dir,
        id,
    });
}
