use crate::myself;
use mooring_platform::process::{Group, exists, kill};
use std::path::Path;
use std::time::{Duration, Instant};

/// Set on the copies of this test program that play the part of a command.
pub const PID_FILE: &str = "MOORING_CONTRACT_PID_FILE";

fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < Duration::from_secs(10), "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn pid_in(file: &Path) -> i32 {
    let mut pid = None;
    wait_until("the helper never wrote its child's pid", || {
        pid = std::fs::read_to_string(file)
            .ok()
            .and_then(|t| t.trim().parse().ok());
        pid.is_some()
    });
    pid.unwrap()
}

/// Only ever does anything as a copy started by `myself`.
#[test]
#[ignore = "started by the tests of inspect"]
fn helper_reads_its_input() {
    if std::env::var_os(PID_FILE).is_some() {
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    }
}

#[test]
#[ignore = "started by the tests in this file"]
fn helper_lasts() {
    if std::env::var_os(PID_FILE).is_some() {
        std::thread::sleep(Duration::from_secs(60));
    }
}

/// Starts a child of its own and says which, then lasts. The pause is the known gap on
/// Windows: a process joins its job just after it starts.
#[test]
#[ignore = "started by the tests in this file"]
fn helper_starts_a_child() {
    let Some(file) = std::env::var_os(PID_FILE) else {
        return;
    };
    std::thread::sleep(Duration::from_millis(300));
    let mut child = myself("process::helper_lasts", Path::new("unused"))
        .spawn()
        .unwrap();
    let temp = Path::new(&file).with_extension("tmp");
    std::fs::write(&temp, child.id().to_string()).unwrap();
    std::fs::rename(&temp, &file).unwrap();
    child.wait().unwrap();
}

#[test]
fn exists_tells_a_running_process_from_an_ended_one() {
    assert!(exists(std::process::id() as i32));
    let dir = crate::scratch("exists");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .spawn()
        .unwrap();
    let pid = child.id() as i32;
    assert!(exists(pid));
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(!exists(pid));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A stale or garbled pid file holds numbers like these. On Unix they mean a group, or
/// everything this account may signal, so `kill` running at all is part of the test.
#[test]
fn only_a_positive_number_is_a_process() {
    for pid in [0, -1, i32::MIN] {
        assert!(!exists(pid), "{pid}");
        kill(pid);
    }
}

#[test]
fn kill_ends_one_process() {
    let dir = crate::scratch("kill");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .spawn()
        .unwrap();
    kill(child.id() as i32);
    child.wait().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_group_ends_with_what_its_command_started() {
    let dir = crate::scratch("group");
    let file = dir.join("pid");
    let (mut child, group) =
        Group::spawn(&mut myself("process::helper_starts_a_child", &file)).unwrap();
    let grandchild = pid_in(&file);
    assert!(exists(grandchild));
    group.kill();
    child.wait().unwrap();
    wait_until("the group's child outlived it", || !exists(grandchild));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ending_a_group_after_its_leader_is_harmless() {
    let dir = crate::scratch("gone");
    let (mut child, group) =
        Group::spawn(&mut myself("process::helper_lasts", &dir.join("unused"))).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    group.kill();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropping_a_group_ends_nothing() {
    let dir = crate::scratch("drop");
    let mut child = {
        let (child, _group) =
            Group::spawn(&mut myself("process::helper_lasts", &dir.join("unused"))).unwrap();
        child
    };
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(child.try_wait().unwrap(), None);
    child.kill().unwrap();
    child.wait().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn terminate_and_a_group_end_a_process() {
    use mooring_platform::process::{hang_up_group, kill_group, terminate, was_signalled};
    let dir = crate::scratch("process-terminate");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .spawn()
        .unwrap();
    terminate(child.id() as i32).unwrap();
    let status = child.wait().unwrap();
    // Asked from outside, wherever the system can tell.
    let _ = was_signalled(&status);
    assert!(!status.success());
    // No such group is fine, and so are ids that name none.
    for ended in [hang_up_group, kill_group] {
        ended(child.id() as i32).unwrap();
        ended(0).unwrap();
        ended(-1).unwrap();
    }
    terminate(0).unwrap();
}

#[test]
fn a_thread_can_give_way() {
    std::thread::spawn(mooring_platform::process::give_way)
        .join()
        .unwrap();
}

#[test]
fn this_process_has_a_parent_and_may_raise_its_file_limit() {
    assert!(mooring_platform::process::parent() > 0);
    mooring_platform::process::raise_open_file_limit().unwrap();
}

/// Only ever does anything as a copy that was told to follow its parent.
#[test]
#[ignore = "started by the test below"]
fn helper_follows_its_parent() {
    if std::env::var_os("MOORING_CONTRACT_FOLLOW").is_some() {
        mooring_platform::process::exit_with_parent();
        std::thread::sleep(Duration::from_secs(60));
    }
}

/// Lasts as a parent whose child should end with it, and says which child that is.
#[test]
#[ignore = "started by the test below"]
fn helper_is_a_parent() {
    let Some(file) = std::env::var_os(PID_FILE) else {
        return;
    };
    let mut child = myself("process::helper_follows_its_parent", Path::new("unused"))
        .env("MOORING_CONTRACT_FOLLOW", "1")
        .spawn()
        .unwrap();
    let temp = Path::new(&file).with_extension("tmp");
    std::fs::write(&temp, child.id().to_string()).unwrap();
    std::fs::rename(&temp, &file).unwrap();
    std::thread::sleep(Duration::from_secs(60));
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn a_process_can_end_with_its_parent() {
    let dir = crate::scratch("process-follow");
    let file = dir.join("pid");
    let mut parent = myself("process::helper_is_a_parent", &file)
        .spawn()
        .unwrap();
    let child = pid_in(&file);
    assert!(exists(child));
    // Time for it to start following.
    std::thread::sleep(Duration::from_millis(500));
    parent.kill().unwrap();
    parent.wait().unwrap();
    drop(parent);
    wait_until("the child outlived its parent", || !exists(child));
}
