//! Being told when a process ends, without polling.
use std::io;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

pub struct ExitWatch(imp::ExitWatch);

impl ExitWatch {
    /// `on_exit` is called with each watched process's id, once, on a thread of the watch's.
    pub fn new(on_exit: impl Fn(u32) + Send + 'static) -> io::Result<Self> {
        imp::ExitWatch::new(on_exit).map(Self)
    }

    /// Starts watching `pid`. False closes the registration race: the process is already
    /// gone and `on_exit` will not be called for it. A process that ended a moment ago may
    /// answer true instead, and then `on_exit` is called.
    pub fn watch(&self, pid: u32) -> io::Result<bool> {
        self.0.watch(pid)
    }
}
