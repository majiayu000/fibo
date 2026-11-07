use crate::myself;
use mooring_platform::inspect::{
    all_pids, argv, children, cwd, env_var, executable, first_handed_out_port, memory, name,
    open_files, started, started_micros, stdin_is_a_channel, tcp_sockets, usage,
};
use std::io::ErrorKind;
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The interface lets Windows refuse open files.
fn refused<T>(result: &std::io::Result<T>) -> bool {
    matches!(result, Err(e) if e.kind() == ErrorKind::Unsupported)
}

#[test]
fn arguments_are_the_ones_the_process_was_given() {
    let mine = argv(std::process::id()).unwrap();
    assert_eq!(mine, std::env::args().collect::<Vec<_>>());
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
    assert_eq!(
        argv(pid).unwrap().last().map(String::as_str),
        Some("MOORING_CONTRACT_PROBE=from-arguments")
    );
    // Linux fills these in a moment after the exec that ended `spawn`.
    let until = Instant::now() + Duration::from_secs(10);
    let mut probe = None;
    while probe.is_none() && Instant::now() < until {
        probe = env_var(pid, "MOORING_CONTRACT_PROBE");
        std::thread::sleep(Duration::from_millis(5));
    }
    let longer = env_var(pid, "MOORING_CONTRACT_ONLY_LONGER");
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(probe.as_deref(), Some("from-environment"));
    assert_eq!(longer, None);
}

#[test]
fn the_directory_is_where_the_process_is() {
    assert_eq!(
        cwd(std::process::id()).map(std::path::PathBuf::from),
        std::env::current_dir().ok()
    );
}

#[test]
fn the_directory_of_another_process_is_where_it_was_started() {
    let dir = crate::scratch("inspect-other-cwd");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .current_dir(&dir)
        .spawn()
        .unwrap();
    let pid = child.id();
    // Linux fills this in a moment after the exec that ended `spawn`.
    let until = Instant::now() + Duration::from_secs(10);
    let mut found = None;
    while found.is_none() && Instant::now() < until {
        found = cwd(pid);
    }
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(
        found.map(std::path::PathBuf::from),
        Some(mooring_platform::fs::canonical(&dir).unwrap())
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
    for child in [&mut piped, &mut empty] {
        child.kill().unwrap();
        child.wait().unwrap();
    }
    assert!(!is_null);
    assert!(is_pipe);
}

/// Windows answers about a pipe only once a read blocked on it ends, which may be never.
#[test]
fn a_process_blocked_reading_a_pipe_is_answered_for_at_once() {
    let dir = crate::scratch("inspect-stdin-blocked");
    let mut reading = myself("process::helper_reads_its_input", &dir.join("unused"))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(1000));
    let asked = Instant::now();
    let first = stdin_is_a_channel(reading.id());
    let second = stdin_is_a_channel(reading.id());
    let took = asked.elapsed();
    reading.kill().unwrap();
    reading.wait().unwrap();
    assert!(first && second);
    assert!(took < Duration::from_secs(2), "{took:?}");
}

#[test]
fn sockets_say_what_a_process_listens_on_and_connects_to() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let _client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let me = std::process::id();
    let found = tcp_sockets(&[me]).unwrap();
    assert!(found.listening.contains(&(me, port)), "{found:?}");
    assert!(found.connected.contains(&(me, port)), "{found:?}");
    assert_eq!(tcp_sockets(&[]).unwrap(), Default::default());
}

#[test]
fn open_files_name_what_a_process_holds() {
    let dir = crate::scratch("inspect-open");
    let path = std::fs::canonicalize(&dir).unwrap().join("held.jsonl");
    let _held = std::fs::File::create(&path).unwrap();
    let me = std::process::id();
    let found = open_files(&[me]);
    if refused(&found) {
        return;
    }
    let found = found.unwrap();
    let path = path.to_string_lossy().into_owned();
    assert!(found[&me].contains(&path), "{:?}", found[&me]);
}

#[test]
fn cpu_use_grows_with_work() {
    let me = std::process::id();
    let first = usage(me).expect("a process may read its own");
    let until = Instant::now() + Duration::from_secs(20);
    let mut spin = 0u64;
    loop {
        for i in 0..1_000_000u64 {
            spin = std::hint::black_box(spin.wrapping_add(i));
        }
        let now = usage(me).unwrap();
        if now.cpu > first.cpu {
            assert!(now.cpu < Duration::from_secs(3600), "{now:?}");
            return;
        }
        assert!(Instant::now() < until, "still {now:?}");
    }
}

#[test]
fn a_port_the_system_hands_out_is_past_where_those_start() {
    let socket = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = socket.local_addr().unwrap().port();
    assert!(port >= first_handed_out_port(), "{port}");
}
