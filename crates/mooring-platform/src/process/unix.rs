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

pub fn terminate(pid: i32) -> io::Result<()> {
    // SAFETY: a signal to one process; the caller has checked the id is one.
    if unsafe { libc::kill(pid, libc::SIGTERM) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn signal_group(group: i32, signal: i32) -> io::Result<()> {
    // SAFETY: a signal to a group the caller named.
    if unsafe { libc::kill(-group, signal) } < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}

pub fn hang_up_group(group: i32) -> io::Result<()> {
    signal_group(group, libc::SIGHUP)
}

pub fn kill_group(group: i32) -> io::Result<()> {
    signal_group(group, libc::SIGKILL)
}

pub fn was_signalled(status: &std::process::ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;
    status.signal().is_some()
}

pub fn give_way() {
    #[cfg(target_os = "macos")]
    // SAFETY: only changes this thread's scheduling class.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
    #[cfg(not(target_os = "macos"))]
    // SAFETY: on Linux this names the calling thread; only lowers its priority.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}

pub fn parent() -> u32 {
    // SAFETY: getppid(2) takes no arguments and cannot fail.
    unsafe { libc::getppid() as u32 }
}

/// An orphan is handed to another parent, and nothing tells it: only looking does.
pub fn exit_with_parent() {
    let first = parent();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if parent() != first {
                std::process::exit(0);
            }
        }
    });
}

pub fn raise_open_file_limit() -> io::Result<Option<u64>> {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: getrlimit/setrlimit only read and write the struct passed by pointer.
    unsafe {
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
            return Err(io::Error::last_os_error());
        }
        // macOS rejects RLIM_INFINITY for the soft limit; use a finite cap below it.
        let wanted = limit.rlim_max.min(65_536);
        if wanted <= limit.rlim_cur {
            return Ok(None);
        }
        limit.rlim_cur = wanted;
        if libc::setrlimit(libc::RLIMIT_NOFILE, &limit) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(wanted))
    }
}
