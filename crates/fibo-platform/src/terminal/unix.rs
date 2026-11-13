use super::{Read, Spec};
use std::fs::File;
use std::io::{self, Read as _, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn spawn(spec: &Spec) -> io::Result<(Child, Master)> {
    let mut master = -1;
    let mut slave = -1;
    let mut size = libc::winsize {
        ws_row: spec.rows,
        ws_col: spec.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: initialized out pointers; descriptors immediately become owned.
    if unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut size,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    for fd in [&master, &slave] {
        if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut command = Command::new(&spec.argv[0]);
    command
        .args(&spec.argv[1..])
        .current_dir(spec.cwd)
        .env_clear()
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    // SAFETY: only async-signal-safe operations after fork.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as libc::c_ulong, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut mask: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut mask);
            if libc::sigprocmask(libc::SIG_SETMASK, &mask, std::ptr::null_mut()) < 0 {
                return Err(io::Error::last_os_error());
            }
            // SIGHUP too: ignored here (a host started under nohup) it stays ignored across
            // exec and a shell cannot trap it back, so the terminal closing would end nothing.
            for signal in [
                libc::SIGHUP,
                libc::SIGINT,
                libc::SIGQUIT,
                libc::SIGTSTP,
                libc::SIGTTIN,
                libc::SIGTTOU,
                libc::SIGWINCH,
                libc::SIGCHLD,
            ] {
                if libc::signal(signal, libc::SIG_DFL) == libc::SIG_ERR {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let child = command.spawn()?;
    Ok((Child(child), Master::from(master)))
}

pub struct Child(std::process::Child);

impl Child {
    pub fn id(&self) -> u32 {
        self.0.id()
    }
    pub fn kill(&mut self) -> io::Result<()> {
        self.0.kill()
    }
    pub fn wait(&mut self) -> io::Result<Option<i32>> {
        Ok(self.0.wait()?.code())
    }
}

pub struct Master {
    file: File,
}

impl From<OwnedFd> for Master {
    fn from(fd: OwnedFd) -> Self {
        Self {
            file: File::from(fd),
        }
    }
}

impl AsRawFd for Master {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

impl Master {
    pub fn resize(&self, cols: u16, rows: u16, cell: (u32, u32)) -> io::Result<()> {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: (u32::from(cols) * cell.0).min(u32::from(u16::MAX)) as u16,
            ws_ypixel: (u32::from(rows) * cell.1).min(u32::from(u16::MAX)) as u16,
        };
        if unsafe { libc::ioctl(self.file.as_raw_fd(), libc::TIOCSWINSZ, &size) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn backlog(&self) -> Option<usize> {
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
            let mut name = [0 as libc::c_char; 128];
            // SAFETY: TIOCPTYGNAME writes a NUL-terminated path of at most 128 bytes.
            if unsafe {
                libc::ioctl(
                    self.file.as_raw_fd(),
                    libc::TIOCPTYGNAME as _,
                    name.as_mut_ptr(),
                )
            } < 0
            {
                return None;
            }
            // SAFETY: the buffer is NUL-terminated by the ioctl above.
            let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) };
            let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path.to_bytes()));
            let slave = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
                .open(path)
                .ok()?;
            let mut pending: libc::c_int = 0;
            // SAFETY: FIONREAD stores the input queue length into `pending`.
            if unsafe { libc::ioctl(slave.as_raw_fd(), libc::FIONREAD, &mut pending) } < 0 {
                return None;
            }
            usize::try_from(pending).ok()
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    pub fn foreground_group(&self) -> Option<i32> {
        let group = unsafe { libc::tcgetpgrp(self.file.as_raw_fd()) };
        (group > 0).then_some(group)
    }

    pub fn reader(&self) -> io::Result<Reader> {
        Ok(Reader(self.file.try_clone()?))
    }

    /// Failed writes report the accepted prefix. Never replay a failed input.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut accepted = 0;
        while accepted < bytes.len() {
            let mut poll = libc::pollfd {
                fd: self.file.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut poll, 1, 5_000) };
            if ready < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if ready <= 0 {
                return Err(io::Error::other(format!(
                    "输入中止，已接受 {accepted} 字节；禁止自动重发"
                )));
            }
            match self
                .file
                .write(&bytes[accepted..bytes.len().min(accepted + 256)])
            {
                Ok(0) => {
                    return Err(io::Error::other(format!(
                        "PTY 已关闭，已接受 {accepted} 字节"
                    )));
                }
                Ok(n) => accepted += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) => {}
                Err(e) => return Err(io::Error::other(format!("{e}; 已接受 {accepted} 字节"))),
            }
        }
        Ok(())
    }
}

pub struct Reader(File);

impl Reader {
    pub fn next(&mut self, buffer: &mut [u8], wait: Option<Duration>) -> io::Result<Read> {
        let until = wait.map(|w| Instant::now() + w);
        loop {
            let mut poll = libc::pollfd {
                fd: self.0.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let timeout = until.map_or(-1, |at| {
                i32::try_from(at.saturating_duration_since(Instant::now()).as_millis())
                    .unwrap_or(i32::MAX)
            });
            let ready = unsafe { libc::poll(&mut poll, 1, timeout) };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if ready == 0 {
                return Ok(Read::Quiet);
            }
            match self.0.read(buffer) {
                Ok(0) => return Ok(Read::Closed),
                Ok(n) => return Ok(Read::Data(n)),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) => {}
                Err(e) if e.raw_os_error() == Some(libc::EIO) => return Ok(Read::Closed),
                Err(e) => return Err(e),
            }
        }
    }
}

pub fn own_size() -> Option<(u16, u16)> {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: TIOCGWINSZ fills the struct it is given.
    let read = unsafe { libc::ioctl(0, libc::TIOCGWINSZ, &mut size) };
    (read == 0).then_some((size.ws_col, size.ws_row))
}
