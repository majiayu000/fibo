use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::{Child, Command};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, GetLastError, WAIT_TIMEOUT};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL, TerminateProcess, WaitForSingleObject,
};

/// Neither a console nor a Ctrl+C to share with whatever started it.
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// In PowerShell, there being no other shell every Windows has that can wait for a process.
pub fn start_after_exit(
    pid: u32,
    program: &std::path::Path,
    args: Vec<std::ffi::OsString>,
) -> io::Result<()> {
    use crate::shell::quote;
    use std::os::windows::process::CommandExt;
    let words: Vec<String> = args.iter().map(|a| quote(&a.to_string_lossy())).collect();
    let script = format!(
        "Wait-Process -Id {pid} -ErrorAction Ignore; & {} {}",
        quote(&program.to_string_lossy()),
        words.join(" ")
    );
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let powershell =
        std::path::Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    Command::new(powershell)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
        .spawn()?;
    Ok(())
}

pub fn exit_now(code: i32) -> ! {
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: ends this process; the handle is the pseudo-handle for it.
    unsafe {
        TerminateProcess(GetCurrentProcess(), code as u32);
    }
    std::process::abort()
}

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

pub fn terminate(pid: i32) -> io::Result<()> {
    kill(pid);
    Ok(())
}

/// No signal to save on: the closed console tells console programs, this is what comes after.
pub fn hang_up_group(group: i32) -> io::Result<()> {
    kill_group(group)
}

/// A process and what it started, children first, since there is no group to name.
pub fn kill_group(group: i32) -> io::Result<()> {
    for child in crate::inspect::children(group as u32)? {
        kill_group(child as i32)?;
    }
    kill(group);
    Ok(())
}

pub fn was_signalled(_status: &std::process::ExitStatus) -> bool {
    false
}

pub fn give_way() {
    // SAFETY: only changes this thread's priority.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
}

pub fn parent() -> u32 {
    let mut found = 0;
    let me = std::process::id();
    let _ = crate::inspect::each_process(|pid, parent| {
        if pid == me {
            found = parent;
        }
    });
    found
}

/// A wait on the parent's handle, so no looking. One already gone when this starts ends this too.
pub fn exit_with_parent() {
    // SAFETY: the handle goes into an owner, and is open for as long as the thread waits.
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, parent()) };
    if raw.is_null() {
        std::process::exit(0);
    }
    let parent = unsafe { OwnedHandle::from_raw_handle(raw) };
    std::thread::spawn(move || {
        unsafe { WaitForSingleObject(parent.as_raw_handle(), INFINITE) };
        std::process::exit(0);
    });
}

/// Windows has no such limit on handles.
pub fn raise_open_file_limit() -> io::Result<Option<u64>> {
    Ok(None)
}
