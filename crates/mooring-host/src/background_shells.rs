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
use crate::shell_match::runs;
use mooring_platform::inspect;
use std::collections::{HashMap, HashSet};
use std::io;
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

/// Which rule a judgment came from, for `mooring explain` and the host log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    NoPort,
    Listening,
    ListeningOnce,
    OwnClient,
    TooOld,
    Ended,
    NotFound,
    NotFoundTooLong,
}
impl Why {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoPort => "进程在，没有固定端口",
            Self::Listening => "监听固定端口",
            Self::ListeningOnce => "刚看到固定端口，连续两次才算服务",
            Self::OwnClient => "脚本自己连自己的端口",
            Self::TooOld => "已跑满 2 小时",
            Self::Ended => "进程已结束，等 Agent 醒来",
            Self::NotFound => "Claude 列着它、没说结束，但找不到进程，最多算 2 小时",
            Self::NotFoundTooLong => "Claude 列着它，但 2 小时都没找到进程",
        }
    }
}
/// Looks at a shell never found before it stops counting as a job: about [`JOB_LIMIT`].
/// Counted here because a process that cannot be found has no start time to read.
const UNFOUND_LOOKS: u32 = (JOB_LIMIT / EVERY) as u32;

/// The judgment carried between looks. A shell changes sides only after two looks in a
/// row disagree with it, so a test that starts a server for a moment stays a job. The
/// exception is its own client: one sighting settles it at once, and it stays settled
/// while a worker is left, because such a connection comes and goes between requests.
/// Once only the server and the shells waiting on it remain, the work is over.
///
/// A shell Claude lists but no process was ever found for is taken at Claude's word, up
/// to the limit. One found and then gone has ended, which is a different thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Judged {
    pub job: bool,
    pub why: Why,
    against: u8,
    own_client: bool,
    found: bool,
    unfound: u32,
}
pub fn judge(before: Option<Judged>, seen: Option<Seen>) -> Judged {
    let was_found = before.filter(|b| b.found);
    if seen.is_none() && was_found.is_none() {
        let unfound = before.map_or(0, |b| b.unfound) + 1;
        let job = unfound < UNFOUND_LOOKS;
        return Judged {
            job,
            why: if job {
                Why::NotFound
            } else {
                Why::NotFoundTooLong
            },
            against: 0,
            own_client: false,
            found: false,
            unfound,
        };
    }
    let before = was_found;
    let settled = |job: bool, why: Why, against: u8, own_client: bool| Judged {
        job,
        why,
        against,
        own_client,
        found: true,
        unfound: 0,
    };
    // Too old to be a job, whatever else it shows: no second look needed.
    if seen.is_some_and(|s| s.age >= JOB_LIMIT) {
        return settled(false, Why::TooOld, 0, false);
    }
    if seen.is_some_and(|s| s.own_client) {
        return settled(true, Why::OwnClient, 0, true);
    }
    let own_client = before.is_some_and(|b| b.own_client);
    let now = seen.is_some_and(|s| !s.listening || (own_client && s.worker));
    let (job, against) = match before {
        None => (now, 0),
        Some(b) if b.job == now => (b.job, 0),
        Some(b) if b.against >= 1 => (now, 0),
        Some(b) => (b.job, b.against + 1),
    };
    let why = match seen {
        None => Why::Ended,
        Some(s) if s.listening && own_client && s.worker => Why::OwnClient,
        Some(s) if s.listening && job => Why::ListeningOnce,
        Some(s) if s.listening => Why::Listening,
        Some(_) => Why::NoPort,
    };
    settled(job, why, against, own_client)
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
                let argv = inspect::argv(pid).ok()?;
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
    seen_all(found, now)
}
/// Looks at processes already found, `(id, pid)`.
pub fn look_at(pids: &[(String, u32)]) -> io::Result<HashMap<String, Seen>> {
    let found = pids
        .iter()
        .map(|(id, pid)| (id.clone(), *pid, tree(*pid)))
        .collect();
    seen_all(found, mooring_core::content::now())
}
/// How long a process has run.
pub fn age(pid: u32) -> u64 {
    let now = mooring_core::content::now();
    inspect::started(pid).map_or(0, |at| now.saturating_sub(at))
}
fn seen_all(found: Vec<(String, u32, Vec<u32>)>, now: u64) -> io::Result<HashMap<String, Seen>> {
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
                age: inspect::started(pid).map_or(0, |at| now.saturating_sub(at)),
            };
            (id, seen)
        })
        .collect())
}

pub(crate) fn contains(line: &[u8], needle: &[u8]) -> bool {
    line.windows(needle.len()).any(|w| w == needle)
}

pub(crate) fn children(pid: u32) -> Vec<u32> {
    inspect::children(pid).unwrap_or_default()
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
    crate::recognition::LSOF_SPAWNS.fetch_add(1, Ordering::Relaxed);
    Ok(chosen(inspect::tcp_sockets(pids)?))
}
/// Leaves out the ports the system handed out: only a chosen one says "server".
fn chosen(found: inspect::TcpSockets) -> Sockets {
    let mut out = Sockets {
        dialing: found.connected,
        ..Sockets::default()
    };
    for (pid, port) in found.listening {
        if port < EPHEMERAL {
            out.listening.entry(pid).or_default().insert(port);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    /// zsh, which Agents' shells mostly are, where it is installed.
    fn a_login_shell() -> &'static str {
        if std::path::Path::new("/bin/zsh").exists() {
            "/bin/zsh"
        } else {
            "/bin/sh"
        }
    }
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
                "a loop with a quote in its command, once its shell is found, 2026-11-06",
                &[Some(YOUNG), Some(YOUNG)],
                true,
            ),
            (
                "a server Codex left running, found under its daemon, 2026-11-06",
                &[Some(SERVER)],
                false,
            ),
            (
                "listed by Claude, its process never found: still a job, 2026-11-06",
                &[None, None, None],
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
    fn an_old_shell_is_no_job() {
        assert!(!judge(Some(judge(None, Some(YOUNG))), Some(OLD)).job);
        let job = judge(None, Some(YOUNG));
        assert!(
            judge(Some(job), None).job,
            "a job that just ended stays one until its agent wakes"
        );
    }
    /// Claude says it runs, so a shell never found counts as running, but not for ever: a
    /// server that goes unfound must not spin past the limit every job has.
    #[test]
    fn a_shell_never_found_is_a_job_until_the_limit() {
        let first = judge(None, None);
        assert!(first.job && first.why == Why::NotFound, "{first:?}");
        let mut judged = first;
        for _ in 2..UNFOUND_LOOKS {
            judged = judge(Some(judged), None);
        }
        assert!(judged.job, "one look before the limit");
        let over = judge(Some(judged), None);
        assert!(!over.job && over.why == Why::NotFoundTooLong, "{over:?}");
        assert!(judge(Some(judged), Some(SERVER)).why == Why::Listening);
        assert!(judge(Some(judged), Some(YOUNG)).job, "found after all");
    }
    /// `exec` leaves no trace of the command in the shell: found by what it ran.
    #[test]
    fn a_real_process_an_exec_became_is_found() {
        let command = "cd /tmp && exec /bin/sleep 31.7";
        let mut child = std::process::Command::new(a_login_shell())
            .args(["-c", command])
            .spawn()
            .unwrap();
        let tasks = vec![("x".to_owned(), command.to_owned())];
        let mut seen = HashMap::new();
        for _ in 0..50 {
            seen = look(&[(std::process::id(), tasks.clone())]).unwrap();
            if seen.contains_key("x") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        child.kill().unwrap();
        let _ = child.wait();
        assert!(seen.contains_key("x"), "{seen:?}");
    }
    #[test]
    fn only_a_chosen_port_makes_a_server() {
        let got = chosen(inspect::TcpSockets {
            listening: vec![(100, 51174), (200, 8000), (300, 5173), (300, 60000)],
            connected: vec![],
        })
        .listening;
        assert!(!got.contains_key(&100), "a renderer's own ephemeral port");
        assert!(got.contains_key(&200) && got.contains_key(&300), "{got:?}");
    }
    #[test]
    fn a_client_counts_only_from_another_process_of_the_tree_on_this_machine() {
        // As the Crouton script's relay and its Python client looked. Process 600 connects
        // to another machine's 8898, which `tcp_sockets` does not list.
        let got = chosen(inspect::TcpSockets {
            listening: vec![(28079, 8898)],
            connected: vec![(28079, 49848), (28382, 8898), (500, 8898)],
        });
        assert!(got.own_client(&[28079, 28382]), "its client in the tree");
        assert!(got.own_client(&[28079, 500]), "over IPv6 too");
        assert!(!got.own_client(&[28079]), "a client outside the tree");
        assert!(!got.own_client(&[28079, 600]), "another machine's 8898");
        assert!(
            !got.own_client(&[28382]),
            "a client whose server is elsewhere"
        );
    }
    /// The whole chain on a real process: a shell quoting that way is found and is a job.
    #[test]
    fn a_real_shell_quoting_a_quote_the_new_way_is_found() {
        let command = "perl -e 'sleep 30' # quote-9b2e";
        let quoted = command.replace('\'', r#"'"'"'"#);
        let mut child = std::process::Command::new(a_login_shell())
            .args(["-c", &format!("eval '{quoted}' < /dev/null && true")])
            .spawn()
            .unwrap();
        let tasks = vec![("q".to_owned(), command.to_owned())];
        let mut seen = HashMap::new();
        for _ in 0..50 {
            seen = look(&[(std::process::id(), tasks.clone())]).unwrap();
            if seen.contains_key("q") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        child.kill().unwrap();
        let _ = child.wait();
        assert!(seen.contains_key("q"), "{seen:?}");
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
