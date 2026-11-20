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

pub fn group_members(group: i32) -> Vec<u32> {
    crate::inspect::all_pids()
        .into_iter()
        .filter(|&pid| {
            // SAFETY: asks for one process's group; a process that just exited returns -1.
            unsafe { libc::getpgid(pid as i32) == group }
        })
        .collect()
}

pub fn was_signalled(status: &std::process::ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;
    status.signal().is_some()
}

pub fn detach(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: setsid and signal are async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            default_term()
        });
    }
}

pub fn takes_term(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: signal is async-signal-safe.
    unsafe { command.pre_exec(default_term) };
}

/// TERM as a program starts with: a window's process ignores it (AppKit quits the app its
/// own way), and its worker threads block every signal; a child keeps both across exec, and
/// std's spawn does not clear the mask here.
fn default_term() -> io::Result<()> {
    // SAFETY: resets one signal's disposition and the signal mask in the child before exec.
    unsafe {
        if libc::signal(libc::SIGTERM, libc::SIG_DFL) == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
        let mut mask: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        if libc::sigprocmask(libc::SIG_SETMASK, &mask, std::ptr::null_mut()) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub fn start_after_exit(
    pid: u32,
    program: &std::path::Path,
    args: Vec<std::ffi::OsString>,
) -> io::Result<()> {
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"while kill -0 "$1" 2>/dev/null; do sleep 0.1; done; shift; exec "$@""#)
        .arg("sh")
        .arg(pid.to_string())
        .arg(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

pub fn become_command(command: &mut Command) -> io::Error {
    command.exec()
}

pub fn exit_now(code: i32) -> ! {
    // SAFETY: ends the process; nothing runs after it.
    unsafe { libc::_exit(code) }
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
