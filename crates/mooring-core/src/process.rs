//! Ending a command together with what it started, and asking whether a process is there.
//!
//! macOS and Linux use a process group. Windows has none, so there a job object plays that
//! part. Its code has only been compiled, never run (docs/windows/plan.md).
use std::io;
use std::process::{Child, Command};

/// What a command started: everything in it ends with `kill`. Dropping this ends nothing.
pub struct Group(imp::Group);

impl Group {
    /// Starts `cmd` in a group of its own.
    pub fn spawn(cmd: &mut Command) -> io::Result<(Child, Self)> {
        let (child, group) = imp::spawn(cmd)?;
        Ok((child, Self(group)))
    }

    /// Ends everything in the group. One that is already gone is fine.
    pub fn kill(&self) {
        self.0.kill();
    }
}

/// Whether a process with this id is running.
pub fn exists(pid: i32) -> bool {
    imp::exists(pid)
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::process::CommandExt;

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
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, GetLastError, WAIT_TIMEOUT};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    /// A job without kill-on-close: closing it leaves its processes running, as dropping a
    /// process group's id does. A job outlives its first process, so `Leftovers::End` still
    /// reaches what the command left behind.
    pub struct Group {
        job: OwnedHandle,
    }

    /// std cannot start a process inside a job, so it joins just after it starts: a child
    /// it starts in that instant stays outside. That is the known gap with a process group.
    pub fn spawn(cmd: &mut Command) -> io::Result<(Child, Group)> {
        let mut child = cmd.spawn()?;
        match join_new_job(&child) {
            Ok(job) => Ok((child, Group { job })),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(e)
            }
        }
    }

    fn join_new_job(child: &Child) -> io::Result<OwnedHandle> {
        // SAFETY: both pointers may be null; the handle returned is owned by the caller.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `raw` was just created and nothing else owns it.
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        // SAFETY: both handles are open for the length of the call.
        if unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    impl Group {
        /// The callers have no way to go on if this fails, but it is not hidden.
        pub fn kill(&self) {
            // SAFETY: the job handle is open for as long as `self` lives.
            if unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } == 0 {
                eprintln!("结束进程组失败：{}", io::Error::last_os_error());
            }
        }
    }

    /// One that exists but may not be opened (another account's) counts as there: what asks
    /// this would otherwise delete the files of a process still using them.
    pub fn exists(pid: i32) -> bool {
        let Ok(pid) = u32::try_from(pid) else {
            return false;
        };
        // SAFETY: a plain call; a null return is handled, and the handle gets one owner.
        unsafe {
            let raw = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if raw.is_null() {
                return GetLastError() == ERROR_ACCESS_DENIED;
            }
            let process = OwnedHandle::from_raw_handle(raw);
            WaitForSingleObject(process.as_raw_handle(), 0) == WAIT_TIMEOUT
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn exists_tells_a_running_process_from_an_ended_one() {
        assert!(exists(std::process::id() as i32));
        let mut child = Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id() as i32;
        child.wait().unwrap();
        assert!(!exists(pid));
    }

    #[test]
    fn a_group_ending_after_its_leader_is_harmless() {
        let (mut child, group) = Group::spawn(&mut Command::new("/usr/bin/true")).unwrap();
        child.wait().unwrap();
        group.kill();
    }
}
