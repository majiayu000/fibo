//! Reading what a running process is: its arguments, environment, directory, children and
//! start time. Asks the kernel for one process at a time, never scans for it by name.
//!
//! Arguments, environment and directory are refused with `Unsupported` on Windows for now:
//! reading them there means another process's memory, which the Windows chain (step 4 in
//! docs/platform/index.html) adds.
use std::io;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

/// The arguments a process was started with, split where it was given them, so a prompt
/// is never parsed out of a command line.
pub fn argv(pid: u32) -> io::Result<Vec<String>> {
    imp::argv(pid)
}

/// One variable of the environment a process started with. Nothing else of the environment
/// is kept, and arguments that look like a variable are not it. The account must own the
/// process.
pub fn env_var(pid: u32, name: &str) -> Option<String> {
    imp::env_var(pid, name)
}

/// A process's current directory.
pub fn cwd(pid: u32) -> Option<String> {
    imp::cwd(pid)
}

/// The processes `pid` started that are still running. Empty when `pid` itself is gone.
pub fn children(pid: u32) -> io::Result<Vec<u32>> {
    imp::children(pid)
}

/// When a process started, seconds since the epoch.
pub fn started(pid: u32) -> Option<u64> {
    imp::started(pid)
}
