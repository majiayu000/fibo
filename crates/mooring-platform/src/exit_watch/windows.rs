use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::mpsc::{Sender, channel};
use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, GetLastError};
use windows_sys::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

/// A blocked thread per watched process, since a wait on a handle has no shared queue to
/// join. They all report to one thread, so `on_exit` runs on one, as it does elsewhere.
pub struct ExitWatch {
    ended: Sender<u32>,
}

impl ExitWatch {
    pub fn new(on_exit: impl Fn(u32) + Send + 'static) -> io::Result<Self> {
        let (ended, reports) = channel();
        std::thread::spawn(move || {
            for pid in reports {
                on_exit(pid);
            }
        });
        Ok(Self { ended })
    }

    pub fn watch(&self, pid: u32) -> io::Result<bool> {
        // SAFETY: the handle goes straight into an OwnedHandle.
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if raw.is_null() {
            if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
                return Ok(false);
            }
            return Err(io::Error::last_os_error());
        }
        let process = unsafe { OwnedHandle::from_raw_handle(raw) };
        let ended = self.ended.clone();
        std::thread::spawn(move || {
            // SAFETY: `process` is open for as long as this thread waits on it.
            unsafe { WaitForSingleObject(process.as_raw_handle(), INFINITE) };
            let _ = ended.send(pid);
        });
        Ok(true)
    }
}
