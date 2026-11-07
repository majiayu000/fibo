use crate::myself;
use mooring_platform::inspect::{
    all_pids, argv, children, cwd, env_var, executable, memory, name, started, started_micros,
    stdin_is_a_channel,
};
use std::io::ErrorKind;
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    // Linux fills these in a moment after the exec that ended `spawn`.
    let until = Instant::now() + Duration::from_secs(10);
    let mut probe = None;
    while !refuses && probe.is_none() && Instant::now() < until {
        probe = env_var(pid, "MOORING_CONTRACT_PROBE");
        std::thread::sleep(Duration::from_millis(5));
    }
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

#[test]
fn start_time_says_the_same_in_either_unit() {
    let me = std::process::id();
    let micros = started_micros(me).unwrap();
    assert_eq!(started(me), Some(micros / 1_000_000));
}

#[test]
fn this_process_is_among_all_of_them() {
    assert!(all_pids().contains(&std::process::id()));
}

#[test]
fn name_and_program_are_the_running_test() {
    let me = std::process::id();
    let path = std::env::current_exe().unwrap();
    assert_eq!(
        std::fs::canonicalize(executable(me).unwrap()).unwrap(),
        std::fs::canonicalize(&path).unwrap()
    );
    // What a name keeps of it differs, but it starts the same: `contract-<hash>`.
    let found = name(me).unwrap();
    assert!(found.starts_with("contract"), "{found}");
}

#[test]
fn an_ended_process_has_no_name_or_program() {
    let dir = crate::scratch("inspect-gone-name");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .spawn()
        .unwrap();
    let pid = child.id();
    child.kill().unwrap();
    child.wait().unwrap();
    // On Windows a process lasts as long as a handle to it.
    drop(child);
    assert_eq!(name(pid), None);
    assert!(executable(pid).is_err());
    assert_eq!(memory(pid), None);
    assert!(!all_pids().contains(&pid));
}

#[test]
fn a_process_uses_some_memory() {
    let bytes = memory(std::process::id()).expect("a process may read its own");
    assert!(bytes > 1 << 20, "{bytes}");
}

#[test]
fn standard_input_tells_a_pipe_from_nothing() {
    let dir = crate::scratch("inspect-stdin");
    let mut piped = myself("process::helper_lasts", &dir.join("unused"))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut empty = myself("process::helper_lasts", &dir.join("unused"))
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let (is_pipe, is_null) = (
        stdin_is_a_channel(piped.id()),
        stdin_is_a_channel(empty.id()),
    );
    let refuses = refused(&argv(piped.id()));
    for child in [&mut piped, &mut empty] {
        child.kill().unwrap();
        child.wait().unwrap();
    }
    assert!(!is_null);
    assert!(is_pipe || refuses);
}
