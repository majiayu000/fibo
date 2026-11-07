use std::io;

/// KERN_PROCARGS2 also returns the environment, after the arguments. Only ever read through
/// `argv_start`, and never kept.
fn procargs(pid: u32) -> io::Result<Vec<u8>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut len = 0;
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut bytes = vec![0u8; len];
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    bytes.truncate(len);
    Ok(bytes)
}

/// argc and the offset of argv[0], past the executable path and its padding.
fn argv_start(bytes: &[u8]) -> Option<(usize, usize)> {
    if bytes.len() < 4 {
        return None;
    }
    let argc = i32::from_ne_bytes(bytes[..4].try_into().unwrap()).max(0) as usize;
    let end = bytes[4..].iter().position(|b| *b == 0)?;
    let mut pos = 4 + end;
    while bytes.get(pos) == Some(&0) {
        pos += 1;
    }
    Some((argc, pos))
}

pub fn argv(pid: u32) -> io::Result<Vec<String>> {
    let bytes = procargs(pid)?;
    let Some((argc, mut pos)) = argv_start(&bytes) else {
        return Ok(vec![]);
    };
    let mut args = Vec::new();
    for _ in 0..argc {
        let Some(end) = bytes[pos..].iter().position(|b| *b == 0) else {
            break;
        };
        args.push(String::from_utf8_lossy(&bytes[pos..pos + end]).into_owned());
        pos += end + 1;
    }
    Ok(args)
}

fn env_value(bytes: &[u8], key: &str) -> Option<String> {
    let (argc, pos) = argv_start(bytes)?;
    bytes[pos..]
        .split(|b| *b == 0)
        .skip(argc)
        .find_map(|e| e.strip_prefix(key.as_bytes())?.strip_prefix(b"="))
        .map(|v| String::from_utf8_lossy(v).into_owned())
}

pub fn env_var(pid: u32, name: &str) -> Option<String> {
    env_value(&procargs(pid).ok()?, name)
}

pub fn cwd(pid: u32) -> Option<String> {
    let mut info = std::mem::MaybeUninit::<libc::proc_vnodepathinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as i32;
    // SAFETY: the kernel writes at most `size` bytes into the zeroed buffer.
    let n = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if n != size {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let path = unsafe { std::ffi::CStr::from_ptr(info.pvi_cdir.vip_path.as_ptr().cast()) };
    Some(path.to_string_lossy().into_owned()).filter(|p| !p.is_empty())
}

pub fn children(pid: u32) -> io::Result<Vec<u32>> {
    let gone_or = |error: io::Error| match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(vec![]),
        _ => Err(error),
    };
    let count = unsafe { libc::proc_listchildpids(pid as i32, std::ptr::null_mut(), 0) };
    if count < 0 {
        return gone_or(io::Error::last_os_error());
    }
    let mut children = vec![0i32; count as usize + 32];
    let count = unsafe {
        libc::proc_listchildpids(
            pid as i32,
            children.as_mut_ptr().cast(),
            (children.len() * std::mem::size_of::<i32>()) as i32,
        )
    };
    if count < 0 {
        return gone_or(io::Error::last_os_error());
    }
    if count as usize >= children.len() {
        return Err(io::Error::other(
            "process tree changed during enumeration; retry on next event",
        ));
    }
    Ok(children[..count as usize]
        .iter()
        .filter(|p| **p > 0)
        .map(|p| *p as u32)
        .collect())
}

pub fn started_micros(pid: u32) -> Option<u64> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let n = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    (n == size).then_some(info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec)
}

/// proc_pid_rusage counts in Mach absolute time, which on Apple Silicon is not nanoseconds.
#[allow(deprecated)] // libc's binding is enough; no extra dependency for one call.
fn mach_time(ticks: u64) -> std::time::Duration {
    let mut base = libc::mach_timebase_info { numer: 0, denom: 0 };
    if unsafe { libc::mach_timebase_info(&mut base) } != 0 || base.denom == 0 {
        return std::time::Duration::from_nanos(ticks);
    }
    let nanos = u128::from(ticks) * u128::from(base.numer) / u128::from(base.denom);
    std::time::Duration::from_nanos(nanos as u64)
}

pub fn usage(pid: u32) -> Option<super::Usage> {
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    // SAFETY: the kernel fills the struct the flavor names.
    let read = unsafe {
        libc::proc_pid_rusage(
            pid as libc::c_int,
            libc::RUSAGE_INFO_V4,
            (&raw mut info).cast(),
        )
    };
    if read != 0 {
        return None;
    }
    let mut task: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&task) as i32;
    // SAFETY: the kernel writes at most `size` bytes.
    let n = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTASKINFO,
            0,
            (&raw mut task).cast(),
            size,
        )
    };
    Some(super::Usage {
        cpu: mach_time(info.ri_user_time + info.ri_system_time),
        disk_read: info.ri_diskio_bytesread,
        disk_written: info.ri_diskio_byteswritten,
        instructions: info.ri_instructions,
        syscalls: if n == size {
            u64::from(task.pti_syscalls_unix as u32)
        } else {
            0
        },
    })
}

/// `net.inet.ip.portrange.first`, whose default nobody changes.
pub fn first_handed_out_port() -> u16 {
    49152
}

pub fn all_pids() -> Vec<u32> {
    let mut buf = vec![0i32; 8192];
    let bytes = (buf.len() * std::mem::size_of::<i32>()) as i32;
    let n = unsafe { libc::proc_listallpids(buf.as_mut_ptr().cast(), bytes) };
    buf[..n.max(0) as usize]
        .iter()
        .filter(|p| **p > 0)
        .map(|p| *p as u32)
        .collect()
}

pub fn name(pid: u32) -> Option<String> {
    let mut buf = [0u8; 256];
    let n = unsafe { libc::proc_name(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32) };
    Some(String::from_utf8_lossy(&buf[..n.max(0) as usize]).into_owned())
}

pub fn executable(pid: u32) -> io::Result<String> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let n = unsafe {
        libc::proc_pidpath(
            pid as libc::c_int,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
        )
    };
    if n <= 0 {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(n as usize);
    String::from_utf8(buf).map_err(io::Error::other)
}

pub fn stdin_is_a_channel(pid: u32) -> bool {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FdInfo {
        fd: i32,
        kind: u32,
    }
    const PROC_PIDLISTFDS: i32 = 1;
    const SOCKET: u32 = 2;
    const PIPE: u32 = 6;
    let mut fds = [FdInfo { fd: -1, kind: 0 }; 64];
    let size = std::mem::size_of_val(&fds) as i32;
    let n = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            size,
        )
    };
    let count = n.max(0) as usize / std::mem::size_of::<FdInfo>();
    fds[..count.min(fds.len())]
        .iter()
        .any(|f| f.fd == 0 && matches!(f.kind, SOCKET | PIPE))
}

/// Activity Monitor's Memory column. Only our own user's processes answer; root and system
/// daemons return EPERM.
pub fn memory(pid: u32) -> Option<u64> {
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::proc_pid_rusage(
            pid as libc::c_int,
            libc::RUSAGE_INFO_V4,
            (&mut info as *mut libc::rusage_info_v4).cast(),
        )
    };
    (result == 0).then_some(info.ri_phys_footprint)
}

fn pid_list(pids: &[u32]) -> String {
    pids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

pub fn tcp_sockets(pids: &[u32]) -> io::Result<super::TcpSockets> {
    let out = std::process::Command::new("/usr/sbin/lsof")
        .args([
            "-nP",
            "-a",
            "-p",
            &pid_list(pids),
            "-iTCP",
            "-sTCP:LISTEN,ESTABLISHED",
            "-Fpn",
        ])
        .output()?;
    // Exit 1 with no rows is "none".
    Ok(parse_sockets(&String::from_utf8_lossy(&out.stdout)))
}

/// From `lsof -Fpn` rows: `p<pid>`, then one `n<addr>:<port>` per listening socket or
/// `n<addr>:<port>-><addr>:<port>` per connection.
fn parse_sockets(rows: &str) -> super::TcpSockets {
    let port = |addr: &str| addr.rsplit(':').next().and_then(|n| n.parse::<u16>().ok());
    let mut out = super::TcpSockets::default();
    let mut pid = None;
    for line in rows.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().ok();
        } else if let (Some(name), Some(p)) = (line.strip_prefix('n'), pid) {
            if let Some((_, remote)) = name.split_once("->") {
                let here = remote.starts_with("127.") || remote.starts_with("[::1]");
                if let (true, Some(n)) = (here, port(remote)) {
                    out.connected.push((p, n));
                }
            } else if let Some(n) = port(name) {
                out.listening.push((p, n));
            }
        }
    }
    out
}

pub fn open_files(pids: &[u32]) -> io::Result<std::collections::HashMap<u32, Vec<String>>> {
    let result = std::process::Command::new("/usr/sbin/lsof")
        .args(["-nP", "-p", &pid_list(pids), "-Fpn"])
        .output()?;
    // lsof can return 1 when one process exits during enumeration; valid rows remain useful.
    if !result.status.success() && result.stdout.is_empty() {
        if result.status.code() == Some(1) {
            return Ok(Default::default());
        }
        return Err(io::Error::other("open-file inventory failed"));
    }
    let mut files: std::collections::HashMap<u32, Vec<String>> = Default::default();
    let mut pid = None;
    for line in String::from_utf8_lossy(&result.stdout).lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse().ok();
        } else if let (Some(pid), Some(path)) = (pid, line.strip_prefix('n')) {
            files.entry(pid).or_default().push(path.into());
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::{env_value, memory, parse_sockets};

    #[test]
    fn lsof_rows_say_who_listens_and_who_connects_here() {
        // Rows as lsof printed them for a relay and its Python client.
        let rows = "p28079\nf5\nn*:8898\nf6\nn127.0.0.1:8898->127.0.0.1:49848\n\
                    p28382\nf4\nn127.0.0.1:49848->127.0.0.1:8898\n\
                    p500\nf4\nn[::1]:49900->[::1]:8898\n\
                    p600\nf4\nn10.0.0.2:49901->10.0.0.9:8898\n\
                    p700\nf3\nn[::1]:5173\n";
        let got = parse_sockets(rows);
        assert_eq!(got.listening, vec![(28079, 8898), (700, 5173)]);
        assert_eq!(
            got.connected,
            vec![(28079, 49848), (28382, 8898), (500, 8898)]
        );
    }

    #[test]
    fn launchd_belongs_to_root_so_its_memory_is_not_ours_to_read() {
        assert_eq!(memory(1), None);
    }

    fn procargs_of(argv: &[&str], env: &[&str]) -> Vec<u8> {
        let mut b = (argv.len() as i32).to_ne_bytes().to_vec();
        b.extend(b"/usr/bin/nono\0\0\0");
        for a in argv.iter().chain(env) {
            b.extend(a.as_bytes());
            b.push(0);
        }
        b.extend(b"\0executable_path=/usr/bin/nono\0");
        b
    }

    #[test]
    fn a_variable_is_read_only_from_the_environment_tail() {
        let key = "MOORING_AGENT";
        let b = procargs_of(
            &["nono", "MOORING_AGENT=codex"],
            &["HOME=/x", "MOORING_AGENT=Claude"],
        );
        assert_eq!(env_value(&b, key).as_deref(), Some("Claude"));
        for (argv, env) in [
            (&["nono", "MOORING_AGENT=1"][..], &["HOME=/u"][..]),
            (&["nono"], &["MOORING_AGENT_X=1"]),
            (&["nono"], &[]),
        ] {
            assert_eq!(env_value(&procargs_of(argv, env), key), None);
        }
        assert_eq!(env_value(b"\0\0", key), None);
    }
}
