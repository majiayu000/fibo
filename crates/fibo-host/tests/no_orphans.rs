//! A test run leaves no process behind, however it ends. Fake Agents in leftover terminals
//! once outlived their runs by a day, each loop forking twenty times a second. Each case
//! marks everything it starts with its runtime's name, and looks for that name afterwards.
#[path = "support/tied.rs"]
mod tied;
use fibo_core::{model::*, protocol::*};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tied::{SpawnTied, Tied};

const ROOT_ENV: &str = "MOORING_NO_ORPHANS_ROOT";
const PANIC_ENV: &str = "MOORING_NO_ORPHANS_PANIC";

fn new_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("kl-{}", uuid::Uuid::new_v4().simple()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    std::os::unix::fs::symlink("/bin/sh", root.join("claude")).unwrap();
    root
}
fn serve(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fibo"));
    command
        .args(["serve", "--runtime"])
        .arg(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
/// Starts `argv` in a terminal of the host at `root`.
fn launch(root: &Path, id: &str, argv: Vec<String>) {
    let request = Request::Launch {
        id: id.into(),
        space: "s".into(),
        argv,
        cwd: "/tmp".into(),
        cols: 80,
        rows: 24,
        scheme: Default::default(),
        colors: None,
    };
    rpc(&root.join("workbench.sock"), &request).unwrap();
}
/// A fake Agent and the loop `a_taken_terminal_follows_the_remote_panes_size` once left
/// behind, neither looking at its parent. Waits until host and both are running.
fn fill(root: &Path) {
    let socket = root.join("workbench.sock");
    let until = Instant::now() + Duration::from_secs(30);
    while rpc(&socket, &Request::Workspace).is_err() {
        assert!(Instant::now() < until, "host never listened");
        std::thread::sleep(Duration::from_millis(20));
    }
    let edit = Edit::AddSpace(Space {
        id: "s".into(),
        name: "Tests".into(),
        cwd: "/tmp".into(),
    });
    let add = Request::Edit {
        request: None,
        edit,
    };
    rpc(&socket, &add).unwrap();
    let name = name(root);
    let agent = root.join("claude").to_string_lossy().into_owned();
    let script = format!(": {name}; while :; do sleep 0.05; done");
    launch(root, "agent", vec![agent, "-c".into(), script.clone()]);
    let sh = "/bin/sh".to_owned();
    launch(root, "loop", vec![sh, "-c".into(), script]);
    wait_for_running(root, 3);
}
/// A program that ignores hangup on purpose: only a signal to its group can end it.
fn add_hangup_ignorer(root: &Path) {
    let script = format!(
        ": {}; trap '' HUP; while :; do sleep 0.05; done",
        name(root)
    );
    launch(root, "deaf", vec!["/bin/sh".into(), "-c".into(), script]);
    wait_for_running(root, 4);
}
fn name(root: &Path) -> String {
    root.file_name().unwrap().to_string_lossy().into_owned()
}
fn wait_for_running(root: &Path, count: usize) {
    let until = Instant::now() + Duration::from_secs(10);
    while running(root).len() < count {
        assert!(
            Instant::now() < until,
            "expected {count} running: {:?}",
            running(root)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
/// Every process with this runtime's name on its command line, as `pid command`.
fn running(root: &Path) -> Vec<String> {
    let out = Command::new("/usr/bin/pgrep")
        .args(["-fl", &name(root)])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}
/// Passes once nothing of `root` runs. A failure ends what it found: a test for leftovers
/// must not leave any.
fn assert_all_gone(root: &Path) {
    let until = Instant::now() + Duration::from_secs(10);
    let mut left = running(root);
    while !left.is_empty() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
        left = running(root);
    }
    for line in &left {
        if let Some(pid) = line.split(' ').next().and_then(|p| p.parse::<i32>().ok()) {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
    let _ = std::fs::remove_dir_all(root);
    assert!(left.is_empty(), "still running: {left:#?}");
}
/// A whole test run in a process of its own, for the cases that end it.
fn run(root: &Path, panics: bool) -> Tied {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", "a_test_run", "--nocapture"])
        .env(ROOT_ENV, root)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if panics {
        command.env(PANIC_ENV, "1");
    }
    command.spawn_tied().unwrap()
}
#[test]
#[ignore = "subprocess entry for the tests below"]
fn a_test_run() {
    let Some(root) = std::env::var_os(ROOT_ENV).map(PathBuf::from) else {
        return;
    };
    let _host = serve(&root).spawn_tied().unwrap();
    fill(&root);
    add_hangup_ignorer(&root);
    std::fs::write(root.join("ready"), "").unwrap();
    assert!(std::env::var_os(PANIC_ENV).is_none(), "on purpose");
    std::thread::sleep(Duration::from_secs(120));
}
fn wait_ready(root: &Path) {
    let until = Instant::now() + Duration::from_secs(30);
    while !root.join("ready").exists() {
        assert!(Instant::now() < until, "the run never got ready");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Nothing unwinds here: this is a timeout, or someone cleaning up with `kill -9`.
#[test]
fn a_killed_test_run_takes_its_host_and_terminals_with_it() {
    let root = new_root();
    let mut run = run(&root, false);
    wait_ready(&root);
    run.kill().unwrap();
    run.wait().unwrap();
    assert_all_gone(&root);
}

#[test]
fn a_panicking_test_leaves_nothing_behind() {
    let root = new_root();
    let mut run = run(&root, true);
    assert!(!run.wait().unwrap().success());
    assert_all_gone(&root);
}

/// A run started under `nohup`, or by whatever else ignores hangup, passes that on through
/// exec. The terminal closing is the only signal a program gets when its host is killed,
/// and an inherited ignore cannot be trapped back: that left the loops above running.
#[test]
fn a_killed_host_ends_its_terminals_even_when_started_with_hangup_ignored() {
    let root = new_root();
    let mut command = serve(&root);
    // SAFETY: signal(2) is async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            Ok(())
        });
    }
    let mut host = command.spawn_tied().unwrap();
    fill(&root);
    // The host alone: what ends the terminals here is the terminal closing, not the cleanup.
    host.kill().unwrap();
    host.wait().unwrap();
    assert_all_gone(&root);
}

/// `spawn_tied` is what the cases above prove; a harness that starts a host with a bare
/// `spawn` is back to leaving it running when its run is killed.
#[test]
fn every_host_and_relay_connection_a_test_starts_is_tied() {
    fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                found.push(path);
            }
        }
    }
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files = Vec::new();
    sources(&tests, &mut files);
    let mut bare = Vec::new();
    for file in files.iter().filter(|f| !f.ends_with("no_orphans.rs")) {
        let text = std::fs::read_to_string(file).unwrap();
        for (at, _) in text.match_indices(".spawn()") {
            let before = &text[..at];
            let start = ["Command::new", "fixture_command("]
                .iter()
                .filter_map(|from| before.rfind(from))
                .max()
                .unwrap_or(0);
            let chain = &before[start..];
            if [
                "fixture_command",
                "\"--ignored\"",
                "\"serve\"",
                "\"--relay\"",
            ]
            .iter()
            .any(|long_lived| chain.contains(long_lived))
            {
                let line = before.lines().count() + 1;
                bare.push(format!("{}:{line}", file.display()));
            }
        }
    }
    assert!(bare.is_empty(), "use spawn_tied: {bare:#?}");
}
