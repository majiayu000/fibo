//! Which of an agent's background shells are jobs it waits for, and which are servers.
//!
//! Claude Code wakes the agent when a background shell exits, so a shell that will end
//! (tests, a build, a download, a polling loop) keeps the turn working. A server never
//! ends: a turn that left only servers behind is over. A shell whose processes listen
//! on a chosen TCP port is a server; so is one running longer than [`JOB_LIMIT`], which
//! covers what listens on nothing (`tail -f`, a watcher). A port the system handed out
//! (bound to 0, so in the ephemeral range) does not count: that is a tool talking to its
//! own helper, like a renderer serving a page to its headless browser. Shells are found
//! among the agent's child processes by the command the Stop hook listed.
use std::collections::{HashMap, HashSet};
use std::io;
use std::process::Command;
use std::sync::atomic::Ordering;

/// A job still running after this long is taken for something that does not end.
pub const JOB_LIMIT: u64 = 2 * 3600;
/// Seconds between looks while some turn has shells left.
pub const EVERY: u64 = 5;
/// macOS hands out ports from here up (`net.inet.ip.portrange.first`) to a socket bound
/// to port 0; a server meant to be visited is put on a port below it.
const EPHEMERAL: u16 = 49152;

/// What one look found for a shell: its process tree listens on a port, and how long
/// it has run. `None` from [`look`] when no child process runs the command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub listening: bool,
    pub age: u64,
}

/// The judgment carried between looks. A shell changes sides only after two looks in a
/// row disagree with it, so a test that starts a server for a moment stays a job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Judged {
    pub job: bool,
    against: u8,
}
pub fn judge(before: Option<Judged>, seen: Option<Seen>) -> Judged {
    let now = seen.is_some_and(|s| !s.listening && s.age < JOB_LIMIT);
    match before {
        None => Judged {
            job: now,
            against: 0,
        },
        // Too old to be a job: no second look needed.
        Some(_) if seen.is_some_and(|s| s.age >= JOB_LIMIT) => Judged {
            job: false,
            against: 0,
        },
        Some(b) if b.job == now => Judged { against: 0, ..b },
        Some(b) if b.against >= 1 => Judged {
            job: now,
            against: 0,
        },
        Some(b) => Judged {
            against: b.against + 1,
            ..b
        },
    }
}

/// Looks at the shells `agent` started: for each `(task id, command)`, what its process
/// shows, or nothing when no child runs that command. One `lsof` covers every tree.
pub fn look(targets: &[(u32, Vec<(String, String)>)]) -> io::Result<HashMap<String, Seen>> {
    let now = mooring_core::content::now();
    // task id -> (shell pid, its whole tree)
    let mut found: Vec<(String, u32, Vec<u32>)> = Vec::new();
    for (agent, tasks) in targets {
        let mut shells: Vec<(u32, String)> = children(*agent)
            .into_iter()
            .filter_map(|pid| {
                let argv = crate::recognition::process_argv(pid).ok()?;
                Some((pid, argv.join(" ")))
            })
            .collect();
        for (id, command) in tasks {
            let Some(at) = shells.iter().position(|(_, line)| runs(line, command)) else {
                continue;
            };
            let (pid, _) = shells.remove(at);
            found.push((id.clone(), pid, tree(pid)));
        }
    }
    let all: Vec<u32> = found
        .iter()
        .flat_map(|(_, _, t)| t.iter().copied())
        .collect();
    let listening = listeners(&all)?;
    Ok(found
        .into_iter()
        .map(|(id, pid, tree)| {
            let seen = Seen {
                listening: tree.iter().any(|p| listening.contains(p)),
                age: started(pid).map_or(0, |at| now.saturating_sub(at)),
            };
            (id, seen)
        })
        .collect())
}

/// Whether a shell's argv runs `command`. Claude Code passes it as `eval '<command>'`,
/// each `'` inside written `'\''`; the hook clips a long command with "… [+N chars]".
fn runs(argv: &str, command: &str) -> bool {
    let command = command.split("… [+").next().unwrap_or(command);
    let head: String = command.chars().take(60).collect();
    if head.trim().is_empty() {
        return false;
    }
    argv.contains(&head.replace('\'', "'\\''")) || argv.contains(&head)
}

fn children(pid: u32) -> Vec<u32> {
    let mut buf = vec![0i32; 256];
    let n = unsafe {
        libc::proc_listchildpids(
            pid as i32,
            buf.as_mut_ptr().cast(),
            (buf.len() * std::mem::size_of::<i32>()) as i32,
        )
    };
    if n <= 0 {
        return vec![];
    }
    buf[..(n as usize).min(buf.len())]
        .iter()
        .filter(|p| **p > 0)
        .map(|p| *p as u32)
        .collect()
}
fn tree(root: u32) -> Vec<u32> {
    let mut out = vec![];
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    while let Some(pid) = pending.pop() {
        if seen.insert(pid) {
            out.push(pid);
            pending.extend(children(pid));
        }
    }
    out
}
/// When the process started, seconds since the epoch.
fn started(pid: u32) -> Option<u64> {
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
    (n == size).then_some(info.pbi_start_tvsec)
}
/// The pids among `pids` listening on a chosen (not ephemeral) TCP port.
fn listeners(pids: &[u32]) -> io::Result<HashSet<u32>> {
    if pids.is_empty() {
        return Ok(HashSet::new());
    }
    let list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    crate::recognition::LSOF_SPAWNS.fetch_add(1, Ordering::Relaxed);
    let out = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-a", "-p", &list, "-iTCP", "-sTCP:LISTEN", "-Fpn"])
        .output()?;
    // Exit 1 with no rows is "none listening".
    Ok(chosen_ports(&String::from_utf8_lossy(&out.stdout)))
}
/// From `lsof -Fpn` rows (`p<pid>` then one `n<addr>:<port>` per socket), the pids with
/// at least one socket on a port below [`EPHEMERAL`].
fn chosen_ports(rows: &str) -> HashSet<u32> {
    let mut out = HashSet::new();
    let mut pid = None;
    for line in rows.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().ok();
        } else if let (Some(addr), Some(p)) = (line.strip_prefix('n'), pid) {
            let port = addr.rsplit(':').next().and_then(|n| n.parse::<u16>().ok());
            if port.is_some_and(|n| n < EPHEMERAL) {
                out.insert(p);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const YOUNG: Seen = Seen {
        listening: false,
        age: 60,
    };
    const SERVER: Seen = Seen {
        listening: true,
        age: 60,
    };
    #[test]
    fn a_shell_is_a_job_until_two_looks_in_a_row_find_it_listening() {
        let first = judge(None, Some(YOUNG));
        assert!(first.job, "a fresh shell listening on nothing is a job");
        let once = judge(Some(first), Some(SERVER));
        assert!(once.job, "one look at a port is not enough");
        assert!(judge(Some(once), Some(YOUNG)).job);
        let twice = judge(Some(once), Some(SERVER));
        assert!(!twice.job, "two looks in a row make it a server");
        assert!(!judge(None, Some(SERVER)).job, "listening at first sight");
    }
    #[test]
    fn an_old_shell_or_one_not_found_is_no_job() {
        let old = Seen {
            listening: false,
            age: JOB_LIMIT,
        };
        assert!(!judge(Some(judge(None, Some(YOUNG))), Some(old)).job);
        assert!(!judge(None, None).job, "no process runs the command");
        let job = judge(None, Some(YOUNG));
        assert!(
            judge(Some(job), None).job,
            "a job that just ended stays one until its agent wakes"
        );
    }
    #[test]
    fn only_a_chosen_port_makes_a_server() {
        let rows = "p100\nf12\nn127.0.0.1:51174\np200\nf3\nn*:8000\np300\nf4\nn[::1]:5173\nf5\nn[::1]:60000\n";
        let got = chosen_ports(rows);
        assert!(!got.contains(&100), "a renderer's own ephemeral port");
        assert!(got.contains(&200) && got.contains(&300), "{got:?}");
    }
    #[test]
    fn a_command_is_found_in_the_shell_that_evals_it() {
        let argv = "/bin/zsh -c source /x/snapshot.sh 2>/dev/null || true && eval 'python3 -m http.server 8000 --bind 127.0.0.1' < /dev/null && pwd -P >| /tmp/cwd";
        assert!(runs(argv, "python3 -m http.server 8000 --bind 127.0.0.1"));
        let quoted = "/bin/zsh -c eval 'echo '\\''hi'\\'' && sleep 9' < /dev/null";
        assert!(runs(quoted, "echo 'hi' && sleep 9"));
        assert!(runs(argv, "python3 -m http.server 8000… [+40 chars]"));
        assert!(!runs(argv, "cargo test"));
        assert!(!runs(argv, "  "));
    }
    #[test]
    fn real_children_are_found_by_command_and_only_a_chosen_port_reads_as_a_server() {
        let job = "sleep 30; echo job-7f3a";
        // Bound to 0: the system picks an ephemeral port, as a renderer does for its browser.
        let helper = r#"python3 -c 'import socket,time; s=socket.socket(); s.bind(("127.0.0.1", 0)); s.listen(); time.sleep(30)' # eph-7f3a"#;
        let server = r#"python3 -c 'import socket,time
for p in range(41000, 41200):
    s = socket.socket()
    try:
        s.bind(("127.0.0.1", p)); break
    except OSError:
        pass
s.listen(); time.sleep(30)' # srv-7f3a"#;
        let spawn = |command: &str| {
            std::process::Command::new("/bin/sh")
                .args(["-c", &format!("eval '{}'", command.replace('\'', "'\\''"))])
                .spawn()
                .unwrap()
        };
        let mut children = [spawn(job), spawn(helper), spawn(server)];
        let tasks = vec![
            ("j".to_owned(), job.to_owned()),
            ("e".to_owned(), helper.to_owned()),
            ("s".to_owned(), server.to_owned()),
            ("gone".to_owned(), "never started".to_owned()),
        ];
        // The servers need a moment to bind.
        let mut seen = HashMap::new();
        for _ in 0..50 {
            seen = look(&[(std::process::id(), tasks.clone())]).unwrap();
            if seen.get("s").is_some_and(|s| s.listening) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // Give the helper as long again, so a miss on it is not a slow bind.
        std::thread::sleep(std::time::Duration::from_millis(300));
        let again = look(&[(std::process::id(), tasks.clone())]).unwrap();
        for c in &mut children {
            let _ = c.kill();
            let _ = c.wait();
        }
        assert_eq!(seen.get("j").map(|s| s.listening), Some(false), "{seen:?}");
        assert!(seen["j"].age < 60);
        assert_eq!(seen.get("s").map(|s| s.listening), Some(true), "{seen:?}");
        assert_eq!(
            again.get("e").map(|s| s.listening),
            Some(false),
            "{again:?}"
        );
        assert!(!seen.contains_key("gone"));
    }
}
