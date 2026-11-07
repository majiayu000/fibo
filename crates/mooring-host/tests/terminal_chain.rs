//! The whole way through on whatever system this runs on: a host starts, opens this system's
//! own shell in a terminal, and the command line lists it, reads its screen and types into it.
#[cfg_attr(unix, path = "support/tied.rs")]
#[cfg_attr(windows, path = "support/tied_windows.rs")]
mod tied;
use mooring_core::{agent::Agent, model::*, protocol::*};
use mooring_platform::{fs, inspect, local::Stream, shell};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use tied::{SpawnTied, Tied};

const PATIENCE: Duration = Duration::from_secs(60);

struct Host {
    child: Tied,
    root: PathBuf,
}
impl Drop for Host {
    fn drop(&mut self) {
        self.child.kill_tree();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn start_host() -> Host {
    let root = std::env::temp_dir().join(format!("kl-chain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    fs::create_private_dir(&root).unwrap();
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::create_dir_all(root.join("work")).unwrap();
    let child = cli(&root)
        .args(["serve", "--runtime"])
        .arg(&root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn_tied()
        .unwrap();
    Host { child, root }
}

/// The command line, with the shell's profile and the Agents' files kept out of this account's.
fn cli(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mooring"));
    cmd.env("HOME", root.join("home"))
        .env("USERPROFILE", root.join("home"))
        .env("MOORING_RUNTIME", root);
    cmd
}

fn run(root: &Path, args: &[&str]) -> Output {
    cli(root).args(args).output().unwrap()
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn rpc(socket: &Path, request: &Request) -> Reply {
    let mut stream = Stream::connect(socket).unwrap();
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    receive::<Reply>(&mut stream).unwrap();
    send(&mut stream, CONTROL, request).unwrap();
    receive::<Reply>(&mut stream).unwrap()
}

/// Asks until `done` says so, and says how long that took.
fn until(what: &str, mut done: impl FnMut() -> bool) -> Duration {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < PATIENCE, "{what}");
        std::thread::sleep(Duration::from_millis(50));
    }
    start.elapsed()
}

#[test]
fn a_real_terminal_is_listed_read_and_typed_into() {
    let host = start_host();
    let root = host.root.clone();
    let socket = root.join("workbench.sock");
    let work = root.join("work");
    let cwd = work.to_string_lossy().into_owned();
    until("host never answered", || Stream::connect(&socket).is_ok());
    let space = "s".to_owned();
    rpc(
        &socket,
        &Request::Edit {
            request: None,
            edit: Edit::AddSpace(Space {
                id: space.clone(),
                name: "Tests".into(),
                cwd: cwd.clone(),
            }),
        },
    );
    let opened = Instant::now();
    let reply = rpc(
        &socket,
        &Request::Launch {
            id: "t".into(),
            space: space.clone(),
            argv: shell::stay(&shell::interactive()),
            cwd: cwd.clone(),
            cols: 120,
            rows: 30,
            scheme: Default::default(),
            colors: None,
        },
    );
    assert!(!matches!(reply, Reply::Error(_)), "{reply:?}");
    rpc(
        &socket,
        &Request::Edit {
            request: None,
            edit: Edit::CreateTab {
                tab: Tab {
                    id: "tab".into(),
                    space,
                    default_task: None,
                    title: String::new(),
                    layout: Some(mooring_core::layout::Layout::Leaf("t".into())),
                },
            },
        },
    );

    let listed = text(&run(&root, &["list", "--json"]));
    assert!(listed.contains("\"t\""), "{listed}");

    let first_screen = until("the shell never showed anything", || {
        !text(&run(&root, &["read", "t"])).trim().is_empty()
    });
    eprintln!("shell up after {:?}", opened.elapsed());
    eprintln!("first screen seen after {first_screen:?} of asking");

    let typed = Instant::now();
    let sent = run(&root, &["send", "t", "echo mark > sent-mark", "--enter"]);
    assert!(sent.status.success(), "{sent:?}");
    until("what was typed never ran", || {
        work.join("sent-mark").exists()
    });
    eprintln!("typed to ran: {:?}", typed.elapsed());
    until("the screen never showed what was typed", || {
        text(&run(&root, &["read", "t"])).contains("sent-mark")
    });

    let split = run(&root, &["split", "t", "--", "echo", "two words"]);
    assert!(split.status.success(), "{split:?}");
    let second = text(&split).trim().to_owned();
    until("the split never ran its command", || {
        text(&run(&root, &["read", &second])).contains("two words")
    });

    let closed = run(&root, &["close", &second]);
    assert!(closed.status.success(), "{closed:?}");
    let closed = run(&root, &["close", "t"]);
    assert!(closed.status.success(), "{closed:?}");
    until("the terminals never went", || {
        !text(&run(&root, &["list", "--json"])).contains("\"t\"")
    });
}

/// Started in a terminal by the measurement below; does nothing unless it finds its file.
#[test]
#[ignore = "started by how_much_a_host_costs"]
fn helper_prints_a_lot() {
    use std::io::Write;
    if !Path::new("print-here").exists() {
        return;
    }
    let mut line = vec![b'x'; 79];
    line.push(b'\n');
    let mut out = std::io::stdout().lock();
    for _ in 0..(PRINTED / line.len()) {
        out.write_all(&line).unwrap();
    }
    out.flush().unwrap();
    std::fs::write("printed", "").unwrap();
    std::thread::sleep(Duration::from_secs(300));
}
const PRINTED: usize = 50 * 1024 * 1024;

fn megabytes(bytes: u64) -> u64 {
    bytes / 1024 / 1024
}

/// How long a key takes to come back on the screen, as a window attached to the terminal sees it.
fn key_to_screen(socket: &Path, terminal: &str, when: &str) {
    let mut stream = Stream::connect(socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    receive::<Reply>(&mut stream).unwrap();
    send(
        &mut stream,
        CONTROL,
        &Request::Attach {
            id: terminal.into(),
            takeover: true,
            watch: false,
        },
    )
    .unwrap();
    receive::<Reply>(&mut stream).unwrap();
    while read_frame(&mut stream).unwrap().0 != READY {}
    // The shell's first redraw on being attached is not an answer to a key.
    std::thread::sleep(Duration::from_millis(300));
    let mut took = vec![];
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(30));
        let typed = Instant::now();
        send_input(&mut stream, b"x", false).unwrap();
        loop {
            let (kind, bytes) = read_frame(&mut stream).unwrap();
            if kind == OUTPUT && bytes.contains(&b'x') {
                break;
            }
        }
        took.push(typed.elapsed());
    }
    took.sort();
    eprintln!(
        "key to screen, {when}: median {:?}, slowest {:?}",
        took[took.len() / 2],
        took[took.len() - 1]
    );
}

/// By hand, in a release build, and the machine otherwise quiet:
/// `cargo test --release -p mooring-host --test terminal_chain how_much -- --ignored --nocapture`
#[test]
#[ignore = "measures, and takes a minute"]
fn how_much_a_host_costs() {
    const TERMINALS: usize = 20;
    let host = start_host();
    let pid = host.child.id();
    let root = host.root.clone();
    let socket = root.join("workbench.sock");
    let work = root.join("work");
    let cwd = work.to_string_lossy().into_owned();
    until("host never answered", || Stream::connect(&socket).is_ok());
    rpc(
        &socket,
        &Request::Edit {
            request: None,
            edit: Edit::AddSpace(Space {
                id: "s".into(),
                name: "Tests".into(),
                cwd: cwd.clone(),
            }),
        },
    );
    let launch = |id: String, argv: Vec<String>| {
        let reply = rpc(
            &socket,
            &Request::Launch {
                id,
                space: "s".into(),
                argv,
                cwd: cwd.clone(),
                cols: 120,
                rows: 30,
                scheme: Default::default(),
                colors: None,
            },
        );
        assert!(!matches!(reply, Reply::Error(_)), "{reply:?}");
    };
    let consoles = || {
        inspect::all_pids()
            .into_iter()
            .filter(|p| inspect::name(*p).is_some_and(|n| n == "conhost" || n == "OpenConsole"))
            .collect::<Vec<_>>()
    };
    let consoles_before = consoles();
    eprintln!(
        "empty host: {} MB",
        megabytes(inspect::memory(pid).unwrap_or(0))
    );

    let opening = Instant::now();
    for i in 0..TERMINALS {
        launch(format!("t{i}"), shell::stay(&shell::interactive()));
    }
    for i in 0..TERMINALS {
        until("a shell never showed anything", || {
            !text(&run(&root, &["read", &format!("t{i}")]))
                .trim()
                .is_empty()
        });
    }
    eprintln!(
        "{TERMINALS} shells open and showing: {:?}",
        opening.elapsed()
    );
    eprintln!(
        "host with them: {} MB",
        megabytes(inspect::memory(pid).unwrap_or(0))
    );
    let new_consoles: Vec<_> = consoles()
        .into_iter()
        .filter(|p| !consoles_before.contains(p))
        .collect();
    let console_memory: u64 = new_consoles
        .iter()
        .filter_map(|p| inspect::memory(*p))
        .sum();
    eprintln!(
        "console processes: {}, {} MB together",
        new_consoles.len(),
        megabytes(console_memory)
    );

    // Long enough for the shells to finish starting before the clock starts.
    std::thread::sleep(Duration::from_secs(10));
    let cpu = |pid| inspect::usage(pid).map_or(Duration::ZERO, |u| u.cpu);
    let idle_from = cpu(pid);
    let idle = Duration::from_secs(30);
    std::thread::sleep(idle);
    let spent = cpu(pid) - idle_from;
    eprintln!(
        "idle for {idle:?}: {spent:?} of CPU, {:.2}% of one core",
        spent.as_secs_f64() / idle.as_secs_f64() * 100.
    );

    key_to_screen(&socket, "t2", "with them open");

    let typed = Instant::now();
    let sent = run(&root, &["send", "t0", "echo mark > sent-mark", "--enter"]);
    assert!(sent.status.success(), "{sent:?}");
    until("what was typed never ran", || {
        work.join("sent-mark").exists()
    });
    eprintln!("typed to ran, with {TERMINALS} open: {:?}", typed.elapsed());

    std::fs::write(work.join("print-here"), "").unwrap();
    let cpu_from = cpu(pid);
    let printing = Instant::now();
    launch(
        "printer".into(),
        vec![
            std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            "helper_prints_a_lot".into(),
            "--exact".into(),
            "--ignored".into(),
            "--nocapture".into(),
        ],
    );
    let start = Instant::now();
    while !work.join("printed").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "never finished; its screen: {}",
            text(&run(&root, &["read", "printer", "--lines", "5"]))
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let took = printing.elapsed();
    eprintln!(
        "{} MB printed through a terminal: {took:?}, {:.0} MB/s, host CPU {:?}, host {} MB after",
        PRINTED / 1024 / 1024,
        PRINTED as f64 / 1024. / 1024. / took.as_secs_f64(),
        cpu(pid) - cpu_from,
        megabytes(inspect::memory(pid).unwrap_or(0))
    );
    key_to_screen(&socket, "t3", "after that");
    let typed = Instant::now();
    let sent = run(&root, &["send", "t1", "echo mark > after-mark", "--enter"]);
    assert!(sent.status.success(), "{sent:?}");
    until("what was typed after never ran", || {
        work.join("after-mark").exists()
    });
    eprintln!("typed to ran, after that: {:?}", typed.elapsed());
    for i in 0..TERMINALS {
        let _ = run(&root, &["close", &format!("t{i}")]);
    }
    let _ = run(&root, &["close", "printer"]);
}

/// What the system's terminal itself can carry with nothing of the host in the way: what
/// [`how_much_a_host_costs`] prints is to be read against this.
#[test]
#[ignore = "measures"]
fn how_fast_a_bare_terminal_is() {
    use mooring_platform::terminal::{spawn, Read, Spec};
    let work = std::env::temp_dir().join(format!("kl-bare-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("print-here"), "").unwrap();
    let argv = [
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        "helper_prints_a_lot".to_owned(),
        "--exact".to_owned(),
        "--ignored".to_owned(),
        "--nocapture".to_owned(),
    ];
    let start = Instant::now();
    let (mut child, master) = spawn(&Spec {
        argv: &argv,
        cwd: &work,
        env: shell::login_env(std::env::vars_os()),
        cols: 120,
        rows: 30,
    })
    .unwrap();
    let mut reader = master.reader().unwrap();
    let mut buf = vec![0u8; 64 * 1024];
    let mut bytes = 0usize;
    while start.elapsed() < Duration::from_secs(120) {
        match reader.next(&mut buf, Some(Duration::from_millis(200))) {
            Ok(Read::Data(n)) => bytes += n,
            Ok(Read::Quiet) if work.join("printed").exists() => break,
            Ok(Read::Quiet) => (),
            Ok(Read::Closed) | Err(_) => break,
        }
    }
    let took = start.elapsed();
    let _ = child.kill();
    assert!(
        work.join("printed").exists(),
        "never finished, {bytes} bytes"
    );
    eprintln!(
        "{} MB printed, {} MB came out, nothing else reading: {took:?}, {:.0} MB/s",
        PRINTED / 1024 / 1024,
        bytes / 1024 / 1024,
        PRINTED as f64 / 1024. / 1024. / took.as_secs_f64(),
    );
    let _ = std::fs::remove_dir_all(&work);
}

/// The real CLIs, installed, in a terminal of this system's own shell, with a home of their own
/// and no credentials: what the host can tell of them before anyone signs in. By hand, with
/// `claude` and `codex` on PATH; `MOORING_REAL_CLI_PREFIX` goes before each, in the shell's own
/// language, for what must be set first (their updaters, which would change the real install):
/// `cargo test --release -p mooring-host --test terminal_chain real_clis -- --ignored --nocapture`
#[test]
#[ignore = "needs the real CLIs installed"]
fn real_clis_are_recognized_in_this_system_s_shell() {
    let host = start_host();
    let pid = host.child.id();
    let root = host.root.clone();
    let socket = root.join("workbench.sock");
    let work = root.join("work");
    let cwd = work.to_string_lossy().into_owned();
    let home = root.join("home");
    // A home of their own: nothing here says who anyone is.
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(
        home.join(".claude/settings.json"),
        r#"{"env":{"DISABLE_AUTOUPDATER":"1"}}"#,
    )
    .unwrap();
    // First-run setup checks the network and leaves when it is not there; past it, a CLI stays
    // at its sign-in as it does on a machine in use.
    std::fs::write(
        home.join(".claude.json"),
        r#"{"hasCompletedOnboarding":true,"autoUpdates":false}"#,
    )
    .unwrap();
    until("host never answered", || Stream::connect(&socket).is_ok());
    rpc(
        &socket,
        &Request::Edit {
            request: None,
            edit: Edit::AddSpace(Space {
                id: "s".into(),
                name: "Tests".into(),
                cwd: cwd.clone(),
            }),
        },
    );
    let prefix = std::env::var("MOORING_REAL_CLI_PREFIX").unwrap_or_default();
    let mut seen = vec![];
    let mut unknown = vec![];
    let mut logs = vec![];
    for (id, cli, agent) in [("c", "claude", Agent::Claude), ("x", "codex", Agent::Codex)] {
        let shell = shell::interactive();
        let reply = rpc(
            &socket,
            &Request::Launch {
                id: id.into(),
                space: "s".into(),
                // What was asked stays on the screen, as it does once someone has typed it.
                argv: shell::run_then_stay(
                    &shell,
                    &format!("{prefix}echo '{REAL_PROMPT}'; {cli}"),
                    None,
                ),
                cwd: cwd.clone(),
                cols: 120,
                rows: 30,
                scheme: Default::default(),
                colors: None,
            },
        );
        assert!(!matches!(reply, Reply::Error(_)), "{reply:?}");
        rpc(
            &socket,
            &Request::Edit {
                request: None,
                edit: Edit::CreateTab {
                    tab: Tab {
                        id: format!("tab-{id}"),
                        space: "s".into(),
                        default_task: None,
                        title: String::new(),
                        layout: Some(mooring_core::layout::Layout::Leaf(id.into())),
                    },
                },
            },
        );
        // The host looks at processes when a log or a hook gives it reason to; a CLI nobody has
        // signed in to writes no log, so this is the one it would have written.
        let appeared = until("the CLI never started", || {
            let Reply::Workspace(w) = rpc(&socket, &Request::Workspace) else {
                return false;
            };
            mooring_host::recognition::processes(&w)
                .unwrap()
                .iter()
                .any(|p| p.runtime == Some(agent))
        });
        eprintln!("{cli}: process there after {appeared:?}");
        std::thread::sleep(Duration::from_secs(2));
        let log = write_log(agent, &home, &cwd);
        logs.push((agent, log.clone()));
        let waited = Instant::now();
        let mut known = false;
        while !known && waited.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(100));
            // Named while it has no conversation; once it has, the conversation says.
            if let Reply::Recognition(status) = rpc(&socket, &Request::RecognitionStatus) {
                known = status.terminal_agents.get(id) == Some(&agent);
            }
            if let Reply::Workspace(w) = rpc(&socket, &Request::Workspace) {
                known |= w.current_agent(id).is_some_and(|s| s.runtime == agent);
            }
        }
        eprintln!(
            "{cli}: host says it is here={known}, {:?} after its log",
            waited.elapsed()
        );
        let Reply::Workspace(w) = rpc(&socket, &Request::Workspace) else {
            panic!("no workspace");
        };
        let probing = Instant::now();
        let found = mooring_host::recognition::processes(&w).unwrap();
        eprintln!(
            "{cli}: {} processes looked at in {:?}",
            found.len(),
            probing.elapsed()
        );
        for p in found.iter().filter(|p| p.parent != 0) {
            eprintln!(
                "{cli}: pid {} parent {} name {:?} runtime {:?} argv {:?}",
                p.pid,
                p.parent,
                inspect::name(p.pid),
                p.runtime,
                inspect::argv(p.pid).unwrap_or_default(),
            );
            seen.push(p.pid);
        }
        std::thread::sleep(Duration::from_secs(3));
        eprintln!(
            "{cli}: screen: {}",
            text(&run(&root, &["read", id, "--lines", "12"]))
        );
        unknown.extend((!known).then_some(cli));
    }
    let Reply::Workspace(w) = rpc(&socket, &Request::Workspace) else {
        panic!("no workspace");
    };
    for id in ["c", "x"] {
        let bound = w.current_agent(id).map(|s| s.cli_session_id.clone());
        eprintln!("terminal {id}: its conversation by the screen: {bound:?}");
    }
    let cpu = |pid| inspect::usage(pid).map_or(Duration::ZERO, |u| u.cpu);
    let quiet = Duration::from_secs(30);
    let from = cpu(pid);
    std::thread::sleep(quiet);
    let spent = cpu(pid) - from;
    eprintln!(
        "both running and following, nothing written, {quiet:?}: {spent:?} of host CPU, host {} MB",
        megabytes(inspect::memory(pid).unwrap_or(0))
    );
    let from = cpu(pid);
    let writing = Instant::now();
    while writing.elapsed() < quiet {
        for (agent, log) in &logs {
            use std::io::Write;
            let line = log_line(*agent, "still working on it");
            let mut file = std::fs::OpenOptions::new().append(true).open(log).unwrap();
            writeln!(file, "{line}").unwrap();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let spent = cpu(pid) - from;
    eprintln!(
        "ten lines a second to each log, {quiet:?}: {spent:?} of host CPU, host {} MB",
        megabytes(inspect::memory(pid).unwrap_or(0))
    );
    for id in ["c", "x"] {
        let closed = run(&root, &["close", id]);
        assert!(closed.status.success(), "{closed:?}");
    }
    until("what ran in the terminals outlived them", || {
        seen.iter()
            .all(|p| !mooring_platform::process::exists(*p as i32))
    });
    assert!(unknown.is_empty(), "never recognized: {unknown:?}");
}

const REAL_PROMPT: &str = "please check the rollback steps in the release script";

/// A line of a log as the CLI writes it.
fn log_line(agent: Agent, said: &str) -> String {
    let now = mooring_core::content::now();
    match agent {
        Agent::Claude => serde_json::json!({"type":"assistant","timestamp":now,
            "message":{"role":"assistant","content":[{"type":"text","text":said}]}}),
        _ => serde_json::json!({"timestamp":now,"type":"event_msg",
            "payload":{"type":"agent_message","message":said}}),
    }
    .to_string()
}

/// The log a CLI starts for a conversation, under its own home.
fn write_log(agent: Agent, home: &Path, cwd: &str) -> PathBuf {
    let id = uuid::Uuid::new_v4().to_string();
    let now = mooring_core::content::now();
    let (dir, name, lines) = match agent {
        Agent::Claude => (
            home.join(".claude/projects/work"),
            format!("{id}.jsonl"),
            vec![
                serde_json::json!({"type":"user","sessionId":id,"uuid":"u0","cwd":cwd,
                "entrypoint":"cli","userType":"external","timestamp":now,
                "message":{"role":"user","content":REAL_PROMPT}}),
            ],
        ),
        _ => (
            home.join(".codex/sessions"),
            format!("rollout-{id}.jsonl"),
            vec![
                serde_json::json!({"type":"session_meta","payload":{"id":id,"cwd":cwd}}),
                serde_json::json!({"timestamp":now,"type":"event_msg",
                    "payload":{"type":"user_message","message":REAL_PROMPT}}),
            ],
        ),
    };
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&path, text).unwrap();
    path
}
