use std::collections::HashSet;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::Mutex;
use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessCommandLineInformation,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_INSUFFICIENT_BUFFER, FILETIME,
    INVALID_HANDLE_VALUE, LocalFree, STATUS_INFO_LENGTH_MISMATCH, UNICODE_STRING,
};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP_STATE_ESTAB, MIB_TCP_STATE_LISTEN, MIB_TCP6ROW_OWNER_PID,
    MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows_sys::Win32::Storage::FileSystem::{FILE_TYPE_PIPE, GetFileType};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessIoCounters, GetProcessTimes, IO_COUNTERS, OpenProcess,
    PROCESS_BASIC_INFORMATION, PROCESS_DUP_HANDLE, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

/// Not read yet: finding what another process holds open means going through its handles, and
/// asking about one can hang. Hooks, arguments and screens tie an Agent to its conversation
/// here; add this if a real CLI turns out to need it (docs/platform/index.html).
pub fn open_files(_pids: &[u32]) -> io::Result<std::collections::HashMap<u32, Vec<String>>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "what another process holds open is not read on Windows",
    ))
}

/// A table of the system's connections of one address family, in memory aligned for its rows.
fn tcp_table(family: u16) -> io::Result<Vec<u64>> {
    let mut size = 0u32;
    let mut table: Vec<u64> = vec![];
    loop {
        // SAFETY: `size` says how much of `table` may be written.
        let code = unsafe {
            GetExtendedTcpTable(
                table.as_mut_ptr().cast(),
                &mut size,
                0,
                u32::from(family),
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        match code {
            0 => return Ok(table),
            // Connections come and go between asking the size and asking for them.
            ERROR_INSUFFICIENT_BUFFER => table.resize((size as usize).div_ceil(8) + 1, 0),
            code => return Err(io::Error::from_raw_os_error(code as i32)),
        }
    }
}

/// A port as these tables keep it: in network order, in the low half.
fn port(field: u32) -> u16 {
    u16::from_be(field as u16)
}

fn is_loopback_v6(address: &[u8; 16]) -> bool {
    std::net::Ipv6Addr::from(*address).is_loopback()
        || std::net::Ipv6Addr::from(*address)
            .to_ipv4_mapped()
            .is_some_and(|v4| v4.is_loopback())
}

/// Only those asked about are kept; the system has no way to ask for them alone.
pub fn tcp_sockets(pids: &[u32]) -> io::Result<super::TcpSockets> {
    let mut out = super::TcpSockets::default();
    let mut keep = |pid: u32, state: u32, local: u32, remote: u32, here: bool| {
        if !pids.contains(&pid) {
            return;
        }
        if state == MIB_TCP_STATE_LISTEN as u32 {
            out.listening.push((pid, port(local)));
        } else if state == MIB_TCP_STATE_ESTAB as u32 && here {
            out.connected.push((pid, port(remote)));
        }
    };
    let v4 = tcp_table(AF_INET)?;
    // SAFETY: the system wrote a table of that shape, and says how many rows it holds.
    let rows = unsafe {
        let table = v4.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
        std::slice::from_raw_parts(
            (&raw const (*table).table).cast::<MIB_TCPROW_OWNER_PID>(),
            (*table).dwNumEntries as usize,
        )
    };
    for r in rows {
        let here = r.dwRemoteAddr.to_ne_bytes()[0] == 127;
        keep(
            r.dwOwningPid,
            r.dwState,
            r.dwLocalPort,
            r.dwRemotePort,
            here,
        );
    }
    let v6 = tcp_table(AF_INET6)?;
    // SAFETY: as above.
    let rows = unsafe {
        let table = v6.as_ptr().cast::<MIB_TCP6TABLE_OWNER_PID>();
        std::slice::from_raw_parts(
            (&raw const (*table).table).cast::<MIB_TCP6ROW_OWNER_PID>(),
            (*table).dwNumEntries as usize,
        )
    };
    for r in rows {
        let here = is_loopback_v6(&r.ucRemoteAddr);
        keep(
            r.dwOwningPid,
            r.dwState,
            r.dwLocalPort,
            r.dwRemotePort,
            here,
        );
    }
    Ok(out)
}

/// A process's arguments are the only part the system will give out directly, even for a process
/// of a higher level. It keeps them as one line: this splits it as the C runtime will have.
pub fn argv(pid: u32) -> io::Result<Vec<String>> {
    let gone = || io::Error::new(io::ErrorKind::NotFound, "no such process");
    let process = open(pid).ok_or_else(gone)?;
    let mut size = 0u32;
    let mut buffer = vec![0u64; 512];
    let line: Vec<u16> = loop {
        // SAFETY: the length given is what `buffer` holds, and a success leaves a UNICODE_STRING
        // at its start pointing further into it.
        unsafe {
            let status = NtQueryInformationProcess(
                process.as_raw_handle(),
                ProcessCommandLineInformation,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut size,
            );
            if status == STATUS_INFO_LENGTH_MISMATCH {
                buffer.resize((size as usize).div_ceil(8) + 1, 0);
                continue;
            }
            if status != 0 {
                return Err(io::Error::other(format!(
                    "reading the arguments of {pid}: status {status:#x}"
                )));
            }
            let text = &*buffer.as_ptr().cast::<UNICODE_STRING>();
            let chars = std::slice::from_raw_parts(text.Buffer, usize::from(text.Length) / 2);
            break chars.iter().copied().chain([0]).collect();
        }
    };
    let mut count = 0;
    // SAFETY: `line` ends in a zero; the array that comes back is freed once, after reading it.
    unsafe {
        let args = CommandLineToArgvW(line.as_ptr(), &mut count);
        if args.is_null() {
            return Err(io::Error::last_os_error());
        }
        let out = std::slice::from_raw_parts(args, count as usize)
            .iter()
            .map(|arg| {
                let len = (0..).take_while(|i| *arg.add(*i) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(*arg, len))
            })
            .collect();
        LocalFree(args.cast());
        Ok(out)
    }
}

// Where a 64-bit process keeps these. The system has no call for another process's environment
// or directory; debuggers and Process Explorer read them from here, and they have stayed put
// since Windows Vista. A 32-bit process has this view as well.
const PEB_PARAMETERS: usize = 0x20;
const PARAMETERS_STDIN: usize = 0x20;
const PARAMETERS_DIRECTORY: usize = 0x38;
const PARAMETERS_ENVIRONMENT: usize = 0x80;
const PARAMETERS_ENVIRONMENT_SIZE: usize = 0x3f0;
/// More than any real environment: a wrong size read must not become a huge allocation.
const MOST_ENVIRONMENT: usize = 16 * 1024 * 1024;

/// A process whose memory this account may read, and where its parameters are.
fn with_parameters(pid: u32) -> Option<(OwnedHandle, usize)> {
    if !alive(pid) {
        return None;
    }
    // SAFETY: the handle goes straight into an OwnedHandle; `info` is the size given.
    let (process, peb) = unsafe {
        let raw = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if raw.is_null() {
            return None;
        }
        let process = OwnedHandle::from_raw_handle(raw);
        let mut info: PROCESS_BASIC_INFORMATION = std::mem::zeroed();
        let status = NtQueryInformationProcess(
            process.as_raw_handle(),
            ProcessBasicInformation,
            (&raw mut info).cast(),
            std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            std::ptr::null_mut(),
        );
        if status != 0 {
            return None;
        }
        (process, info.PebBaseAddress as usize)
    };
    let parameters = read_word(&process, peb + PEB_PARAMETERS)?;
    Some((process, parameters))
}

fn read_memory(process: &OwnedHandle, at: usize, into: &mut [u8]) -> Option<usize> {
    let mut read = 0;
    // SAFETY: at most `into.len()` bytes are written to it.
    let ok = unsafe {
        ReadProcessMemory(
            process.as_raw_handle(),
            at as *const _,
            into.as_mut_ptr().cast(),
            into.len(),
            &mut read,
        )
    };
    // A partial read fails with the part read: an environment may end before its stated size.
    (ok != 0 || read > 0).then_some(read)
}

fn read_word(process: &OwnedHandle, at: usize) -> Option<usize> {
    let mut word = [0u8; 8];
    (read_memory(process, at, &mut word)? == 8).then(|| usize::from_ne_bytes(word))
}

fn wide_units(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_ne_bytes(*c))
        .collect()
}

/// `name`'s value in an environment block. Names here do not care about case.
fn value_in(block: &[u16], name: &str) -> Option<String> {
    let wanted: Vec<u16> = name.encode_utf16().collect();
    block
        .split(|c| *c == 0)
        .take_while(|entry| !entry.is_empty())
        .find_map(|entry| {
            // Past the first character: `=C:=C:\x` is how a drive's directory is kept.
            let eq = 1 + entry.iter().skip(1).position(|c| *c == u16::from(b'='))?;
            let same = String::from_utf16_lossy(&entry[..eq]).eq_ignore_ascii_case(name)
                && eq == wanted.len();
            same.then(|| String::from_utf16_lossy(&entry[eq + 1..]))
        })
}

/// What the process holds now, which is what it started with unless it set some itself.
pub fn env_var(pid: u32, name: &str) -> Option<String> {
    let (process, parameters) = with_parameters(pid)?;
    let block = read_word(&process, parameters + PARAMETERS_ENVIRONMENT)?;
    let size = read_word(&process, parameters + PARAMETERS_ENVIRONMENT_SIZE)?;
    let mut bytes = vec![0u8; size.min(MOST_ENVIRONMENT)];
    let read = read_memory(&process, block, &mut bytes)?;
    value_in(&wide_units(&bytes[..read]), name)
}

/// In the form [`crate::fs::canonical`] gives, so a directory compares equal to one read elsewhere.
pub fn cwd(pid: u32) -> Option<String> {
    let (process, parameters) = with_parameters(pid)?;
    // A UNICODE_STRING: its length in bytes, then past the padding where it is.
    let mut length = [0u8; 2];
    read_memory(&process, parameters + PARAMETERS_DIRECTORY, &mut length)?;
    let text = read_word(&process, parameters + PARAMETERS_DIRECTORY + 8)?;
    let mut bytes = vec![0u8; usize::from(u16::from_ne_bytes(length))];
    let read = read_memory(&process, text, &mut bytes)?;
    let path = String::from_utf16_lossy(&wide_units(&bytes[..read]));
    Some(match crate::fs::canonical(std::path::Path::new(&path)) {
        Ok(real) => real.to_string_lossy().into_owned(),
        Err(_) => path,
    })
}

/// Goes through a snapshot of the system's processes, as process and parent.
pub(crate) fn each_process(mut visit: impl FnMut(u32, u32)) -> io::Result<()> {
    each_entry(|entry| visit(entry.th32ProcessID, entry.th32ParentProcessID))
}

/// Goes through a snapshot of the system's processes.
fn each_entry(mut visit: impl FnMut(&PROCESSENTRY32W)) -> io::Result<()> {
    // SAFETY: a snapshot of the system's processes, closed below on every path.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `entry` is the size it says it is, and the snapshot is open.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        visit(&entry);
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    Ok(())
}

/// Whose parent each process says it is. Windows never clears that number, so a parent long
/// gone may name a process that has since been given its number: see `below`.
fn parents() -> io::Result<Vec<(u32, u32)>> {
    let mut out = vec![];
    each_entry(|entry| out.push((entry.th32ProcessID, entry.th32ParentProcessID)))?;
    Ok(out)
}

/// Who `parent` really started: only a process younger than its parent can be its child. One
/// the account may not ask about is not this account's terminal's child either.
fn below(
    entries: &[(u32, u32)],
    parent: u32,
    started: &mut impl FnMut(u32) -> Option<u64>,
) -> Vec<u32> {
    let mut named = entries
        .iter()
        .filter(|(pid, of)| *of == parent && *pid != parent)
        .map(|(pid, _)| *pid)
        .peekable();
    if named.peek().is_none() {
        return vec![];
    }
    let Some(born) = started(parent) else {
        return vec![];
    };
    named
        .filter(|pid| started(*pid).is_some_and(|at| at >= born))
        .collect()
}

pub fn children(pid: u32) -> io::Result<Vec<u32>> {
    Ok(below(&parents()?, pid, &mut started_micros))
}

/// One look at the system's processes, however many there are to walk: a look costs
/// milliseconds on any machine, so one per process added up to most of a second.
pub fn descendants(pids: &[u32]) -> io::Result<Vec<(u32, u32)>> {
    let entries = parents()?;
    super::walk(pids, |pid| Ok(below(&entries, pid, &mut started_micros)))
}

/// The dynamic range's default since Windows Vista.
pub fn first_handed_out_port() -> u16 {
    49152
}

pub fn all_pids() -> Vec<u32> {
    let mut out = vec![];
    let _ = each_entry(|entry| out.push(entry.th32ProcessID));
    out.retain(|pid| alive(*pid));
    out
}

/// Asks the process; only one the account may not open needs a look through all of them.
pub fn name(pid: u32) -> Option<String> {
    if !alive(pid) {
        return None;
    }
    if let Ok(path) = executable(pid) {
        let file = path.rsplit('\\').next().unwrap_or(&path);
        return Some(strip_exe(file).to_owned());
    }
    let mut found = None;
    each_entry(|entry| {
        if entry.th32ProcessID == pid {
            let end = entry.szExeFile.iter().position(|c| *c == 0);
            let text = String::from_utf16_lossy(&entry.szExeFile[..end.unwrap_or(260)]);
            found = Some(strip_exe(&text).to_owned());
        }
    })
    .ok()?;
    found
}

fn strip_exe(file: &str) -> &str {
    file.strip_suffix(".exe").unwrap_or(file)
}

/// A process lasts as long as anyone holds a handle to it, a virus scanner included, and until
/// then the system still lists it and still answers about it.
fn alive(pid: u32) -> bool {
    i32::try_from(pid).is_ok_and(crate::process::exists)
}

/// A handle that closes itself.
fn open(pid: u32) -> Option<OwnedHandle> {
    if !alive(pid) {
        return None;
    }
    // SAFETY: the handle goes straight into an OwnedHandle.
    unsafe {
        let raw = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        (!raw.is_null()).then(|| OwnedHandle::from_raw_handle(raw))
    }
}

pub fn executable(pid: u32) -> io::Result<String> {
    let process = open(pid).ok_or_else(io::Error::last_os_error)?;
    let mut path = vec![0u16; 32 * 1024];
    let mut len = path.len() as u32;
    // SAFETY: `len` says how long the buffer is.
    let ok = unsafe {
        QueryFullProcessImageNameW(process.as_raw_handle(), 0, path.as_mut_ptr(), &mut len)
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(String::from_utf16_lossy(&path[..len as usize]))
}

/// Its standard input, as a handle of this process.
fn dup_stdin(pid: u32) -> Option<OwnedHandle> {
    let (process, parameters) = with_parameters(pid)?;
    let theirs = read_word(&process, parameters + PARAMETERS_STDIN)?;
    // SAFETY: each handle goes straight into an OwnedHandle.
    unsafe {
        let raw = OpenProcess(PROCESS_DUP_HANDLE, 0, pid);
        if raw.is_null() {
            return None;
        }
        let source = OwnedHandle::from_raw_handle(raw);
        let mut mine = std::ptr::null_mut();
        let copied = DuplicateHandle(
            source.as_raw_handle(),
            theirs as _,
            GetCurrentProcess(),
            &mut mine,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        );
        (copied != 0).then(|| OwnedHandle::from_raw_handle(mine))
    }
}

/// The system answers about a pipe only once a read blocked on it ends, so it is asked on a thread
/// that may wait a long time. No answer in time means a process reading a pipe: nothing else stays
/// blocked like that here. Such a process is remembered, so it costs one waiting thread at most.
pub fn stdin_is_a_channel(pid: u32) -> bool {
    static BLOCKED: Mutex<Option<HashSet<(u32, u64)>>> = Mutex::new(None);
    let Some(started) = started_micros(pid) else {
        return false;
    };
    let known = |set: &mut Option<HashSet<(u32, u64)>>| {
        set.get_or_insert_default().contains(&(pid, started))
    };
    if known(&mut BLOCKED.lock().unwrap_or_else(|e| e.into_inner())) {
        return true;
    }
    let Some(stdin) = dup_stdin(pid) else {
        return false;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // SAFETY: the handle is open for as long as this thread holds it.
        let kind = unsafe { GetFileType(stdin.as_raw_handle()) };
        let _ = tx.send(kind == FILE_TYPE_PIPE);
    });
    match rx.recv_timeout(std::time::Duration::from_millis(200)) {
        Ok(is_pipe) => is_pipe,
        Err(_) => {
            BLOCKED
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_default()
                .insert((pid, started));
            true
        }
    }
}

/// Task Manager's Memory column: private working set is not on offer without walking pages,
/// private bytes is the closest.
pub fn memory(pid: u32) -> Option<u64> {
    let process = open(pid)?;
    let mut counters: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    // SAFETY: `size` is what `counters` holds.
    let ok = unsafe {
        K32GetProcessMemoryInfo(process.as_raw_handle(), (&raw mut counters).cast(), size)
    };
    (ok != 0).then_some(counters.PrivateUsage as u64)
}

pub fn usage(pid: u32) -> Option<super::Usage> {
    let process = open(pid)?;
    let as_ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    // SAFETY: each struct is written by its call.
    unsafe {
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [created, exited, kernel, user] = &mut times;
        if GetProcessTimes(process.as_raw_handle(), created, exited, kernel, user) == 0 {
            return None;
        }
        let mut io: IO_COUNTERS = std::mem::zeroed();
        GetProcessIoCounters(process.as_raw_handle(), &mut io);
        Some(super::Usage {
            cpu: std::time::Duration::from_nanos((as_ticks(times[2]) + as_ticks(times[3])) * 100),
            disk_read: io.ReadTransferCount,
            disk_written: io.WriteTransferCount,
            instructions: 0,
            syscalls: 0,
        })
    }
}

pub fn started_micros(pid: u32) -> Option<u64> {
    let process = open(pid)?;
    // SAFETY: each FILETIME is written by the call.
    unsafe {
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [created, exited, kernel, user] = &mut times;
        (GetProcessTimes(process.as_raw_handle(), created, exited, kernel, user) != 0)
            .then(|| micros_since_epoch(times[0]))
    }
}

/// A FILETIME counts 100 ns from 1601.
fn micros_since_epoch(at: FILETIME) -> u64 {
    const MICROS_TO_1970: u64 = 11_644_473_600_000_000;
    let ticks = (u64::from(at.dwHighDateTime) << 32) | u64::from(at.dwLowDateTime);
    (ticks / 10).saturating_sub(MICROS_TO_1970)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_variable_is_found_whatever_its_case_and_only_by_its_whole_name() {
        let block: Vec<u16> = "=C:=C:\\x\0Path=a=b\0PATHEXT=.exe\0\0after=no\0"
            .encode_utf16()
            .collect();
        assert_eq!(value_in(&block, "PATH").as_deref(), Some("a=b"));
        assert_eq!(value_in(&block, "pathext").as_deref(), Some(".exe"));
        assert_eq!(value_in(&block, "PAT"), None);
        assert_eq!(value_in(&block, "after"), None);
    }

    #[test]
    fn a_connection_here_may_be_ipv6_or_ipv4_inside_it() {
        assert!(is_loopback_v6(&std::net::Ipv6Addr::LOCALHOST.octets()));
        let mapped = std::net::Ipv4Addr::LOCALHOST.to_ipv6_mapped();
        assert!(is_loopback_v6(&mapped.octets()));
        let away = std::net::Ipv4Addr::new(8, 8, 8, 8).to_ipv6_mapped();
        assert!(!is_loopback_v6(&away.octets()));
    }

    #[test]
    fn a_port_comes_out_of_network_order() {
        assert_eq!(port(u32::from(8080u16.to_be())), 8080);
    }

    /// Numbers: 10 is a terminal started at time 100. 7 started long before it and names 10 as
    /// its parent, because its real parent died and 10 was handed out again; 8 is 7's child.
    /// Taken at its word, that pulls most of a machine into one terminal.
    #[test]
    fn a_number_handed_out_again_does_not_make_a_stranger_a_child() {
        let entries = [(7, 10), (8, 7), (11, 10), (12, 11)];
        let mut started = |pid| match pid {
            7 => Some(50),
            8 => Some(60),
            10 => Some(100),
            11 => Some(110),
            12 => Some(120),
            _ => None,
        };
        assert_eq!(below(&entries, 10, &mut started), vec![11]);
        assert_eq!(below(&entries, 11, &mut started), vec![12]);
        // Started in the same tick still counts: the clock is coarser than process creation.
        let mut same = |_| Some(100);
        assert_eq!(below(&entries, 10, &mut same), vec![7, 11]);
        // Gone, or not ours to ask about: not a child.
        let mut unknown = |pid| (pid == 10).then_some(100);
        assert!(below(&entries, 10, &mut unknown).is_empty());
    }
}
