use std::io;
use std::path::PathBuf;

fn proc_file(pid: u32, name: &str) -> PathBuf {
    PathBuf::from(format!("/proc/{pid}/{name}"))
}

fn nul_separated(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.split(|b| *b == 0).filter(|part| !part.is_empty())
}

pub fn argv(pid: u32) -> io::Result<Vec<String>> {
    let bytes = std::fs::read(proc_file(pid, "cmdline"))?;
    Ok(nul_separated(&bytes)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect())
}

fn env_value(bytes: &[u8], key: &str) -> Option<String> {
    nul_separated(bytes)
        .find_map(|e| e.strip_prefix(key.as_bytes())?.strip_prefix(b"="))
        .map(|v| String::from_utf8_lossy(v).into_owned())
}

pub fn env_var(pid: u32, name: &str) -> Option<String> {
    env_value(&std::fs::read(proc_file(pid, "environ")).ok()?, name)
}

pub fn cwd(pid: u32) -> Option<String> {
    let path = std::fs::read_link(proc_file(pid, "cwd")).ok()?;
    Some(path.to_string_lossy().into_owned()).filter(|p| !p.is_empty())
}

/// The fields of /proc/<pid>/stat after the command, which may itself hold spaces and
/// parentheses: the first of them is the process state, field 3.
fn stat_fields(pid: u32) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(proc_file(pid, "stat")).ok()?;
    let (_, rest) = text.rsplit_once(')')?;
    Some(rest.split_whitespace().map(str::to_owned).collect())
}

/// Every thread's `children` file, or a walk of /proc on a kernel built without them.
pub fn children(pid: u32) -> io::Result<Vec<u32>> {
    let tasks = match std::fs::read_dir(proc_file(pid, "task")) {
        Ok(tasks) => tasks,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut out = vec![];
    for task in tasks.flatten() {
        match std::fs::read_to_string(task.path().join("children")) {
            Ok(text) => out.extend(
                text.split_whitespace()
                    .filter_map(|p| p.parse::<u32>().ok()),
            ),
            // The thread is there and the file is not: a kernel built without it.
            Err(e) if e.kind() == io::ErrorKind::NotFound && task.path().exists() => {
                return children_by_walking(pid);
            }
            // The thread just ended, and its children went to another one or with it.
            Err(_) => (),
        }
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

fn children_by_walking(pid: u32) -> io::Result<Vec<u32>> {
    let mut out = vec![];
    for entry in std::fs::read_dir("/proc")?.flatten() {
        let Some(child) = entry.file_name().to_str().and_then(|n| n.parse().ok()) else {
            continue;
        };
        // Field 4, the parent, is the second after the command.
        let parent = stat_fields(child).and_then(|f| f.get(1)?.parse::<u32>().ok());
        if parent == Some(pid) {
            out.push(child);
        }
    }
    Ok(out)
}

pub fn started_micros(pid: u32) -> Option<u64> {
    // Field 22, in clock ticks since boot.
    let ticks: u64 = stat_fields(pid)?.get(19)?.parse().ok()?;
    let per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    let per_second = u64::try_from(per_second).ok().filter(|t| *t > 0)?;
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let boot: u64 = stat
        .lines()
        .find_map(|l| l.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()?;
    Some(boot * 1_000_000 + ticks * 1_000_000 / per_second)
}

pub fn usage(pid: u32) -> Option<super::Usage> {
    // Fields 14 and 15, in clock ticks.
    let fields = stat_fields(pid)?;
    let user: u64 = fields.get(11)?.parse().ok()?;
    let system: u64 = fields.get(12)?.parse().ok()?;
    let per_second = u64::try_from(unsafe { libc::sysconf(libc::_SC_CLK_TCK) })
        .ok()
        .filter(|t| *t > 0)?;
    // Not everywhere readable, and a container may hide it: then no disk counts.
    let io = std::fs::read_to_string(proc_file(pid, "io")).unwrap_or_default();
    let count = |name: &str| {
        io.lines()
            .find_map(|l| l.strip_prefix(name)?.trim().parse::<u64>().ok())
            .unwrap_or(0)
    };
    Some(super::Usage {
        cpu: std::time::Duration::from_micros((user + system) * 1_000_000 / per_second),
        disk_read: count("read_bytes:"),
        disk_written: count("write_bytes:"),
        instructions: 0,
        syscalls: 0,
    })
}

pub fn first_handed_out_port() -> u16 {
    std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range")
        .ok()
        .and_then(|range| range.split_whitespace().next()?.parse().ok())
        .unwrap_or(32768)
}

pub fn all_pids() -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return vec![];
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse().ok())
        .collect()
}

/// The program's file, as macOS names a process. Where that is not ours to read, or there is
/// no file (a kernel thread), what the kernel calls it, of which it keeps 15 bytes.
pub fn name(pid: u32) -> Option<String> {
    if let Ok(path) = std::fs::read_link(proc_file(pid, "exe")) {
        // npm's `claude` is a native file called `claude.exe`, on Linux too.
        let file = path.file_name()?.to_string_lossy();
        return Some(file.strip_suffix(".exe").unwrap_or(&file).to_owned());
    }
    let name = std::fs::read_to_string(proc_file(pid, "comm")).ok()?;
    Some(name.trim_end_matches('\n').to_owned())
}

pub fn executable(pid: u32) -> io::Result<String> {
    Ok(std::fs::read_link(proc_file(pid, "exe"))?
        .to_string_lossy()
        .into_owned())
}

pub fn stdin_is_a_channel(pid: u32) -> bool {
    std::fs::read_link(proc_file(pid, "fd/0")).is_ok_and(|target| {
        let target = target.to_string_lossy();
        target.starts_with("pipe:") || target.starts_with("socket:")
    })
}

/// What it holds in memory, as `top` shows it.
pub fn memory(pid: u32) -> Option<u64> {
    let statm = std::fs::read_to_string(proc_file(pid, "statm")).ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok()?;
    Some(pages * page)
}

/// /proc/<pid>/fd's targets.
fn open_targets(pid: u32) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(proc_file(pid, "fd")) else {
        return vec![];
    };
    entries
        .flatten()
        .filter_map(|e| std::fs::read_link(e.path()).ok())
        .collect()
}

pub fn open_files(pids: &[u32]) -> io::Result<std::collections::HashMap<u32, Vec<String>>> {
    Ok(pids
        .iter()
        .map(|pid| {
            let files = open_targets(*pid)
                .into_iter()
                .filter(|t| t.is_absolute())
                .map(|t| t.to_string_lossy().into_owned())
                .collect();
            (*pid, files)
        })
        .collect())
}

/// An address in /proc/net/tcp's hex, whose bytes run backwards in each 32-bit word.
fn is_loopback(hex: &str) -> bool {
    match hex.len() {
        8 => hex.ends_with("7F"),
        32 => {
            hex == "00000000000000000000000001000000"
                || (hex.starts_with("0000000000000000FFFF0000") && hex.ends_with("7F"))
        }
        _ => false,
    }
}

/// One row of /proc/net/tcp: (socket's inode, listening, port it is about).
fn parse_tcp_row(row: &str) -> Option<(u64, bool, u16)> {
    let fields: Vec<&str> = row.split_whitespace().collect();
    let (local, remote) = (fields.get(1)?, fields.get(2)?);
    let inode = fields.get(9)?.parse().ok()?;
    let port = |addr: &str| u16::from_str_radix(addr.rsplit(':').next()?, 16).ok();
    match *fields.get(3)? {
        "0A" => Some((inode, true, port(local)?)),
        "01" if is_loopback(remote.split(':').next()?) => Some((inode, false, port(remote)?)),
        _ => None,
    }
}

pub fn tcp_sockets(pids: &[u32]) -> io::Result<super::TcpSockets> {
    let mut out = super::TcpSockets::default();
    for pid in pids {
        let inodes: Vec<u64> = open_targets(*pid)
            .iter()
            .filter_map(|t| {
                t.to_str()?
                    .strip_prefix("socket:[")?
                    .strip_suffix(']')?
                    .parse()
                    .ok()
            })
            .collect();
        if inodes.is_empty() {
            continue;
        }
        for table in ["net/tcp", "net/tcp6"] {
            // No such table: no IPv6, or the process just ended.
            let Ok(text) = std::fs::read_to_string(proc_file(*pid, table)) else {
                continue;
            };
            for (inode, listening, port) in text.lines().skip(1).filter_map(parse_tcp_row) {
                if inodes.contains(&inode) {
                    let list = if listening {
                        &mut out.listening
                    } else {
                        &mut out.connected
                    };
                    list.push((*pid, port));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{env_value, is_loopback, parse_tcp_row};

    #[test]
    fn tcp_rows_say_who_listens_and_who_connects_here() {
        let row = |local: &str, remote: &str, state: &str| {
            format!(
                "  0: {local} {remote} {state} 00000000:00000000 00:00000000 00000000  1000  0 42 1"
            )
        };
        let listen = row("0100007F:1F90", "00000000:0000", "0A");
        assert_eq!(parse_tcp_row(&listen), Some((42, true, 8080)));
        let here = row("0100007F:C000", "0100007F:1F90", "01");
        assert_eq!(parse_tcp_row(&here), Some((42, false, 8080)));
        let away = row("0A00020F:C000", "08080808:01BB", "01");
        assert_eq!(parse_tcp_row(&away), None);
        let closing = row("0100007F:C000", "0100007F:1F90", "06");
        assert_eq!(parse_tcp_row(&closing), None);
    }

    #[test]
    fn loopback_in_each_of_its_spellings() {
        assert!(is_loopback("0100007F"));
        assert!(is_loopback("00000000000000000000000001000000"));
        assert!(is_loopback("0000000000000000FFFF00000100007F"));
        assert!(!is_loopback("0A00020F"));
        assert!(!is_loopback("0000000000000000FFFF00000A00020F"));
    }

    #[test]
    fn a_variable_is_its_whole_name() {
        let environ = b"HOME=/u\0MOORING_AGENT_X=1\0MOORING_AGENT=codex\0EMPTY=\0";
        assert_eq!(
            env_value(environ, "MOORING_AGENT").as_deref(),
            Some("codex")
        );
        assert_eq!(env_value(environ, "EMPTY").as_deref(), Some(""));
        assert_eq!(env_value(environ, "MOORING"), None);
    }
}
