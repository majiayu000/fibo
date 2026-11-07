use std::collections::HashMap;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex};

/// One blocking epoll over a pidfd per watched process: a pidfd turns readable when its
/// process ends.
pub struct ExitWatch {
    epoll: Arc<OwnedFd>,
    pidfds: Arc<Mutex<HashMap<u32, OwnedFd>>>,
}

impl ExitWatch {
    pub fn new(on_exit: impl Fn(u32) + Send + 'static) -> io::Result<Self> {
        let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let epoll = Arc::new(unsafe { OwnedFd::from_raw_fd(fd) });
        let pidfds: Arc<Mutex<HashMap<u32, OwnedFd>>> = Arc::default();
        let (thread_epoll, thread_pidfds) = (epoll.clone(), pidfds.clone());
        std::thread::spawn(move || {
            loop {
                let mut event: libc::epoll_event = unsafe { std::mem::zeroed() };
                let result =
                    unsafe { libc::epoll_wait(thread_epoll.as_raw_fd(), &mut event, 1, -1) };
                if result < 0 {
                    let e = io::Error::last_os_error();
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    eprintln!("Process exit subscription failed: {e}");
                    return;
                }
                let pid = event.u64 as u32;
                // Closing it also takes it out of the epoll, which makes this once only.
                if let Ok(mut pidfds) = thread_pidfds.lock() {
                    pidfds.remove(&pid);
                }
                on_exit(pid);
            }
        });
        Ok(Self { epoll, pidfds })
    }

    pub fn watch(&self, pid: u32) -> io::Result<bool> {
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
        if fd < 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::ESRCH) {
                return Ok(false);
            }
            return Err(e);
        }
        let pidfd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        let raw = pidfd.as_raw_fd();
        let mut pidfds = self
            .pidfds
            .lock()
            .map_err(|_| io::Error::other("poisoned"))?;
        // Held across the add, so the thread cannot look before it is there.
        if pidfds.contains_key(&pid) {
            return Ok(true);
        }
        pidfds.insert(pid, pidfd);
        let mut event = libc::epoll_event {
            events: (libc::EPOLLIN | libc::EPOLLONESHOT) as u32,
            u64: u64::from(pid),
        };
        let added = unsafe {
            libc::epoll_ctl(self.epoll.as_raw_fd(), libc::EPOLL_CTL_ADD, raw, &mut event)
        };
        if added < 0 {
            pidfds.remove(&pid);
            return Err(io::Error::last_os_error());
        }
        Ok(true)
    }
}
