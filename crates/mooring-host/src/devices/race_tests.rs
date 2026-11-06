//! The host and `mooring remote` change the phones at once: nothing they do may be lost, and
//! a phone unpaired stays unpaired. Workers start together (a barrier, or a pipe between
//! processes) and every assertion is on the end state, so no timing is assumed.
use super::*;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::os::fd::AsRawFd;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Barrier};

const WORKERS: usize = 8;
const ROUNDS: usize = 10;
const SOCKET_ENV: &str = "MOORING_DEVICES_RACE_SOCKET";
const WORKER_ENV: &str = "MOORING_DEVICES_RACE_WORKER";

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("kl-race-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn socket(&self) -> PathBuf {
        self.0.join("workbench.sock")
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn kept(worker: usize, round: usize) -> String {
    format!("w{worker}-kept{round}")
}
fn unpaired(worker: usize, round: usize) -> String {
    format!("w{worker}-gone{round}")
}

/// One worker's share: every kind of change, each against what the others are changing.
/// Returns everything that went wrong, so a failure shows all of it and not only the first.
fn churn(socket: &Path, worker: usize) -> Vec<String> {
    let mut wrong = Vec::new();
    let mut removed = HashSet::new();
    for round in 0..ROUNDS {
        let (keep, go) = (kept(worker, round), unpaired(worker, round));
        let step = || -> io::Result<()> {
            add(socket, &keep, Role::View)?;
            add(socket, &go, Role::Full)?;
            set_role(socket, &keep, Role::Answer)?;
            kick(socket, &keep)?;
            remove(socket, &go).map(drop)
        };
        match step() {
            Ok(()) => {
                removed.insert(go);
            }
            Err(e) => wrong.push(format!("{keep}: {:?}: {e}", e.kind())),
        }
        if let Some(back) = load(socket).iter().find(|d| removed.contains(&d.name)) {
            wrong.push(format!("{} 取消配对后又出现了", back.name));
        }
    }
    wrong
}

/// What `WORKERS` finished churns must leave, whichever way they interleaved.
fn assert_settled(socket: &Path) {
    let devices = load(socket);
    let names: HashSet<&str> = devices.iter().map(|d| d.name.as_str()).collect();
    let mut lost = Vec::new();
    for worker in 0..WORKERS {
        for round in 0..ROUNDS {
            let name = kept(worker, round);
            match devices.iter().find(|d| d.name == name) {
                None => lost.push(format!("{name} 丢了")),
                Some(d) if d.role != Role::Answer => lost.push(format!("{name} 的权限丢了")),
                Some(d) if d.kicked == 0 => lost.push(format!("{name} 的断开丢了")),
                Some(_) => (),
            }
            let name = unpaired(worker, round);
            if names.contains(name.as_str()) {
                lost.push(format!("{name} 复活了"));
            }
        }
    }
    assert!(lost.is_empty(), "{lost:#?}");
    assert_eq!(devices.len(), WORKERS * ROUNDS, "多出了设备");
    assert_eq!(
        revoked(socket).iter().collect::<HashSet<_>>().len(),
        WORKERS * ROUNDS,
        "撤销名单丢了路由"
    );
    assert_no_temporaries(socket);
}

fn assert_no_temporaries(socket: &Path) {
    let left: Vec<_> = std::fs::read_dir(socket.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .filter(|n| n.to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(left.is_empty(), "留下了临时文件：{left:?}");
}

#[test]
fn threads_changing_the_phones_lose_nothing_and_revive_nothing() {
    let dir = Dir::new();
    let barrier = Arc::new(Barrier::new(WORKERS));
    let workers: Vec<_> = (0..WORKERS)
        .map(|worker| {
            let (socket, barrier) = (dir.socket(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                churn(&socket, worker)
            })
        })
        .collect();
    let wrong: Vec<String> = workers
        .into_iter()
        .flat_map(|w| w.join().unwrap())
        .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
    assert_settled(&dir.socket());
}

/// Not a test of its own: `processes_changing_the_phones_...` runs this binary on it.
#[test]
#[ignore = "run by processes_changing_the_phones_lose_nothing_and_revive_nothing"]
fn one_writer_process() {
    let (Ok(socket), Ok(worker)) = (std::env::var(SOCKET_ENV), std::env::var(WORKER_ENV)) else {
        return;
    };
    // Written directly: the test harness keeps `println!` to itself until the test ends.
    let mut out = io::stdout();
    writeln!(out, "ready").unwrap();
    out.flush().unwrap();
    // Waits until the parent has every process ready and closes this pipe.
    io::stdin().read_to_end(&mut Vec::new()).unwrap();
    let wrong = churn(Path::new(&socket), worker.parse().unwrap());
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn processes_changing_the_phones_lose_nothing_and_revive_nothing() {
    let dir = Dir::new();
    let mut children: Vec<(Child, BufReader<std::process::ChildStdout>)> = (0..WORKERS)
        .map(|worker| {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "--ignored", "--nocapture"])
                .arg("devices::race_tests::one_writer_process")
                .env(SOCKET_ENV, dir.socket())
                .env(WORKER_ENV, worker.to_string())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let stdout = BufReader::new(child.stdout.take().unwrap());
            (child, stdout)
        })
        .collect();
    for (_, stdout) in &mut children {
        let mut line = String::new();
        while !line.trim_end().ends_with("ready") {
            line.clear();
            assert_ne!(stdout.read_line(&mut line).unwrap(), 0, "写进程没有就绪");
        }
    }
    for (child, _) in &mut children {
        drop(child.stdin.take());
    }
    let mut wrong = Vec::new();
    for (mut child, mut stdout) in children {
        let mut said = String::new();
        stdout.read_to_string(&mut said).unwrap();
        if !child.wait().unwrap().success() {
            wrong.push(said);
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert_settled(&dir.socket());
}

/// Each phone ever added ends up exactly one of paired or revoked, however `clear` and `add`
/// (`--new-code` beside a pairing in the App) fall.
#[test]
fn clearing_beside_pairing_leaves_each_phone_paired_or_revoked() {
    let dir = Dir::new();
    let barrier = Arc::new(Barrier::new(WORKERS));
    let pairing: Vec<_> = (1..WORKERS)
        .map(|worker| {
            let (socket, barrier) = (dir.socket(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                (0..ROUNDS)
                    .map(|round| add(&socket, &kept(worker, round), Role::View).map(|(d, _)| d))
                    .collect::<io::Result<Vec<Device>>>()
            })
        })
        .collect();
    barrier.wait();
    for _ in 0..ROUNDS {
        clear(&dir.socket()).unwrap();
    }
    let added: Vec<Device> = pairing
        .into_iter()
        .flat_map(|w| w.join().unwrap().unwrap())
        .collect();
    let socket = dir.socket();
    let paired = routes(&load(&socket));
    let gone = revoked(&socket);
    for device in added {
        let hash = device.route_hash().unwrap();
        assert!(
            paired.contains(&hash) != gone.contains(&hash),
            "{} 应当只在已配对或已撤销其中一处",
            device.name
        );
    }
    assert_no_temporaries(&socket);
}

/// flock does not recognise its own process: a second open file waits like anyone else. This
/// is what keeps the host's threads apart, and why nothing may lock while holding the lock.
#[test]
fn the_lock_excludes_another_open_file_in_this_process_and_is_private() {
    let dir = Dir::new();
    let socket = dir.socket();
    let try_lock = || {
        let other = std::fs::File::open(socket.with_file_name("remote-devices.lock")).unwrap();
        // SAFETY: flock on a file this closure holds open.
        unsafe { libc::flock(other.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
    };
    let held = lock(&socket).unwrap();
    assert!(!try_lock(), "持锁时第二个 fd 不该拿到");
    drop(held);
    assert!(try_lock(), "放锁后应当能拿到");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(socket.with_file_name("remote-devices.lock"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_failed_write_leaves_no_temporary() {
    let dir = Dir::new();
    let socket = dir.socket();
    // A directory where the file goes: the temporary is written, the rename fails.
    std::fs::create_dir(file(&socket)).unwrap();
    assert!(save(&socket, &[]).is_err());
    assert!(file(&socket).is_dir());
    assert_no_temporaries(&socket);
}

/// A process killed between writing and renaming cannot clean up after itself, and what it
/// leaves holds every key, so the next change removes it, and nothing else.
#[test]
fn the_next_change_removes_what_a_killed_write_left_and_nothing_else() {
    let dir = Dir::new();
    let socket = dir.socket();
    let id = "0123456789abcdef0123456789abcdef";
    let abandoned = format!("remote-devices.4242.{id}.tmp");
    let unrelated = [
        "remote-devices.notes.tmp".to_owned(),
        "remote-devices.4242.tmp".to_owned(),
        "remote-devices.4242.short.tmp".to_owned(),
        format!("remote-devices.x.{id}.tmp"),
        format!("remote-devices.4242.{id}.tmp.bak"),
        format!("remote-routes-revoked.4242.{id}.tmp"),
        format!("machine-id.4242.{id}.tmp"),
    ];
    std::fs::write(dir.0.join(&abandoned), "keys").unwrap();
    for name in &unrelated {
        std::fs::write(dir.0.join(name), "other").unwrap();
    }
    let (device, _) = add(&socket, "iPhone", Role::Full).unwrap();
    assert!(!dir.0.join(&abandoned).exists(), "残留的临时文件还在");
    for name in &unrelated {
        assert!(dir.0.join(name).exists(), "{name} 不该被删");
    }
    // Left again, it goes on the way out of a removal too.
    std::fs::write(dir.0.join(&abandoned), "keys").unwrap();
    remove(&socket, &device.id).unwrap();
    assert!(!dir.0.join(&abandoned).exists(), "残留的临时文件还在");
}
