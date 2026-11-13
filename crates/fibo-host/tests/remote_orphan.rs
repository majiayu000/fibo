//! The relay connection a host starts must not outlive that host: left behind after a
//! handover it redials and fights the new host's connection for the machine's place.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[test]
fn a_hosts_relay_connection_ends_when_its_host_is_gone() {
    let root = std::env::temp_dir().join(format!("mo-orphan-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // A shell plays the host: it starts the connection, says its pid and exits without
    // stopping it, as a host does after handing over. Nothing listens on the relay's port,
    // so the connection stays alive only by redialing.
    let bin = env!("CARGO_BIN_EXE_fibo");
    let script = format!(
        "'{bin}' remote --relay ws://127.0.0.1:9 --report --runtime '{}' >/dev/null 2>&1 </dev/null & echo $!; sleep 2",
        root.display()
    );
    let host = Command::new("sh").args(["-c", &script]).output().unwrap();
    let pid = String::from_utf8(host.stdout).unwrap().trim().to_owned();
    assert!(!pid.is_empty());
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(&pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let left = alive(&pid);
    if left {
        Command::new("kill").arg(&pid).status().unwrap();
    }
    assert!(!left, "the connection outlived its host");
}
