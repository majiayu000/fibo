//! How busy the machine is.
use std::io;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

/// Physical memory, in the system monitor's own terms. What a system does not separate out
/// is zero: only macOS compresses, and Windows does not report a file cache here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub used: u64,
    pub compressed: u64,
    pub wired: u64,
    /// Given back as soon as anything needs it.
    pub cached: u64,
    pub swap: u64,
    /// How hard pressed it is, where the system says: 1 normal, 2 warn, 4 critical.
    pub pressure: Option<i32>,
}

/// Physical memory right now. None when the system does not answer.
pub fn memory() -> Option<Memory> {
    imp::memory()
}

/// CPU time spent since boot over all cores, as user, system, idle, nice. Only the difference
/// between two readings means anything, and each count may wrap.
pub fn cpu_ticks() -> io::Result<[u32; 4]> {
    imp::cpu_ticks()
}
