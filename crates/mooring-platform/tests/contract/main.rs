//! What the interface promises, run unchanged on every platform: the cheapest place for a
//! difference between them to show. A test here has no platform branch unless the interface
//! itself says a platform may refuse.
mod clock;
mod dirs;
mod exit_watch;
mod fs;
mod inspect;
mod process;
mod shell;

/// This test program started again as `helper`, the one way to get a process that lasts a
/// while without assuming any command exists on the machine.
fn myself(helper: &str, pid_file: &std::path::Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args([helper, "--exact", "--ignored", "--nocapture"])
        .env(process::PID_FILE, pid_file);
    cmd
}

/// A new empty directory for one test.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kl-contract-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
