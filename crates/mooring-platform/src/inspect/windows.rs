use std::io;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

fn later() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "reading another process's arguments is not there yet on Windows",
    )
}

pub fn argv(_pid: u32) -> io::Result<Vec<String>> {
    Err(later())
}

pub fn env_var(_pid: u32, _name: &str) -> Option<String> {
    None
}

pub fn cwd(_pid: u32) -> Option<String> {
    None
}

pub fn children(pid: u32) -> io::Result<Vec<u32>> {
    // SAFETY: a snapshot of the system's processes, closed below on every path.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut out = vec![];
    // SAFETY: `entry` is the size it says it is, and the snapshot is open.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        if entry.th32ParentProcessID == pid && entry.th32ProcessID != pid {
            out.push(entry.th32ProcessID);
        }
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    Ok(out)
}

pub fn started(pid: u32) -> Option<u64> {
    // SAFETY: the handle is closed below, and each FILETIME is written by the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [created, exited, kernel, user] = &mut times;
        let ok = GetProcessTimes(process, created, exited, kernel, user) != 0;
        CloseHandle(process);
        ok.then(|| since_epoch(times[0]))
    }
}

/// A FILETIME counts 100 ns from 1601.
fn since_epoch(at: FILETIME) -> u64 {
    const SECONDS_TO_1970: u64 = 11_644_473_600;
    let ticks = (u64::from(at.dwHighDateTime) << 32) | u64::from(at.dwLowDateTime);
    (ticks / 10_000_000).saturating_sub(SECONDS_TO_1970)
}
