//! A program running in a terminal of its own, and this end of that terminal.
//!
//! A pseudo-terminal on macOS and Linux, a pseudo console on Windows. What the program prints
//! comes out as terminal output either way, but Windows renders it first: what arrives is
//! the screen's state, so a byte the program wrote may not arrive as written, and it drops
//! the mouse modes (`?1000h`, `?1006h`). See docs/platform/index.html.
use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
mod hand_over;
#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

/// What to start.
pub struct Spec<'a> {
    pub argv: &'a [String],
    pub cwd: &'a Path,
    /// The whole environment the program gets.
    pub env: Vec<(OsString, OsString)>,
    pub cols: u16,
    pub rows: u16,
}

/// Starts a program in a new terminal.
pub fn spawn(spec: &Spec) -> io::Result<(Child, Master)> {
    let (child, master) = imp::spawn(spec)?;
    Ok((Child(child), Master(master)))
}

/// The program started in a terminal.
pub struct Child(imp::Child);

impl Child {
    pub fn id(&self) -> u32 {
        self.0.id()
    }
    pub fn kill(&mut self) -> io::Result<()> {
        self.0.kill()
    }
    /// Waits for it to end. Its exit code, if it left one rather than being stopped by a
    /// signal.
    pub fn wait(&mut self) -> io::Result<Option<i32>> {
        self.0.wait()
    }
}

/// This end of the terminal. Dropping every copy hangs the terminal up.
pub struct Master(imp::Master);

impl Master {
    /// Types `bytes`. A failure says how many were accepted, and nothing is sent twice.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write(bytes)
    }
    /// `cell` is one character's size in pixels, for programs that ask.
    pub fn resize(&self, cols: u16, rows: u16, cell: (u32, u32)) -> io::Result<()> {
        self.0.resize(cols, rows, cell)
    }
    /// Bytes typed that the program has not read yet. None when that cannot be told, which
    /// outside macOS is always for now.
    pub fn backlog(&self) -> Option<usize> {
        self.0.backlog()
    }
    /// The process group the terminal's foreground job runs in. None on Windows, which has
    /// no such thing.
    pub fn foreground_group(&self) -> Option<i32> {
        self.0.foreground_group()
    }
    /// What the program prints comes from here. Only ask once.
    pub fn reader(&self) -> io::Result<Reader> {
        self.0.reader().map(Reader)
    }
}

/// What came of waiting for output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
    /// This many bytes are in the buffer.
    Data(usize),
    /// Nothing for as long as was asked.
    Quiet,
    /// The terminal is gone, and everything it printed has been read.
    Closed,
}

pub struct Reader(imp::Reader);

impl Reader {
    /// Waits for output, for `wait` or until there is some. Sleeps in the kernel.
    pub fn next(&mut self, buffer: &mut [u8], wait: Option<Duration>) -> io::Result<Read> {
        self.0.next(buffer, wait)
    }
}

/// The size of the terminal this process runs in, as columns and rows.
pub fn own_size() -> Option<(u16, u16)> {
    imp::own_size()
}

/// Whether a running terminal can pass to another process, so that its programs never notice.
/// Windows has nothing to pass a pseudo console with.
pub const CAN_HAND_OVER: bool = cfg!(unix);

/// Sends these terminals over a connection. Refused where `CAN_HAND_OVER` is false.
pub fn send_masters(stream: &crate::local::Stream, masters: &[&Master]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fds: Vec<_> = masters.iter().map(|m| m.0.as_raw_fd()).collect();
        hand_over::send_fds(stream, &fds)
    }
    #[cfg(not(unix))]
    {
        let _ = (stream, masters);
        Err(refused())
    }
}

/// Receives `count` terminals sent with `send_masters`.
pub fn receive_masters(stream: &crate::local::Stream, count: usize) -> io::Result<Vec<Master>> {
    #[cfg(unix)]
    {
        let fds = hand_over::recv_fds(stream, count)?;
        Ok(fds
            .into_iter()
            .map(|fd| Master(imp::Master::from(fd)))
            .collect())
    }
    #[cfg(not(unix))]
    {
        let _ = (stream, count);
        Err(refused())
    }
}

#[cfg(not(unix))]
fn refused() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "a terminal cannot pass to another process here",
    )
}
