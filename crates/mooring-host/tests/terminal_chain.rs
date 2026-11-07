//! The whole way through on whatever system this runs on: a host starts, opens this system's
//! own shell in a terminal, and the command line lists it, reads its screen and types into it.
#[cfg_attr(unix, path = "support/tied.rs")]
#[cfg_attr(windows, path = "support/tied_windows.rs")]
mod tied;
use mooring_core::{model::*, protocol::*};
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
