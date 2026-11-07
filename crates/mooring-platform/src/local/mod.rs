//! The connection between programs on one machine: the host and its clients.
//!
//! A Unix socket at a path, on Windows too, which has had them since 1803 (ConPTY needs 1809
//! anyway). A socket means the same reads, timeouts and shutdown everywhere, where a named pipe
//! would have to imitate each one. The path's folder is what keeps others out; `peer_is_this_account`
//! checks again.
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::path::Path;
use std::time::Duration;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

/// One end of a connection. Copies from `try_clone` are the same connection: they share its
/// timeouts, and one may read while another writes.
pub struct Stream(imp::Stream);

impl Stream {
    pub fn connect(path: impl AsRef<Path>) -> io::Result<Self> {
        imp::Stream::connect(path.as_ref()).map(Self)
    }
    /// Two ends joined to each other, in this process.
    pub fn pair() -> io::Result<(Self, Self)> {
        let (a, b) = imp::Stream::pair()?;
        Ok((Self(a), Self(b)))
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }
    /// A read that waits this long fails with `WouldBlock`. None waits for ever.
    pub fn set_read_timeout(&self, wait: Option<Duration>) -> io::Result<()> {
        self.0.set_read_timeout(wait)
    }
    pub fn set_write_timeout(&self, wait: Option<Duration>) -> io::Result<()> {
        self.0.set_write_timeout(wait)
    }
    /// Ends the connection for every copy: a read waiting in another thread returns.
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.0.shutdown(how)
    }
    /// Whether the other end is still there. Reads nothing, waits for nothing.
    pub fn is_open(&self) -> bool {
        self.0.is_open()
    }
    /// Waits until there is something to read or the other end has gone, for up to `wait`.
    /// False when the time ran out. Sleeps in the kernel.
    pub fn wait_readable(&self, wait: Duration) -> io::Result<bool> {
        self.0.wait_readable(wait)
    }
    /// Whether the other end runs as the same account as this process.
    pub fn peer_is_this_account(&self) -> io::Result<bool> {
        self.0.peer_is_this_account()
    }
}

impl Read for Stream {
    #[inline]
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}
impl Write for Stream {
    #[inline]
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Read for &Stream {
    #[inline]
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}
impl Write for &Stream {
    #[inline]
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The handoff passes descriptors over a connection, which only Unix does.
#[cfg(unix)]
impl std::os::fd::AsRawFd for Stream {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.0.as_raw_fd()
    }
}

/// What is at a path a socket goes: nothing, this account's socket, or something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtPath {
    Nothing,
    MySocket,
    Other,
}

/// Looks without following a link.
pub fn at_path(path: &Path) -> io::Result<AtPath> {
    imp::at_path(path)
}

/// Keeps others off a socket at `path`, as far as a socket itself can: on Windows that is left
/// to its folder (`fs::secure_dir`).
pub fn restrict(path: &Path) -> io::Result<()> {
    imp::restrict(path)
}

/// Where connections arrive.
pub struct Listener(imp::Listener);

impl Listener {
    /// Fails when something is at `path` already.
    pub fn bind(path: impl AsRef<Path>) -> io::Result<Self> {
        imp::Listener::bind(path.as_ref()).map(Self)
    }
    pub fn accept(&self) -> io::Result<Stream> {
        self.0.accept().map(Stream)
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }
}
