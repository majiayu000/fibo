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
