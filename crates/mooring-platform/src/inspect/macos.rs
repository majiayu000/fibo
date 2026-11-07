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

#[cfg(test)]
mod tests {
    use super::{env_value, memory};

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
