//! Reading what a running process is: its arguments, environment, directory, children and
//! start time. Asks the kernel for one process at a time, never scans for it by name.
//!
//! Arguments, environment, directory, sockets and open files are refused with `Unsupported`
//! on Windows for now: the Windows chain (step 4 in docs/platform/index.html) adds them.
use std::collections::HashMap;
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
    started_micros(pid).map(|at| at / 1_000_000)
}

/// When a process started, microseconds since the epoch, as finely as the system keeps it.
pub fn started_micros(pid: u32) -> Option<u64> {
    imp::started_micros(pid)
}

/// Every process there is, this account's or not. For asking once, not for looping.
pub fn all_pids() -> Vec<u32> {
    imp::all_pids()
}

/// The name of the file a process runs, without a path or `.exe`.
pub fn name(pid: u32) -> Option<String> {
    imp::name(pid).filter(|n| !n.is_empty())
}

/// The absolute path of the program a process runs.
pub fn executable(pid: u32) -> io::Result<String> {
    imp::executable(pid)
}

/// Whether a process's standard input is a pipe or a socket rather than a file or terminal.
/// Windows does not say of another process: false there.
pub fn stdin_is_a_channel(pid: u32) -> bool {
    imp::stdin_is_a_channel(pid)
}

/// The memory a process accounts for, in bytes, the way the system's own monitor counts
/// it. None for what this account may not read.
pub fn memory(pid: u32) -> Option<u64> {
    imp::memory(pid)
}

/// The TCP sockets some processes hold.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TcpSockets {
    /// (pid, port) it listens on.
    pub listening: Vec<(u32, u16)>,
    /// (pid, port): a connection from that process to a port on this machine.
    pub connected: Vec<(u32, u16)>,
}

/// What these processes listen on and connect to. A process that ended meanwhile has none.
pub fn tcp_sockets(pids: &[u32]) -> io::Result<TcpSockets> {
    if pids.is_empty() {
        return Ok(TcpSockets::default());
    }
    imp::tcp_sockets(pids)
}

/// The paths of the files these processes hold open. A process that ended meanwhile has none.
pub fn open_files(pids: &[u32]) -> io::Result<HashMap<u32, Vec<String>>> {
    if pids.is_empty() {
        return Ok(HashMap::new());
    }
    imp::open_files(pids)
}

/// What a process has used since it started, for telling how heavy some work was by the
/// difference between two readings. What a system does not count stays zero: instructions
/// and system calls are macOS only, and in a virtual machine there are no instructions either.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub cpu: std::time::Duration,
    pub disk_read: u64,
    pub disk_written: u64,
    pub instructions: u64,
    pub syscalls: u64,
}

/// None for what this account may not read, and for a process that is gone.
pub fn usage(pid: u32) -> Option<Usage> {
    imp::usage(pid)
}

/// Where the ports start that the system hands out to a socket bound to port 0. A port below
/// it was chosen by someone.
pub fn first_handed_out_port() -> u16 {
    imp::first_handed_out_port()
}
