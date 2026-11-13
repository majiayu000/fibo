use super::Memory;
use std::io;
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::GetSystemTimes;

pub fn memory() -> Option<Memory> {
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `dwLength` says how much `status` holds.
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 || status.ullTotalPhys == 0 {
        return None;
    }
    Some(Memory {
        total: status.ullTotalPhys,
        used: status.ullTotalPhys.saturating_sub(status.ullAvailPhys),
        compressed: 0,
        wired: 0,
        cached: 0,
        swap: status
            .ullTotalPageFile
            .saturating_sub(status.ullAvailPageFile)
            .saturating_sub(status.ullTotalPhys.saturating_sub(status.ullAvailPhys)),
        pressure: None,
    })
}

/// Hundredths of a second, as the others count. System time includes idle here.
pub fn cpu_ticks() -> io::Result<[u32; 4]> {
    let ticks =
        |t: FILETIME| ((u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)) / 100_000;
    let (mut idle, mut kernel, mut user) = unsafe { std::mem::zeroed() };
    // SAFETY: each FILETIME is written by the call.
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let (idle, kernel, user) = (ticks(idle), ticks(kernel), ticks(user));
    Ok([
        user as u32,
        kernel.saturating_sub(idle) as u32,
        idle as u32,
        0,
    ])
}

pub fn host_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

/// This system's announcements are not listened to yet.
pub fn watch_pressure(_changed: fn()) {}

pub fn announced_pressure() -> Option<i32> {
    None
}

pub fn swap_used() -> u64 {
    0
}
