//! Which of an agent's background shells are jobs it waits for, and which are servers.
//!
//! Claude Code wakes the agent when a background shell exits, so a shell that will end
//! (tests, a build, a download, a polling loop) keeps the turn working. A server never
//! ends: a turn that left only servers behind is over. A shell whose processes listen
//! on a chosen TCP port is a server; so is one running longer than [`JOB_LIMIT`], which
//! covers what listens on nothing (`tail -f`, a watcher). A port the system handed out
//! (bound to 0, so in the ephemeral range) does not count: that is a tool talking to its
//! own helper, like a renderer serving a page to its headless browser. Neither does a
//! port another process of the same tree connects to: that server is a part of the
//! script, and ends with it. Shells are found among the agent's child processes by the
//! command the Stop hook listed, or after a host restart by what the agent's log says it
//! ran. The rules and the real cases behind them:
//! docs/decisions/2026-11-04-background-shell-own-client.md.
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

/// What one look found for a shell: its process tree listens on a port, another process
/// of the tree is connected to that port, something in the tree still works beside the
/// listeners (a leaf that listens on nothing), and how long it has run. `None` from
/// [`look`] when no child process runs the command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub listening: bool,
    pub own_client: bool,
    pub worker: bool,
    pub age: u64,
}

/// The judgment carried between looks. A shell changes sides only after two looks in a
/// row disagree with it, so a test that starts a server for a moment stays a job. The
/// exception is its own client: one sighting settles it at once, and it stays settled
/// while a worker is left, because such a connection comes and goes between requests.
/// Once only the server and the shells waiting on it remain, the work is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Judged {
    pub job: bool,
    against: u8,
    own_client: bool,
}
pub fn judge(before: Option<Judged>, seen: Option<Seen>) -> Judged {
    // Too old to be a job, whatever else it shows: no second look needed.
    if seen.is_some_and(|s| s.age >= JOB_LIMIT) {
        return Judged {
            job: false,
            against: 0,
            own_client: false,
        };
    }
    if seen.is_some_and(|s| s.own_client) {
        return Judged {
            job: true,
            against: 0,
            own_client: true,
        };
    }
    let own_client = before.is_some_and(|b| b.own_client);
    let now = seen.is_some_and(|s| !s.listening || (own_client && s.worker));
    match before {
        None => Judged {
            job: now,
            against: 0,
            own_client,
        },
        Some(b) if b.job == now => Judged { against: 0, ..b },
        Some(b) if b.against >= 1 => Judged {
            job: now,
            against: 0,
            own_client,
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
    let sockets = sockets(&all)?;
    Ok(found
        .into_iter()
        .map(|(id, pid, tree)| {
            let seen = Seen {
                listening: tree.iter().any(|p| sockets.listening.contains_key(p)),
                own_client: sockets.own_client(&tree),
                worker: tree
                    .iter()
                    .any(|p| !sockets.listening.contains_key(p) && children(*p).is_empty()),
                age: started(pid).map_or(0, |at| now.saturating_sub(at)),
            };
            (id, seen)
        })
        .collect())
}

/// `(task id, command)` for each of `ids` that this Claude Code log started. A host that
/// restarted has lost the commands its last Stop hooks listed, and that hook will not
/// run again while the shell works. The log keeps them, so nothing new is stored.
pub fn commands_in_log(log: &std::path::Path, ids: &[String]) -> io::Result<Vec<(String, String)>> {
    let mut reader = io::BufReader::new(std::fs::File::open(log)?);
    let mut commands: HashMap<String, String> = HashMap::new();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut line = Vec::new();
    while io::BufRead::read_until(&mut reader, b'\n', &mut line)? > 0 {
        // Most lines are neither: don't parse them.
        let used = contains(&line, b"\"name\":\"Bash\"");
        let started = contains(&line, b"with ID: ");
        if used || started {
            let value: serde_json::Value = serde_json::from_slice(&line).unwrap_or_default();
            let blocks = value.pointer("/message/content").and_then(|c| c.as_array());
            for block in blocks.into_iter().flatten() {
                if let (Some(id), Some(command)) = (
                    block["id"].as_str(),
                    block.pointer("/input/command").and_then(|c| c.as_str()),
                ) {
                    commands.insert(id.to_owned(), command.to_owned());
                }
                let Some(used_by) = block["tool_use_id"].as_str() else {
                    continue;
                };
                let text = block["content"].to_string();
                let task = ids.iter().find(|id| {
                    text.split("with ID: ")
                        .skip(1)
                        .any(|rest| rest.starts_with(id.as_str()))
                });
                // The first mention is the start; later ones are output quoting the id.
                if let (Some(task), Some(command)) = (task, commands.get(used_by)) {
                    if !out.iter().any(|(id, _)| id == task) {
                        out.push((task.clone(), command.clone()));
                    }
                }
            }
        }
        line.clear();
    }
    Ok(out)
}
fn contains(line: &[u8], needle: &[u8]) -> bool {
    line.windows(needle.len()).any(|w| w == needle)
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
pub(crate) fn started(pid: u32) -> Option<u64> {
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
/// The TCP sockets of some processes that say what a shell is.
#[derive(Debug, Default)]
struct Sockets {
    /// Pid -> the chosen (not ephemeral) ports it listens on.
    listening: HashMap<u32, HashSet<u16>>,
    /// (pid, port): a connection from that pid to a port on this machine.
    dialing: Vec<(u32, u16)>,
}
impl Sockets {
    /// Whether a process of `tree` is connected to a port another process of it listens on.
    fn own_client(&self, tree: &[u32]) -> bool {
        self.dialing.iter().any(|(client, port)| {
            tree.contains(client)
                && tree.iter().any(|server| {
                    server != client
                        && self
                            .listening
                            .get(server)
                            .is_some_and(|ports| ports.contains(port))
                })
        })
    }
}
fn sockets(pids: &[u32]) -> io::Result<Sockets> {
    if pids.is_empty() {
        return Ok(Sockets::default());
    }
    let list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    crate::recognition::LSOF_SPAWNS.fetch_add(1, Ordering::Relaxed);
    let out = Command::new("/usr/sbin/lsof")
        .args([
            "-nP",
            "-a",
            "-p",
            &list,
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
fn parse_sockets(rows: &str) -> Sockets {
    let port = |addr: &str| addr.rsplit(':').next().and_then(|n| n.parse::<u16>().ok());
    let mut out = Sockets::default();
    let mut pid = None;
    for line in rows.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().ok();
        } else if let (Some(name), Some(p)) = (line.strip_prefix('n'), pid) {
            if let Some((_, remote)) = name.split_once("->") {
                let here = remote.starts_with("127.") || remote.starts_with("[::1]");
                if let (true, Some(n)) = (here, port(remote)) {
                    out.dialing.push((p, n));
                }
            } else if let Some(n) = port(name).filter(|n| *n < EPHEMERAL) {
                out.listening.entry(p).or_default().insert(n);
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
        own_client: false,
        worker: true,
        age: 60,
    };
    const SERVER: Seen = Seen {
        listening: true,
        own_client: false,
        worker: false,
        age: 60,
    };
    /// A script's own server with its client connected at this look.
    const OWN: Seen = Seen {
        listening: true,
        own_client: true,
        worker: true,
        age: 60,
    };
    /// The same script between requests: not connected, but a worker is alive.
    const BETWEEN: Seen = Seen {
        listening: true,
        own_client: false,
        worker: true,
        age: 60,
    };
    const OLD: Seen = Seen {
        listening: false,
        own_client: false,
        worker: true,
        age: JOB_LIMIT,
    };
    /// The judgment after these looks, in order.
    fn after(looks: &[Option<Seen>]) -> Judged {
        let mut judged = None;
        for seen in looks {
            judged = Some(judge(judged, *seen));
        }
        judged.unwrap()
    }
    /// Every shell that was once judged wrong or argued over, with the answer agreed.
    /// A new mistake gets a row here and in the decision record this module names.
    #[test]
    fn known_cases() {
        let cases: &[(&str, &[Option<Seen>], bool)] = &[
            (
                "tests or a build: no port",
                &[Some(YOUNG), Some(YOUNG)],
                true,
            ),
            (
                "http.server or a relay left running",
                &[Some(SERVER), Some(SERVER)],
                false,
            ),
            (
                "a test that starts a server for a moment",
                &[Some(YOUNG), Some(SERVER), Some(YOUNG)],
                true,
            ),
            (
                "a script with its own relay, 2026-11-04",
                &[Some(OWN), Some(BETWEEN), Some(BETWEEN), Some(BETWEEN)],
                true,
            ),
            (
                "the same script, seen before its client connects",
                &[Some(BETWEEN), Some(BETWEEN), Some(OWN), Some(BETWEEN)],
                true,
            ),
            (
                "clients gone, a `wait` stuck on the relay, 2026-11-04",
                &[Some(OWN), Some(BETWEEN), Some(SERVER), Some(SERVER)],
                false,
            ),
            (
                "own server still up past the limit: does not spin for ever",
                &[
                    Some(OWN),
                    Some(Seen {
                        age: JOB_LIMIT,
                        ..OWN
                    }),
                ],
                false,
            ),
            (
                "polling loop first seen after the host restarted, 2026-11-05",
                &[Some(YOUNG)],
                true,
            ),
            (
                "tail -f: no port, past the limit",
                &[Some(YOUNG), Some(OLD)],
                false,
            ),
            (
                "the script ended and its agent has not woken for two looks",
                &[Some(OWN), None, None],
                false,
            ),
        ];
        for (name, looks, job) in cases {
            assert_eq!(after(looks).job, *job, "{name}");
        }
    }
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
        assert!(!judge(Some(judge(None, Some(YOUNG))), Some(OLD)).job);
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
        let got = parse_sockets(rows).listening;
        assert!(!got.contains_key(&100), "a renderer's own ephemeral port");
        assert!(got.contains_key(&200) && got.contains_key(&300), "{got:?}");
    }
    #[test]
    fn a_client_counts_only_from_another_process_of_the_tree_on_this_machine() {
        // Rows as lsof printed them for the Crouton script: the relay and its Python client.
        let rows = "p28079\nf5\nn*:8898\nf6\nn127.0.0.1:8898->127.0.0.1:49848\n\
                    p28382\nf4\nn127.0.0.1:49848->127.0.0.1:8898\n\
                    p500\nf4\nn[::1]:49900->[::1]:8898\n\
                    p600\nf4\nn10.0.0.2:49901->10.0.0.9:8898\n";
        let got = parse_sockets(rows);
        assert!(got.own_client(&[28079, 28382]), "its client in the tree");
        assert!(got.own_client(&[28079, 500]), "over IPv6 too");
        assert!(!got.own_client(&[28079]), "a client outside the tree");
        assert!(!got.own_client(&[28079, 600]), "another machine's 8898");
        assert!(
            !got.own_client(&[28382]),
            "a client whose server is elsewhere"
        );
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
    #[test]
    fn a_log_gives_back_the_command_of_each_background_task() {
        // Shaped as Claude Code 2.1.318 writes them, including a shell moved to the
        // background by hand and a task another session started.
        let rows = [
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"sleep 1; echo 'one'","run_in_background":true}}]}}"#,
            r#"{"type":"user","message":{"content":[{"tool_use_id":"t1","type":"tool_result","content":"Command running in background with ID: aaa. Output is being written to: /x"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"cargo test"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"tool_use_id":"t2","type":"tool_result","content":[{"type":"text","text":"Command was manually backgrounded with ID: bbb"}]}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"not asked for"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"tool_use_id":"t3","type":"tool_result","content":"Command running in background with ID: ccc."}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"grep aaa log"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"tool_use_id":"t4","type":"tool_result","content":"log: running in background with ID: aaa."}]}}"#,
            "not json with ID: aaa",
        ];
        let dir = std::env::temp_dir().join(format!("kl-shells-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("s.jsonl");
        std::fs::write(&log, rows.join("\n")).unwrap();
        let got = commands_in_log(&log, &["aaa".to_owned(), "bbb".to_owned()]).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            got,
            [
                ("aaa".to_owned(), "sleep 1; echo 'one'".to_owned()),
                ("bbb".to_owned(), "cargo test".to_owned()),
            ]
        );
    }
    /// A real script with its own fixed-port server and client, beside a server whose
    /// client is outside its tree. Undoing the own-client check fails this.
    #[test]
    fn a_real_script_using_its_own_port_differs_from_a_server_with_an_outside_client() {
        let bind = "import socket,subprocess,sys,time
for p in range({from}, {to}):
    s = socket.socket()
    try:
        s.bind((\"127.0.0.1\", p)); break
    except OSError:
        pass
s.listen()
print(p, flush=True)
{client}
c, _ = s.accept(); time.sleep(20)";
        let own_client = "subprocess.Popen([sys.executable, \"-c\", \"import socket,sys,time; c = socket.create_connection((sys.argv[1], int(sys.argv[2]))); time.sleep(20)\", \"127.0.0.1\", str(p)])";
        let script = bind
            .replace("{from}", "41300")
            .replace("{to}", "41400")
            .replace("{client}", own_client);
        let server = bind
            .replace("{from}", "41400")
            .replace("{to}", "41500")
            .replace("{client}", "");
        let own = format!(": own-8c1d; python3 -c '{script}'");
        let alone = format!(": srv-8c1d; python3 -c '{server}'");
        let spawn = |command: &str| {
            std::process::Command::new("/bin/sh")
                .args(["-c", &format!("eval '{}'", command.replace('\'', "'\\''"))])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        };
        let mut own_child = spawn(&own);
        let mut alone_child = spawn(&alone);
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(alone_child.stdout.as_mut().unwrap()),
            &mut line,
        )
        .unwrap();
        let port: u16 = line.trim().parse().unwrap();
        // This test process is the outside client.
        let _outside = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let tasks = vec![("o".to_owned(), own), ("a".to_owned(), alone)];
        let mut seen = HashMap::new();
        for _ in 0..50 {
            seen = look(&[(std::process::id(), tasks.clone())]).unwrap();
            if seen.get("o").is_some_and(|s| s.own_client) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let again = look(&[(std::process::id(), tasks.clone())]).unwrap();
        own_child.kill().unwrap();
        alone_child.kill().unwrap();
        let _ = own_child.wait();
        let _ = alone_child.wait();
        assert!(seen["o"].listening && seen["o"].own_client, "{seen:?}");
        assert!(again["a"].listening, "{again:?}");
        assert!(!again["a"].own_client, "{again:?}");
        assert!(
            seen["o"].worker,
            "its client is a leaf listening on nothing"
        );
        assert!(!again["a"].worker, "the server is the only leaf");
    }
}
