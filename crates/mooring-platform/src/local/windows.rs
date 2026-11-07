use std::io;
use std::net::Shutdown;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Once};
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Networking::WinSock::{
    AF_UNIX, FD_SET, FIONBIO, INVALID_SOCKET, MSG_PEEK, SD_BOTH, SD_RECEIVE, SD_SEND, SO_RCVTIMEO,
    SO_SNDTIMEO, SOCK_STREAM, SOCKADDR, SOCKADDR_UN, SOCKET, SOCKET_ERROR, SOL_SOCKET, TIMEVAL,
    WSA_FLAG_NO_HANDLE_INHERIT, WSA_FLAG_OVERLAPPED, WSADATA, WSAETIMEDOUT, WSAEWOULDBLOCK,
    WSAGetLastError, WSAIoctl, WSASocketW, WSAStartup, accept, bind, closesocket, connect,
    ioctlsocket, listen, recv, select, send, setsockopt, shutdown,
};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::IO::CancelIoEx;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// `SIO_AF_UNIX_GETPEERPID`, which windows-sys does not name.
const GET_PEER_PID: u32 = 0x5800_0100;

fn last_error() -> io::Error {
    match unsafe { WSAGetLastError() } {
        // What a Unix socket says when its timeout runs out.
        WSAETIMEDOUT | WSAEWOULDBLOCK => io::ErrorKind::WouldBlock.into(),
        code => io::Error::from_raw_os_error(code),
    }
}

fn started() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let mut data: WSADATA = unsafe { std::mem::zeroed() };
        // SAFETY: filled by the call. A failure shows at the first socket.
        unsafe { WSAStartup(0x0202, &mut data) };
    });
}

/// Closes with its last copy, so that clones need no second socket.
struct Socket {
    raw: SOCKET,
    /// Reading has ended here: Winsock's shutdown wakes no waiting read, so `shutdown` cancels
    /// it, and this says what that cancelling means.
    read_ended: AtomicBool,
}

impl Socket {
    fn new(raw: SOCKET) -> Self {
        Self {
            raw,
            read_ended: AtomicBool::new(false),
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        // SAFETY: the last owner.
        unsafe { closesocket(self.raw) };
    }
}

fn socket() -> io::Result<Socket> {
    started();
    // SAFETY: a new socket, closed by its owner.
    let raw = unsafe {
        WSASocketW(
            AF_UNIX as i32,
            SOCK_STREAM,
            0,
            std::ptr::null(),
            0,
            // Not for overlapped use as such: without this flag the socket is synchronous, and
            // then a read blocks writes on it, timeouts do nothing and shutdown wakes no read.
            WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT,
        )
    };
    if raw == INVALID_SOCKET {
        return Err(last_error());
    }
    Ok(Socket::new(raw))
}

fn address(path: &Path) -> io::Result<SOCKADDR_UN> {
    let mut sun: SOCKADDR_UN = unsafe { std::mem::zeroed() };
    sun.sun_family = AF_UNIX;
    let text = path.to_str().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "socket path is not valid text")
    })?;
    let bytes = text.as_bytes();
    if bytes.len() >= sun.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("socket path is over {} bytes", sun.sun_path.len() - 1),
        ));
    }
    for (to, from) in sun.sun_path.iter_mut().zip(bytes) {
        *to = *from as _;
    }
    Ok(sun)
}

const ADDRESS_SIZE: i32 = std::mem::size_of::<SOCKADDR_UN>() as i32;

#[derive(Clone)]
pub struct Stream(Arc<Socket>);

impl Stream {
    pub fn connect(path: &Path) -> io::Result<Self> {
        let sun = address(path)?;
        let raw = socket()?;
        // SAFETY: the address is the size it says.
        if unsafe { connect(raw.raw, (&raw const sun).cast::<SOCKADDR>(), ADDRESS_SIZE) }
            == SOCKET_ERROR
        {
            return Err(last_error());
        }
        Ok(Self(Arc::new(raw)))
    }

    /// Windows has no socketpair: through a moment's socket in the temporary folder.
    pub fn pair() -> io::Result<(Self, Self)> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ml-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let listener = Listener::bind(&path)?;
        let joined = Self::connect(&path).and_then(|a| Ok((a, listener.accept()?)));
        let _ = std::fs::remove_file(&path);
        joined
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(self.clone())
    }

    fn set_timeout(&self, option: i32, wait: Option<Duration>) -> io::Result<()> {
        // Milliseconds, and zero means for ever: the shortest wait must not become that.
        let ms: u32 = wait.map_or(0, |w| {
            u32::try_from(w.as_millis()).unwrap_or(u32::MAX).max(1)
        });
        // SAFETY: the value is the size it says.
        let done = unsafe {
            setsockopt(
                self.0.raw,
                SOL_SOCKET,
                option,
                (&raw const ms).cast(),
                std::mem::size_of::<u32>() as i32,
            )
        };
        if done == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(())
    }
    pub fn set_read_timeout(&self, wait: Option<Duration>) -> io::Result<()> {
        self.set_timeout(SO_RCVTIMEO, wait)
    }
    pub fn set_write_timeout(&self, wait: Option<Duration>) -> io::Result<()> {
        self.set_timeout(SO_SNDTIMEO, wait)
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Read => SD_RECEIVE,
            Shutdown::Write => SD_SEND,
            Shutdown::Both => SD_BOTH,
        };
        let ends_reading = how != SD_SEND;
        if ends_reading {
            self.0.read_ended.store(true, Ordering::SeqCst);
        }
        // SAFETY: this socket is open while `self` is, and a socket is a handle to cancel on.
        unsafe {
            let shut = shutdown(self.0.raw, how);
            if ends_reading {
                CancelIoEx(self.0.raw as HANDLE, std::ptr::null());
            }
            if shut == SOCKET_ERROR {
                return Err(last_error());
            }
        }
        Ok(())
    }

    fn select(&self, wait: Duration) -> io::Result<bool> {
        let mut set: FD_SET = unsafe { std::mem::zeroed() };
        set.fd_count = 1;
        set.fd_array[0] = self.0.raw;
        let time = TIMEVAL {
            tv_sec: wait.as_secs().min(i32::MAX as u64) as i32,
            tv_usec: wait.subsec_micros() as i32,
        };
        // SAFETY: the set and the time outlive the call.
        let ready = unsafe {
            select(
                0,
                &mut set,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &time,
            )
        };
        if ready == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(ready > 0)
    }

    pub fn is_open(&self) -> bool {
        match self.select(Duration::ZERO) {
            Ok(false) => true,
            Err(_) => false,
            Ok(true) => {
                let mut byte = 0u8;
                // SAFETY: readable, so this does not wait, and a peek consumes nothing.
                unsafe { recv(self.0.raw, &raw mut byte, 1, MSG_PEEK) > 0 }
            }
        }
    }

    pub fn wait_readable(&self, wait: Duration) -> io::Result<bool> {
        self.select(wait)
    }

    pub fn peer_is_this_account(&self) -> io::Result<bool> {
        let mut pid = 0u32;
        let mut written = 0u32;
        // SAFETY: the output is the size it says.
        let asked = unsafe {
            WSAIoctl(
                self.0.raw,
                GET_PEER_PID,
                std::ptr::null(),
                0,
                (&raw mut pid).cast(),
                4,
                &mut written,
                std::ptr::null_mut(),
                None,
            )
        };
        if asked == SOCKET_ERROR {
            return Err(last_error());
        }
        // SAFETY: the handle is closed below on every path.
        let peer = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if peer.is_null() {
            return Err(io::Error::last_os_error());
        }
        let theirs = account(peer);
        unsafe { CloseHandle(peer) };
        let ours = account(unsafe { GetCurrentProcess() })?;
        let theirs = theirs?;
        // SAFETY: both point into buffers kept alive here, at a TOKEN_USER each.
        let same = unsafe {
            let sid = |b: &[u8]| (*b.as_ptr().cast::<TOKEN_USER>()).User.Sid;
            EqualSid(sid(&ours), sid(&theirs))
        };
        Ok(same != 0)
    }

    pub fn read(&self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.0.read_ended.load(Ordering::SeqCst) {
            return Ok(0);
        }
        let size = buffer.len().min(i32::MAX as usize) as i32;
        // SAFETY: `size` fits the buffer.
        let n = unsafe { recv(self.0.raw, buffer.as_mut_ptr(), size, 0) };
        if n == SOCKET_ERROR {
            let error = last_error();
            // Cancelled by `shutdown`, or in its way: the end of the connection, as on Unix.
            if self.0.read_ended.load(Ordering::SeqCst) {
                return Ok(0);
            }
            return Err(error);
        }
        Ok(n as usize)
    }

    pub fn write(&self, bytes: &[u8]) -> io::Result<usize> {
        let size = bytes.len().min(i32::MAX as usize) as i32;
        // SAFETY: `size` fits the bytes.
        let n = unsafe { send(self.0.raw, bytes.as_ptr(), size, 0) };
        if n == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(n as usize)
    }
}

/// A process's TOKEN_USER.
fn account(process: HANDLE) -> io::Result<Vec<u8>> {
    let mut token: HANDLE = INVALID_HANDLE_VALUE;
    // SAFETY: the token is closed below; the second call fills what the first one sized.
    unsafe {
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut size = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0u8; size as usize];
        let read = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        );
        CloseHandle(token);
        if read == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(buffer)
    }
}

#[derive(Clone)]
pub struct Listener(Arc<Socket>);

impl Listener {
    pub fn bind(path: &Path) -> io::Result<Self> {
        let sun = address(path)?;
        let raw = socket()?;
        // SAFETY: the address is the size it says.
        unsafe {
            if bind(raw.raw, (&raw const sun).cast::<SOCKADDR>(), ADDRESS_SIZE) == SOCKET_ERROR
                || listen(raw.raw, 128) == SOCKET_ERROR
            {
                return Err(last_error());
            }
        }
        Ok(Self(Arc::new(raw)))
    }
    pub fn accept(&self) -> io::Result<Stream> {
        // SAFETY: nobody wants the peer's address.
        let raw = unsafe { accept(self.0.raw, std::ptr::null_mut(), std::ptr::null_mut()) };
        if raw == INVALID_SOCKET {
            return Err(last_error());
        }
        Ok(Stream(Arc::new(Socket::new(raw))))
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(self.clone())
    }
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        let mut value = u32::from(nonblocking);
        // SAFETY: the value is what FIONBIO reads.
        if unsafe { ioctlsocket(self.0.raw, FIONBIO, &mut value) } == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(())
    }
}
