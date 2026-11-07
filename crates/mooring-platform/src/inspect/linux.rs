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

pub fn started(pid: u32) -> Option<u64> {
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
    Some(boot + ticks / per_second)
}

#[cfg(test)]
mod tests {
    use super::env_value;

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
