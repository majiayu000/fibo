use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

pub fn peer_uid(stream: &UnixStream) -> io::Result<libc::uid_t> {
    let (mut uid, mut gid) = (0, 0);
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(uid)
}
