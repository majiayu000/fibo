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

/// Keeps a command-line child from opening a console window of its own. The window and the
/// host have no console, so on Windows each such child gets a new one, which Windows 11 shows
/// as a Windows Terminal window that flashes and closes (issue #485). Elsewhere it does nothing.
/// Not for a command started with [`detach`], whose flags it would replace.
pub fn no_console(command: &mut std::process::Command) {
    imp::no_console(command)
}

/// Lets `command` outlive this process and whatever session it runs in; TERM still stops it
/// (`takes_term`).
pub fn detach(command: &mut std::process::Command) {
    imp::detach(command)
}

/// Lets TERM stop `command` though this process ignores it: a window's process does (AppKit
/// quits the app its own way), and what it starts would inherit that, so a forward left by a
/// window that crashed, or a host asked to stop, would not.
pub fn takes_term(command: &mut std::process::Command) {
    imp::takes_term(command)
}

/// Starts `program` with `args` once process `pid` has ended, for a program replacing itself.
pub fn start_after_exit(
    pid: u32,
    program: &std::path::Path,
    args: impl Iterator<Item = std::ffi::OsString>,
) -> io::Result<()> {
    imp::start_after_exit(pid, program, args.collect())
}

/// Runs `command` in place of this process and ends with its exit code: on Unix this
/// process becomes it (exec); on Windows it waits for it. Returns only when it cannot start.
pub fn become_command(command: &mut std::process::Command) -> io::Error {
    imp::become_command(command)
}

/// Ends this process at once, running nothing on the way out, as the abort after a panic would.
pub fn exit_now(code: i32) -> ! {
    imp::exit_now(code)
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

/// The processes still in a Unix group, even after its leader exits. Windows has no
/// process groups: only the named process identifies the tree `kill_group` can end.
pub fn group_members(group: i32) -> Vec<u32> {
    if group <= 0 {
        return vec![];
    }
    #[cfg(unix)]
    return imp::group_members(group);
    #[cfg(windows)]
    vec![group as u32]
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
