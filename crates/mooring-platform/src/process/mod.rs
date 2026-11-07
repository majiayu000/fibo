//! Ending a command together with what it started, and asking whether a process is there.
//!
//! macOS and Linux use a process group. Windows has none, so there a job object plays that
//! part.
use std::io;
use std::process::{Child, Command};

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

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

/// Whether a process with this id is running. Only a positive number is a process id: a stale
/// or garbled pid file must not read as a process being there.
pub fn exists(pid: i32) -> bool {
    pid > 0 && imp::exists(pid)
}

/// Ends one process at once. Only a positive number is a process id: on Unix the others mean a
/// whole group, or every process this account may signal.
pub fn kill(pid: i32) {
    if pid > 0 {
        imp::kill(pid);
    }
}

/// Asks one process to end, where that can be asked. Windows cannot: it ends there and then.
pub fn terminate(pid: i32) -> io::Result<()> {
    if pid <= 0 {
        return Ok(());
    }
    imp::terminate(pid)
}

/// What closing a terminal does to a job in it: time to save and go, for what listens. `group`
/// is what `terminal::Master::foreground_group` gives, or the program a terminal was started
/// with. One already gone is fine.
pub fn hang_up_group(group: i32) -> io::Result<()> {
    if group <= 0 {
        return Ok(());
    }
    imp::hang_up_group(group)
}

/// Ends everything in `group` at once. One already gone is fine.
pub fn kill_group(group: i32) -> io::Result<()> {
    if group <= 0 {
        return Ok(());
    }
    imp::kill_group(group)
}

/// Whether a process was ended from outside rather than ending itself.
pub fn was_signalled(status: &std::process::ExitStatus) -> bool {
    imp::was_signalled(status)
}

/// The calling thread gives way to everything else, from now on.
pub fn give_way() {
    imp::give_way();
}

/// The process that started this one.
pub fn parent() -> u32 {
    imp::parent()
}

/// This process ends when the one that started it does, however that one went.
pub fn exit_with_parent() {
    imp::exit_with_parent();
}

/// Raises how many files this process may hold open as far as it is allowed. The new limit,
/// if there is such a limit and it changed.
pub fn raise_open_file_limit() -> io::Result<Option<u64>> {
    imp::raise_open_file_limit()
}
