//! Ending a command together with what it started.

/// SIGKILL to the process group `child` leads (it was spawned with `process_group(0)`).
/// The group is gone once all its members are; a stale id is harmless (ESRCH).
pub fn kill_group(child: &std::process::Child) {
    // SAFETY: a signal to a process group this program created.
    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
}
