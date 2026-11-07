use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::Threading::{
    GetProcessIoCounters, GetProcessTimes, IO_COUNTERS, OpenProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

fn later() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "reading this of another process is not there yet on Windows",
    )
}

pub fn tcp_sockets(_pids: &[u32]) -> io::Result<super::TcpSockets> {
    Err(later())
}

pub fn open_files(_pids: &[u32]) -> io::Result<std::collections::HashMap<u32, Vec<String>>> {
    Err(later())
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

/// Goes through a snapshot of the system's processes.
fn each_process(mut visit: impl FnMut(&PROCESSENTRY32W)) -> io::Result<()> {
    // SAFETY: a snapshot of the system's processes, closed below on every path.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `entry` is the size it says it is, and the snapshot is open.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        visit(&entry);
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    Ok(())
}

pub fn children(pid: u32) -> io::Result<Vec<u32>> {
    let mut out = vec![];
    each_process(|entry| {
        if entry.th32ParentProcessID == pid && entry.th32ProcessID != pid {
            out.push(entry.th32ProcessID);
        }
    })?;
    Ok(out)
}

/// The dynamic range's default since Windows Vista.
pub fn first_handed_out_port() -> u16 {
    49152
}

pub fn all_pids() -> Vec<u32> {
    let mut out = vec![];
    let _ = each_process(|entry| out.push(entry.th32ProcessID));
    out
}

pub fn name(pid: u32) -> Option<String> {
    let mut found = None;
    each_process(|entry| {
        if entry.th32ProcessID == pid {
            let end = entry.szExeFile.iter().position(|c| *c == 0);
            let text = String::from_utf16_lossy(&entry.szExeFile[..end.unwrap_or(260)]);
            found = Some(text.strip_suffix(".exe").map(str::to_owned).unwrap_or(text));
        }
    })
    .ok()?;
    found
}

/// A handle that closes itself.
fn open(pid: u32) -> Option<OwnedHandle> {
    // SAFETY: the handle goes straight into an OwnedHandle.
    unsafe {
        let raw = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        (!raw.is_null()).then(|| OwnedHandle::from_raw_handle(raw))
    }
}

pub fn executable(pid: u32) -> io::Result<String> {
    let process = open(pid).ok_or_else(io::Error::last_os_error)?;
    let mut path = vec![0u16; 32 * 1024];
    let mut len = path.len() as u32;
    // SAFETY: `len` says how long the buffer is.
    let ok = unsafe {
        QueryFullProcessImageNameW(process.as_raw_handle(), 0, path.as_mut_ptr(), &mut len)
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(String::from_utf16_lossy(&path[..len as usize]))
}

pub fn stdin_is_a_channel(_pid: u32) -> bool {
    false
}

/// Task Manager's Memory column: private working set is not on offer without walking pages,
/// private bytes is the closest.
pub fn memory(pid: u32) -> Option<u64> {
    let process = open(pid)?;
    let mut counters: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    // SAFETY: `size` is what `counters` holds.
    let ok = unsafe {
        K32GetProcessMemoryInfo(process.as_raw_handle(), (&raw mut counters).cast(), size)
    };
    (ok != 0).then_some(counters.PrivateUsage as u64)
}

pub fn usage(pid: u32) -> Option<super::Usage> {
    let process = open(pid)?;
    let as_ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    // SAFETY: each struct is written by its call.
    unsafe {
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [created, exited, kernel, user] = &mut times;
        if GetProcessTimes(process.as_raw_handle(), created, exited, kernel, user) == 0 {
            return None;
        }
        let mut io: IO_COUNTERS = std::mem::zeroed();
        GetProcessIoCounters(process.as_raw_handle(), &mut io);
        Some(super::Usage {
            cpu: std::time::Duration::from_nanos((as_ticks(times[2]) + as_ticks(times[3])) * 100),
            disk_read: io.ReadTransferCount,
            disk_written: io.WriteTransferCount,
            instructions: 0,
            syscalls: 0,
        })
    }
}

pub fn started_micros(pid: u32) -> Option<u64> {
    let process = open(pid)?;
    // SAFETY: each FILETIME is written by the call.
    unsafe {
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [created, exited, kernel, user] = &mut times;
        (GetProcessTimes(process.as_raw_handle(), created, exited, kernel, user) != 0)
            .then(|| micros_since_epoch(times[0]))
    }
}

/// A FILETIME counts 100 ns from 1601.
fn micros_since_epoch(at: FILETIME) -> u64 {
    const MICROS_TO_1970: u64 = 11_644_473_600_000_000;
    let ticks = (u64::from(at.dwHighDateTime) << 32) | u64::from(at.dwLowDateTime);
    (ticks / 10).saturating_sub(MICROS_TO_1970)
}
