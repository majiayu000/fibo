//! What a host costs on this system, under the load Agents really make rather than a flood. By
//! hand, in a release build, on a quiet machine:
//! `cargo test --release -p fibo-host --test system_cost how_much -- --ignored --nocapture`
#[cfg_attr(unix, path = "support/tied.rs")]
#[cfg_attr(windows, path = "support/tied_windows.rs")]
mod tied;
use fibo_core::{model::*, protocol::*};
use fibo_platform::{fs, inspect, local::Stream, shell};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tied::{SpawnTied, Tied};

const TERMINALS: usize = 20;
const WRITERS: usize = 4;
const WRITE_SECONDS: u64 = 30;

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

fn cli(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fibo"));
    cmd.env("HOME", root.join("home"))
        .env("USERPROFILE", root.join("home"))
        .env("MOORING_RUNTIME", root);
    cmd
}

/// A long-used install: this many old conversations, spread over 20 projects.
const OLD_LOGS: usize = 2000;
const LINES_EACH: usize = 40;

fn log_line(i: usize) -> String {
    format!(
        "{{\"type\":\"assistant\",\"sessionId\":\"s\",\"cwd\":\"/w\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"{i} {}\"}}]}}}}\n",
        "working on it ".repeat(20)
    )
}

fn start_host() -> Host {
    let root = std::env::temp_dir().join(format!("kl-cost-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    fs::create_private_dir(&root).unwrap();
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::create_dir_all(root.join("work")).unwrap();
    let projects = root.join("home/.claude/projects");
    let body: String = (0..LINES_EACH).map(log_line).collect();
    let writing = Instant::now();
    for i in 0..OLD_LOGS {
        let dir = projects.join(format!("p{}", i % 20));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("old-{i}.jsonl")), &body).unwrap();
    }
    eprintln!(
        "{OLD_LOGS} old logs, {} MB, written in {:?}",
        OLD_LOGS * body.len() / 1024 / 1024,
        writing.elapsed()
    );
    let log = std::fs::File::create(std::env::temp_dir().join("kl-cost-host.log")).unwrap();
    let child = cli(&root)
        .args(["serve", "--runtime"])
        .arg(&root)
        .stdout(Stdio::null())
        .stderr(log)
        .spawn_tied()
        .unwrap();
    Host { child, root }
}

fn rpc(socket: &Path, request: &Request) -> Reply {
    let mut stream = Stream::connect(socket).unwrap();
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    receive::<Reply>(&mut stream).unwrap();
    send(&mut stream, CONTROL, request).unwrap();
    receive::<Reply>(&mut stream).unwrap()
}

fn timed(times: usize, mut what: impl FnMut()) -> (Duration, Duration) {
    let mut took: Vec<_> = (0..times)
        .map(|_| {
            let start = Instant::now();
            what();
            start.elapsed()
        })
        .collect();
    took.sort();
    (took[took.len() / 2], took[took.len() - 1])
}

fn cpu(pid: u32) -> Duration {
    inspect::usage(pid).map_or(Duration::ZERO, |u| u.cpu)
}

fn consoles() -> Vec<u32> {
    inspect::all_pids()
        .into_iter()
        .filter(|p| inspect::name(*p).is_some_and(|n| n == "conhost" || n == "OpenConsole"))
        .collect()
}

/// Started in a terminal below: prints as an Agent does, by the mode its file names.
#[test]
#[ignore = "started by how_much_a_host_costs"]
fn helper_writes_like_an_agent() {
    use std::io::Write;
    let Ok(mode) = std::fs::read_to_string("write-mode") else {
        return;
    };
    let mut out = std::io::stdout().lock();
    let start = Instant::now();
    let mut tick = 0u64;
    while start.elapsed() < Duration::from_secs(WRITE_SECONDS) {
        match mode.as_str() {
            // A reply streaming out: a line at a time, 100 lines a second.
            "lines" => writeln!(out, "{tick:>8} {}", "x".repeat(70)).unwrap(),
            // A full-screen interface redrawing itself: 30 lines, 10 times a second.
            _ => {
                let mut frame = String::from("\x1b[H");
                for row in 0..30 {
                    frame.push_str(&format!(
                        "\x1b[38;5;{}m{row:>3} {tick:>8} {}\x1b[K\r\n",
                        row % 200,
                        "y".repeat(90)
                    ));
                }
                out.write_all(frame.as_bytes()).unwrap();
            }
        }
        out.flush().unwrap();
        tick += 1;
        let every = if mode == "lines" { 10 } else { 100 };
        let next = Duration::from_millis(tick * every);
        std::thread::sleep(next.saturating_sub(start.elapsed()));
    }
    std::thread::sleep(Duration::from_secs(300));
}

fn write_round(host: &Host, socket: &Path, cwd: &str, mode: &str) {
    let pid = host.child.id();
    std::fs::write(host.root.join("work/write-mode"), mode).unwrap();
    let before = consoles();
    for i in 0..WRITERS {
        launch(
            socket,
            cwd,
            format!("{mode}{i}"),
            vec![
                std::env::current_exe()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                "helper_writes_like_an_agent".into(),
                "--exact".into(),
                "--ignored".into(),
                "--nocapture".into(),
            ],
        );
    }
    std::thread::sleep(Duration::from_secs(3));
    let mine: Vec<u32> = consoles()
        .into_iter()
        .filter(|p| !before.contains(p))
        .collect();
    let (host_from, console_from): (Duration, Duration) =
        (cpu(pid), mine.iter().map(|p| cpu(*p)).sum());
    let window = Duration::from_secs(20);
    std::thread::sleep(window);
    let spent = cpu(pid) - host_from;
    let console: Duration = mine.iter().map(|p| cpu(*p)).sum::<Duration>() - console_from;
    eprintln!(
        "{WRITERS} writers, {mode}: host {:.2}% of a core, their {} consoles {:.2}%",
        spent.as_secs_f64() / window.as_secs_f64() * 100.,
        mine.len(),
        console.as_secs_f64() / window.as_secs_f64() * 100.
    );
    for i in 0..WRITERS {
        let _ = cli(&host.root)
            .args(["close", &format!("{mode}{i}")])
            .output();
    }
}

/// Four conversations appending ten lines a second, as an Agent writes its log.
fn log_round(host: &Host, keep_open: bool) {
    use std::io::Write;
    let pid = host.child.id();
    let dir = host.root.join("home/.claude/projects/p0");
    let paths: Vec<_> = (0..WRITERS)
        .map(|i| dir.join(format!("live-{keep_open}-{i}.jsonl")))
        .collect();
    let mut open: Vec<_> = paths
        .iter()
        .map(|p| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .unwrap()
        })
        .collect();
    std::thread::sleep(Duration::from_secs(1));
    let from = cpu(pid);
    let window = Duration::from_secs(20);
    let start = Instant::now();
    let mut tick = 0;
    while start.elapsed() < window {
        for (file, path) in open.iter_mut().zip(&paths) {
            let line = log_line(tick);
            if keep_open {
                file.write_all(line.as_bytes()).unwrap();
            } else {
                let mut once = std::fs::OpenOptions::new().append(true).open(path).unwrap();
                once.write_all(line.as_bytes()).unwrap();
            }
        }
        tick += 1;
        let next = Duration::from_millis(tick as u64 * 100);
        std::thread::sleep(next.saturating_sub(start.elapsed()));
    }
    eprintln!(
        "{WRITERS} logs at 10 lines a second, {}: host {:.2}% of a core, {} MB",
        if keep_open {
            "held open"
        } else {
            "closed each time"
        },
        (cpu(pid) - from).as_secs_f64() / start.elapsed().as_secs_f64() * 100.,
        inspect::memory(pid).unwrap_or(0) / 1024 / 1024
    );
}

fn launch(socket: &Path, cwd: &str, id: String, argv: Vec<String>) {
    let reply = rpc(
        socket,
        &Request::Launch {
            id,
            space: "s".into(),
            argv,
            cwd: cwd.to_owned(),
            cols: 120,
            rows: 30,
            scheme: Default::default(),
            colors: None,
        },
    );
    assert!(!matches!(reply, Reply::Error(_)), "{reply:?}");
}

#[test]
#[ignore = "measures, and takes two minutes"]
fn how_much_a_host_costs() {
    let host = start_host();
    let pid = host.child.id();
    let socket = host.root.join("workbench.sock");
    let cwd = host.root.join("work").to_string_lossy().into_owned();
    let start = Instant::now();
    while Stream::connect(&socket).is_err() {
        assert!(start.elapsed() < Duration::from_secs(60));
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!("host answers after {:?}", start.elapsed());
    // Reading the old logs: done when a second goes by with next to no CPU.
    let reading = Instant::now();
    let mut last = cpu(pid);
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = cpu(pid);
        if now - last < Duration::from_millis(20) || reading.elapsed() > Duration::from_secs(120) {
            eprintln!(
                "old logs read in about {:?}, host CPU {now:?}, {} MB",
                reading.elapsed(),
                inspect::memory(pid).unwrap_or(0) / 1024 / 1024
            );
            break;
        }
        last = now;
    }
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

    let everything = inspect::all_pids().len();
    let (median, worst) = timed(100, || {
        let _ = inspect::children(pid);
    });
    eprintln!(
        "{everything} processes on this machine; children(): median {median:?}, worst {worst:?}"
    );
    let (median, worst) = timed(100, || {
        let _ = inspect::name(pid);
    });
    eprintln!("name(): median {median:?}, worst {worst:?}");

    for i in 0..TERMINALS {
        launch(
            &socket,
            &cwd,
            format!("t{i}"),
            shell::stay(&shell::interactive()),
        );
    }
    std::thread::sleep(Duration::from_secs(8));
    let Reply::Workspace(workspace) = rpc(&socket, &Request::Workspace) else {
        panic!("no workspace");
    };
    let (median, worst) = timed(20, || {
        fibo_host::recognition::processes(&workspace).unwrap();
    });
    let tree = fibo_host::recognition::processes(&workspace).unwrap().len();
    let shells = workspace
        .terminals
        .iter()
        .filter(|t| t.pid.is_some())
        .count();
    eprintln!("{shells} terminals with a process; found under them: {tree}");
    eprintln!(
        "walking {TERMINALS} terminals' {tree} processes, once per hook: median {median:?}, worst {worst:?}"
    );

    let (median, worst) = timed(20, || {
        let out = cli(&host.root).args(["list", "--json"]).output().unwrap();
        assert!(out.status.success());
    });
    eprintln!("a whole `fibo list`: median {median:?}, worst {worst:?}");
    let (median, worst) = timed(20, || {
        let _ = rpc(&socket, &Request::Workspace);
    });
    eprintln!("one request, connection included: median {median:?}, worst {worst:?}");

    let from = cpu(pid);
    let window = Duration::from_secs(30);
    std::thread::sleep(window);
    eprintln!(
        "{TERMINALS} idle terminals, {window:?}: host {:?}, {} MB",
        cpu(pid) - from,
        inspect::memory(pid).unwrap_or(0) / 1024 / 1024
    );

    log_round(&host, true);
    log_round(&host, false);
    write_round(&host, &socket, &cwd, "lines");
    write_round(&host, &socket, &cwd, "screen");
    for i in 0..TERMINALS {
        let _ = cli(&host.root).args(["close", &format!("t{i}")]).output();
    }
    let log =
        std::fs::read_to_string(std::env::temp_dir().join("kl-cost-host.log")).unwrap_or_default();
    eprintln!("host said, {} lines; last 8:", log.lines().count());
    for line in log.lines().rev().take(8) {
        eprintln!("  {line}");
    }
}

/// Files followed through a writer's open handle are looked at on a timer on Windows: what that
/// costs while nothing is being written, with more conversations open than anyone runs.
#[test]
#[ignore = "measures"]
fn how_much_following_costs() {
    use fibo_platform::file_watch::{normalized, Watch};
    let dir = std::env::temp_dir().join(format!("kl-follow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = normalized(&dir).unwrap();
    let files: Vec<_> = (0..40)
        .map(|i| {
            let file = dir.join(format!("{i}.jsonl"));
            std::fs::write(&file, "x\n").unwrap();
            file
        })
        .collect();
    let watch = Watch::start(std::slice::from_ref(&dir), Watch::current_id()).unwrap();
    watch.follow(&files).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let me = std::process::id();
    let from = cpu(me);
    let window = Duration::from_secs(20);
    std::thread::sleep(window);
    eprintln!(
        "40 followed files, nothing written: {:.3}% of a core",
        (cpu(me) - from).as_secs_f64() / window.as_secs_f64() * 100.
    );
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}
