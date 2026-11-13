//! The pseudo console. Reading it can only block, so a thread reads and `Reader` waits on what
//! it passes along. Closing the console waits until its output is read, which that thread does
//! to the end.
use super::{Read, Spec};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read as _, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::mpsc::{Receiver, RecvTimeoutError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFO, COORD, ClosePseudoConsole, CreatePseudoConsole,
    GetConsoleScreenBufferInfo, GetStdHandle, HPCON, ResizePseudoConsole, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcess, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
    PROCESS_INFORMATION, PROCESS_SYNCHRONIZE, STARTUPINFOEXW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};

/// The console, until either the program's end or the last master closes it.
struct Console(Mutex<Option<ConsoleHandle>>);
struct ConsoleHandle(HPCON);
// SAFETY: a console handle is not tied to the thread that made it.
unsafe impl Send for ConsoleHandle {}

impl Console {
    /// On a thread of its own: closing waits for the output to be read.
    fn close(self: &Arc<Self>) {
        let this = Arc::clone(self);
        std::thread::spawn(move || {
            let taken = this.0.lock().unwrap_or_else(|p| p.into_inner()).take();
            if let Some(console) = taken {
                // SAFETY: taken out, so closed once.
                unsafe { ClosePseudoConsole(console.0) };
            }
        });
    }
}

fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let (mut read, mut write): (HANDLE, HANDLE) = (std::ptr::null_mut(), std::ptr::null_mut());
    // SAFETY: both are written by the call and go straight into owners.
    unsafe {
        if CreatePipe(&mut read, &mut write, std::ptr::null(), 0) == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        ))
    }
}

/// One argument the way the C runtime reads it back.
fn quote(arg: &str, out: &mut String) {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        out.push_str(arg);
        return;
    }
    out.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            backslashes += 1;
            continue;
        }
        let doubled = if c == '"' {
            backslashes * 2 + 1
        } else {
            backslashes
        };
        out.extend(std::iter::repeat_n('\\', doubled));
        backslashes = 0;
        out.push(c);
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Sorted without regard to case, as Windows wants it.
fn environment_block(env: &[(std::ffi::OsString, std::ffi::OsString)]) -> Vec<u16> {
    let mut sorted: Vec<_> = env.iter().collect();
    sorted.sort_by_key(|(k, _)| k.to_string_lossy().to_uppercase());
    let mut block = vec![];
    for (k, v) in sorted {
        block.extend(k.encode_wide());
        block.push(u16::from(b'='));
        block.extend(v.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

pub fn spawn(spec: &Spec) -> io::Result<(Child, Master)> {
    let (input_read, input_write) = pipe()?;
    let (output_read, output_write) = pipe()?;
    let size = COORD {
        X: spec.cols as i16,
        Y: spec.rows as i16,
    };
    let mut console: HPCON = 0 as HPCON;
    // SAFETY: the pipes stay open through the call, which duplicates them.
    let made = unsafe {
        CreatePseudoConsole(
            size,
            input_read.as_raw_handle(),
            output_write.as_raw_handle(),
            0,
            &mut console,
        )
    };
    if made < 0 {
        return Err(io::Error::from_raw_os_error(made));
    }
    let console = Arc::new(Console(Mutex::new(Some(ConsoleHandle(console)))));
    let started = start(spec, &console);
    drop(input_read);
    drop(output_write);
    let (process, pid) = match started {
        Ok(started) => started,
        Err(e) => {
            console.close();
            return Err(e);
        }
    };
    // The console stays open after its program ends, and output never ends until it closes.
    // SAFETY: a new handle, closed by its owner.
    let waited = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if !waited.is_null() {
        let waited = unsafe { OwnedHandle::from_raw_handle(waited) };
        let console = Arc::clone(&console);
        std::thread::spawn(move || {
            // SAFETY: `waited` is open for as long as this thread waits on it.
            unsafe { WaitForSingleObject(waited.as_raw_handle(), INFINITE) };
            console.close();
        });
    }
    let (sender, received) = sync_channel(8);
    let mut output = File::from(output_read);
    std::thread::spawn(move || {
        let mut buffer = vec![0; 64 * 1024];
        // A broken pipe is how it ends.
        while let Ok(n) = output.read(&mut buffer) {
            if n == 0 || sender.send(buffer[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    Ok((
        Child { process, pid },
        Master {
            console,
            input: File::from(input_write),
            received: Mutex::new(Some(received)),
        },
    ))
}

fn start(spec: &Spec, console: &Console) -> io::Result<(OwnedHandle, u32)> {
    let mut line = String::new();
    for (i, arg) in spec.argv.iter().enumerate() {
        if i > 0 {
            line.push(' ');
        }
        quote(arg, &mut line);
    }
    let mut line = wide(OsStr::new(&line));
    let cwd = wide(spec.cwd.as_os_str());
    let env = environment_block(&spec.env);
    let guard = console.0.lock().unwrap_or_else(|p| p.into_inner());
    let hpc = guard
        .as_ref()
        .ok_or_else(|| io::Error::other("the console is closed"))?
        .0;
    // SAFETY: the list is sized by the first call, initialized by the second, freed below; the
    // strings outlive CreateProcessW, and each handle it returns goes into an owner or is shut.
    unsafe {
        let mut bytes = 0usize;
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut bytes);
        let mut list = vec![0u8; bytes];
        let attributes: LPPROC_THREAD_ATTRIBUTE_LIST = list.as_mut_ptr().cast();
        if InitializeProcThreadAttributeList(attributes, 1, 0, &mut bytes) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut startup: STARTUPINFOEXW = std::mem::zeroed();
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = attributes;
        // Without this a program would take the host's own standard handles over the console.
        startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
        startup.StartupInfo.dwFlags = windows_sys::Win32::System::Threading::STARTF_USESTDHANDLES;
        let result = if UpdateProcThreadAttribute(
            attributes,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            hpc as *const _,
            std::mem::size_of::<HPCON>(),
            std::ptr::null_mut(),
            std::ptr::null(),
        ) == 0
        {
            Err(io::Error::last_os_error())
        } else {
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            if CreateProcessW(
                std::ptr::null(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                env.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            ) == 0
            {
                Err(io::Error::last_os_error())
            } else {
                drop(OwnedHandle::from_raw_handle(info.hThread));
                Ok((
                    OwnedHandle::from_raw_handle(info.hProcess),
                    info.dwProcessId,
                ))
            }
        };
        DeleteProcThreadAttributeList(attributes);
        result
    }
}

pub struct Child {
    process: OwnedHandle,
    pid: u32,
}

impl Child {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn kill(&mut self) -> io::Result<()> {
        // SAFETY: a process this handle owns; one already gone is an error nobody needs.
        unsafe { TerminateProcess(self.process.as_raw_handle(), 1) };
        Ok(())
    }
    pub fn wait(&mut self) -> io::Result<Option<i32>> {
        // SAFETY: both only read from a handle this owns.
        unsafe {
            if WaitForSingleObject(self.process.as_raw_handle(), INFINITE) != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
            let mut code = 0u32;
            if GetExitCodeProcess(self.process.as_raw_handle(), &mut code) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Some(code as i32))
        }
    }
}

pub struct Master {
    console: Arc<Console>,
    input: File,
    received: Mutex<Option<Receiver<Vec<u8>>>>,
}

impl Drop for Master {
    fn drop(&mut self) {
        self.console.close();
    }
}

impl Master {
    /// A pipe that blocks, unlike the Unix side: nothing here gives up after five seconds.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut accepted = 0;
        while accepted < bytes.len() {
            let end = bytes.len().min(accepted + 256);
            match self.input.write(&bytes[accepted..end]) {
                Ok(0) => {
                    return Err(io::Error::other(format!(
                        "终端已关闭，已接受 {accepted} 字节"
                    )));
                }
                Ok(n) => accepted += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(io::Error::other(format!("{e}; 已接受 {accepted} 字节"))),
            }
        }
        Ok(())
    }

    /// A pseudo console has no pixel size.
    pub fn resize(&self, cols: u16, rows: u16, _cell: (u32, u32)) -> io::Result<()> {
        let guard = self.console.0.lock().unwrap_or_else(|p| p.into_inner());
        let Some(console) = guard.as_ref() else {
            return Ok(());
        };
        let size = COORD {
            X: cols as i16,
            Y: rows as i16,
        };
        // SAFETY: open while the lock keeps it from being closed.
        let done = unsafe { ResizePseudoConsole(console.0, size) };
        if done < 0 {
            return Err(io::Error::from_raw_os_error(done));
        }
        Ok(())
    }

    pub fn backlog(&self) -> Option<usize> {
        None
    }

    pub fn foreground_group(&self) -> Option<i32> {
        None
    }

    pub fn reader(&self) -> io::Result<Reader> {
        let received = self
            .received
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
            .ok_or_else(|| io::Error::other("the terminal has one reader"))?;
        Ok(Reader {
            received,
            pending: vec![],
            at: 0,
        })
    }
}

pub struct Reader {
    received: Receiver<Vec<u8>>,
    pending: Vec<u8>,
    at: usize,
}

impl Reader {
    pub fn next(&mut self, buffer: &mut [u8], wait: Option<Duration>) -> io::Result<Read> {
        if self.at >= self.pending.len() {
            let got = match wait {
                Some(wait) => self.received.recv_timeout(wait),
                None => self
                    .received
                    .recv()
                    .map_err(|_| RecvTimeoutError::Disconnected),
            };
            match got {
                Ok(bytes) => {
                    self.pending = bytes;
                    self.at = 0;
                }
                Err(RecvTimeoutError::Timeout) => return Ok(Read::Quiet),
                Err(RecvTimeoutError::Disconnected) => return Ok(Read::Closed),
            }
        }
        let n = buffer.len().min(self.pending.len() - self.at);
        buffer[..n].copy_from_slice(&self.pending[self.at..self.at + n]);
        self.at += n;
        Ok(Read::Data(n))
    }
}

pub fn own_size() -> Option<(u16, u16)> {
    // SAFETY: the info is written by the call.
    unsafe {
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) == 0 {
            return None;
        }
        let window = info.srWindow;
        Some((
            (window.Right - window.Left + 1) as u16,
            (window.Bottom - window.Top + 1) as u16,
        ))
    }
}
