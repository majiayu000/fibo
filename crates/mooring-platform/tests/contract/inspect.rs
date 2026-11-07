use crate::myself;
use mooring_platform::inspect::{argv, children, cwd, env_var, started};
use std::io::ErrorKind;
use std::time::{SystemTime, UNIX_EPOCH};

/// The interface lets Windows refuse this one for now.
fn refused<T>(result: &std::io::Result<T>) -> bool {
    matches!(result, Err(e) if e.kind() == ErrorKind::Unsupported)
}

#[test]
fn arguments_are_the_ones_the_process_was_given() {
    let mine = argv(std::process::id());
    if refused(&mine) {
        return;
    }
    assert_eq!(mine.unwrap(), std::env::args().collect::<Vec<_>>());
}

#[test]
fn asking_after_an_ended_process_is_not_an_answer() {
    let dir = crate::scratch("inspect-gone");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .spawn()
        .unwrap();
    let pid = child.id();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(argv(pid).is_err());
    assert_eq!(env_var(pid, "PATH"), None);
    assert_eq!(cwd(pid), None);
    assert_eq!(children(pid).unwrap(), Vec::<u32>::new());
}

#[test]
fn a_variable_comes_from_the_environment_and_is_its_whole_name() {
    let dir = crate::scratch("inspect-env");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .args(["--skip", "MOORING_CONTRACT_PROBE=from-arguments"])
        .env("MOORING_CONTRACT_PROBE", "from-environment")
        .env("MOORING_CONTRACT_ONLY_LONGER_X", "1")
        .spawn()
        .unwrap();
    let pid = child.id();
    let refuses = refused(&argv(pid));
    let probe = env_var(pid, "MOORING_CONTRACT_PROBE");
    let longer = env_var(pid, "MOORING_CONTRACT_ONLY_LONGER");
    child.kill().unwrap();
    child.wait().unwrap();
    if refuses {
        return;
    }
    assert_eq!(probe.as_deref(), Some("from-environment"));
    assert_eq!(longer, None);
}

#[test]
fn the_directory_is_where_the_process_is() {
    if refused(&argv(std::process::id())) {
        return;
    }
    assert_eq!(
        cwd(std::process::id()).map(std::path::PathBuf::from),
        std::env::current_dir().ok()
    );
}

#[test]
fn children_are_what_the_process_started() {
    let dir = crate::scratch("inspect-children");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .spawn()
        .unwrap();
    let found = children(std::process::id()).unwrap();
    let theirs = children(child.id()).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(found.contains(&child.id()), "{found:?}");
    assert!(theirs.is_empty(), "{theirs:?}");
}

#[test]
fn a_process_started_in_the_past_and_not_long_ago() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let at = started(std::process::id()).expect("this process has a start time");
    assert!(at <= now && now - at < 3600, "started {at}, now {now}");
}
