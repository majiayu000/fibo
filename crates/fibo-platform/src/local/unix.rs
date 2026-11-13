use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

pub struct Stream(UnixStream);

impl Stream {
    pub fn connect(path: &Path) -> io::Result<Self> {
        UnixStream::connect(path).map(Self)
    }
    pub fn pair() -> io::Result<(Self, Self)> {
        let (a, b) = UnixStream::pair()?;
        Ok((Self(a), Self(b)))
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }
    pub fn set_read_timeout(&self, wait: Option<Duration>) -> io::Result<()> {
        self.0.set_read_timeout(wait)
    }
    pub fn set_write_timeout(&self, wait: Option<Duration>) -> io::Result<()> {
        self.0.set_write_timeout(wait)
    }
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.0.shutdown(how)
    }
    pub fn is_open(&self) -> bool {
        let mut byte = 0u8;
        // SAFETY: a one-byte peek on this socket never consumes input.
        let n = unsafe {
            libc::recv(
                self.0.as_raw_fd(),
                (&raw mut byte).cast(),
                1,
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        n > 0 || (n < 0 && io::Error::last_os_error().kind() == io::ErrorKind::WouldBlock)
    }
    pub fn wait_readable(&self, wait: Duration) -> io::Result<bool> {
        let mut fd = libc::pollfd {
            fd: self.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let deadline = std::time::Instant::now() + wait;
        loop {
            let ms = deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_millis()
                .min(i32::MAX as u128) as i32;
            let result = unsafe { libc::poll(&mut fd, 1, ms) };
            if result >= 0 {
                return Ok(result > 0);
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
    pub fn peer_is_this_account(&self) -> io::Result<bool> {
        Ok(peer_uid(&self.0)? == unsafe { libc::geteuid() })
    }
    #[inline]
    pub fn read(&self, buffer: &mut [u8]) -> io::Result<usize> {
        (&self.0).read(buffer)
    }
    #[inline]
    pub fn write(&self, bytes: &[u8]) -> io::Result<usize> {
        (&self.0).write(bytes)
    }
}

impl AsRawFd for Stream {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

#[cfg(target_os = "macos")]
fn peer_uid(stream: &UnixStream) -> io::Result<libc::uid_t> {
    let (mut uid, mut gid) = (0, 0);
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(uid)
}

#[cfg(not(target_os = "macos"))]
fn peer_uid(stream: &UnixStream) -> io::Result<libc::uid_t> {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `len` says how much `cred` holds.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut cred).cast(),
            &mut len,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(cred.uid)
}

pub struct Listener(UnixListener);

impl Listener {
    pub fn bind(path: &Path) -> io::Result<Self> {
        UnixListener::bind(path).map(Self)
    }
    pub fn accept(&self) -> io::Result<Stream> {
        self.0.accept().map(|(stream, _)| Stream(stream))
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }
}

pub fn at_path(path: &Path) -> io::Result<super::AtPath> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_socket() && m.uid() == unsafe { libc::geteuid() } => {
            Ok(super::AtPath::MySocket)
        }
        Ok(_) => Ok(super::AtPath::Other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(super::AtPath::Nothing),
        Err(e) => Err(e),
    }
}

pub fn restrict(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}
