//! Who is at the other end of a local socket. Only where those are Unix sockets for now: the
//! local connection gets its interface, with Windows' named pipes, in step 3 of
//! docs/platform/index.html.
use std::io;
use std::os::unix::net::UnixStream;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
mod imp;

/// Whether the other end runs as the same account as this process.
pub fn is_this_account(stream: &UnixStream) -> io::Result<bool> {
    let uid = imp::peer_uid(stream)?;
    Ok(uid == unsafe { libc::geteuid() })
}
