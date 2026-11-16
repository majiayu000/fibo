//! The audit session (`getaudit_addr`), the nice value, and the responsible process.
use super::Launch;
use std::path::PathBuf;

/// `<bsm/audit.h>`: the session reaches the window server.
const HAS_GRAPHIC_ACCESS: u64 = 0x0010;
/// `<bsm/audit.h>`: the session was created for a login over the network.
const IS_REMOTE: u64 = 0x1000;

#[repr(C)]
#[derive(Default)]
struct AuditInfo {
    auid: u32,
    mask: [u32; 2],
    port: i32,
    kind: u32,
    address: [u32; 4],
    asid: i32,
    flags: u64,
}

unsafe extern "C" {
    fn getaudit_addr(info: *mut AuditInfo, length: u32) -> libc::c_int;
    /// libSystem; what TCC and `launchctl procinfo` report as "responsible pid".
    fn responsibility_get_pid_responsible_for_pid(pid: libc::pid_t) -> libc::pid_t;
}

pub fn current() -> Launch {
    let mut info = AuditInfo::default();
    // SAFETY: the buffer is the struct the call fills, with its exact size.
    let flags = (unsafe { getaudit_addr(&mut info, size_of::<AuditInfo>() as u32) } == 0)
        .then_some(info.flags);
    Launch {
        remote_session: flags.map(|f| f & IS_REMOTE != 0),
        graphic_session: flags.map(|f| f & HAS_GRAPHIC_ACCESS != 0),
        nice: nice(),
        responsible: responsible(),
    }
}

fn nice() -> i32 {
    // SAFETY: reads this process's priority; -1 is also a valid answer, so it cannot fail
    // in a way that matters here.
    unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) }
}

fn responsible() -> Option<PathBuf> {
    // SAFETY: takes a pid, returns a pid or -1.
    let pid = unsafe { responsibility_get_pid_responsible_for_pid(libc::getpid()) };
    if pid <= 0 {
        return None;
    }
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: the buffer is as long as the length passed.
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    buf.truncate(n as usize);
    Some(PathBuf::from(String::from_utf8_lossy(&buf).into_owned()))
}
