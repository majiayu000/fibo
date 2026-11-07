use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

pub struct Group {
    leader: i32,
}

pub fn spawn(cmd: &mut Command) -> io::Result<(Child, Group)> {
    let child = cmd.process_group(0).spawn()?;
    let leader = child.id() as i32;
    Ok((child, Group { leader }))
}

impl Group {
    /// SIGKILL to the group. It is gone once all its members are, and a stale id is
    /// harmless (ESRCH).
    pub fn kill(&self) {
        // SAFETY: a signal to a process group this program created.
        unsafe { libc::kill(-self.leader, libc::SIGKILL) };
    }
}

pub fn exists(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

pub fn kill(pid: i32) {
    // SAFETY: a signal to one process; the caller has checked the id is one.
    unsafe { libc::kill(pid, libc::SIGKILL) };
}
