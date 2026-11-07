use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::{Child, Command};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, GetLastError, WAIT_TIMEOUT};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
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

/// One that is gone or may not be opened is left alone, as a signal that fails would be.
pub fn kill(pid: i32) {
    let Ok(pid) = u32::try_from(pid) else {
        return;
    };
    // SAFETY: a plain call; a null return is handled, and the handle gets one owner.
    unsafe {
        let raw = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !raw.is_null() {
            let process = OwnedHandle::from_raw_handle(raw);
            TerminateProcess(process.as_raw_handle(), 1);
        }
    }
}
