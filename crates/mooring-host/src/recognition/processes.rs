use super::{Process, PS_SPAWNS};
use mooring_core::model::{Agent, ProcessState, Workspace};
use std::{
    collections::{HashMap, HashSet},
    io,
};

use std::sync::atomic::Ordering;
/// Read argv boundaries from the kernel, never parse a prompt out of `ps command`.
/// KERN_PROCARGS2 also returns an environment tail; stop after argc and never retain it.
pub(crate) fn process_argv(pid: u32) -> io::Result<Vec<String>> {
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
/// Claude Code writes no transcript when it believes it is a nested child session, so
/// nothing can ever bind it. Only the variable's name is checked; no value is kept.
pub fn claude_transcript_off(pid: u32) -> bool {
    procargs(pid).is_ok_and(|bytes| env_has(&bytes, "CLAUDE_CODE_CHILD_SESSION"))
}
/// The value of one environment variable from a KERN_PROCARGS2 buffer; nothing else of
/// the environment is read past it or kept.
pub(super) fn env_value(bytes: &[u8], key: &str) -> Option<String> {
    let (argc, pos) = argv_start(bytes)?;
    bytes[pos..]
        .split(|b| *b == 0)
        .skip(argc)
        .find_map(|e| e.strip_prefix(key.as_bytes())?.strip_prefix(b"="))
        .map(|v| String::from_utf8_lossy(v).into_owned())
}
/// The Agent a `MOORING_AGENT` value names, by key or executable, in any case.
pub(super) fn agent_named(value: &str) -> Option<Agent> {
    let value = value.trim();
    Agent::all()
        .into_iter()
        .find(|a| a.key().eq_ignore_ascii_case(value) || a.executable().eq_ignore_ascii_case(value))
}
/// Which hinted processes stand for an Agent: the outermost of each chain (children
/// inherit the variable), and only where no process below is recognized by name, so a
/// wrapper that leaves the real CLI visible is not counted twice.
pub(super) fn hinted_roots(
    raw: &[(u32, u32, Option<Agent>, Option<Agent>)],
) -> HashMap<u32, Agent> {
    let parent: HashMap<u32, u32> = raw.iter().map(|r| (r.0, r.1)).collect();
    let hinted: HashMap<u32, Agent> = raw.iter().filter_map(|r| Some((r.0, r.3?))).collect();
    let named: Vec<u32> = raw.iter().filter(|r| r.2.is_some()).map(|r| r.0).collect();
    let below = |root: u32, pid: u32| {
        let mut seen = HashSet::new();
        let mut at = pid;
        while seen.insert(at) {
            match parent.get(&at) {
                Some(p) if *p == root => return true,
                Some(p) => at = *p,
                None => return false,
            }
        }
        false
    };
    hinted
        .iter()
        .filter(|(pid, _)| !parent.get(*pid).is_some_and(|p| hinted.contains_key(p)))
        .filter(|(pid, _)| !named.iter().any(|n| below(**pid, *n)))
        .map(|(pid, agent)| (*pid, *agent))
        .collect()
}
pub(super) fn env_has(bytes: &[u8], key: &str) -> bool {
    let Some((argc, pos)) = argv_start(bytes) else {
        return false;
    };
    let mut entries = bytes[pos..].split(|b| *b == 0).skip(argc);
    entries.any(|e| {
        e.strip_prefix(key.as_bytes())
            .is_some_and(|rest| rest.first() == Some(&b'='))
    })
}
/// Enumerate only the trees rooted at live PTYs. No global ps or process-name scan.
pub fn processes(workspace: &Workspace) -> io::Result<Vec<Process>> {
    let mut pending: Vec<_> = workspace
        .terminals
        .iter()
        .filter(|t| t.state == ProcessState::Running)
        .filter_map(|t| t.pid)
        .collect();
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    let mut parents = HashMap::new();
    while let Some(pid) = pending.pop() {
        if !seen.insert(pid) {
            continue;
        }
        selected.push(pid);
        let count = unsafe { libc::proc_listchildpids(pid as i32, std::ptr::null_mut(), 0) };
        if count < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(error);
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
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(error);
        }
        if count as usize >= children.len() {
            return Err(io::Error::other(
                "managed process tree changed during enumeration; retry on next event",
            ));
        }
        for child in children[..count as usize].iter().filter(|p| **p > 0) {
            parents.insert(*child as u32, pid);
            pending.push(*child as u32);
        }
    }
    if selected.is_empty() {
        return Ok(vec![]);
    }
    // Read in-process rather than spawning `/bin/ps` per hook: the walk above already knows
    // each child's parent, and ps reads the same KERN_PROCARGS2 argv for its command.
    selected.sort_unstable();
    PS_SPAWNS.fetch_add(1, Ordering::Relaxed);
    // pid, parent, the Agent its name shows, the Agent a MOORING_AGENT hint names.
    let mut raw = vec![];
    for pid in selected {
        let parent = parents.get(&pid).copied().unwrap_or(0);
        let command = process_argv(pid).unwrap_or_default().join(" ");
        let runtime = crate::agents::from_command(&command);
        let hint = if runtime.is_none() {
            procargs(pid)
                .ok()
                .and_then(|b| env_value(&b, mooring_core::protocol::AGENT_ENV))
                .and_then(|v| agent_named(&v))
        } else {
            None
        };
        raw.push((pid, parent, runtime, hint));
    }
    let hinted = hinted_roots(&raw);
    let mut processes = vec![];
    for (pid, parent, runtime, _) in raw {
        let runtime = runtime.or_else(|| hinted.get(&pid).copied());
        let args = if runtime.is_some() {
            process_argv(pid).unwrap_or_default()
        } else {
            vec![]
        };
        let (cli_session_id, no_daemon) = runtime
            .map(|a| crate::agents::adapter(a).process_session(pid, &args))
            .unwrap_or_default();
        let launch = runtime
            .map(|a| crate::agents::adapter(a).launch_flags(&args))
            .unwrap_or_default();
        processes.push(Process {
            pid,
            parent,
            runtime,
            cli_session_id,
            no_daemon,
            launch,
        });
    }
    Ok(processes)
}
pub fn process_cwd(pid: u32) -> Option<String> {
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
