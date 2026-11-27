//! Other machines' own hosts, worked with the same code as this machine's
//! (docs/decisions/2026-11-14-one-window-many-hosts.md). For each, an `ssh -L` keeps that
//! host's socket at `<runtime>/m/<host>/workbench.sock` here; a thread starts it again
//! whenever it ends, until the app quits. A host there is never started, stopped or replaced
//! from here. `Host` says, for any socket this app talks to, which machine it is.
//! A machine paired through the relay (`via: relay`) is kept the same way by
//! `fibo remote dial` instead of `ssh -L` (docs/decisions/2026-11-27-computers-over-relay.md).
use crate::machines::{Spec, Via};
use fibo_platform::local;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod quota;

struct Forward {
    spec: Spec,
    /// That host's socket on its machine, as it said (`fibo socket`).
    remote: String,
    /// `$HOME` there.
    home: String,
    /// `$SHELL` there: what its terminals start, as its own window's do.
    shell: String,
    /// Its id and name as its host gives them to phones (`machine-id`, `machine-name`
    /// beside its socket, else its host name); the id is empty before a phone asked.
    machine: (String, String),
    /// Where it is forwarded to here: `<root>/workbench.sock`.
    local: PathBuf,
    child: Mutex<Option<Child>>,
    stopped: AtomicBool,
}
/// Every forward this app keeps, found by the folder its socket is in.
static FORWARDS: Mutex<Vec<Arc<Forward>>> = Mutex::new(Vec::new());
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// The machine a host runs on, as this app reaches it: this one, or another through its
/// forward. Cheap to clone; ask it rather than assuming the machine is this one.
#[derive(Clone, Default)]
pub struct Host(Option<Arc<Forward>>);

impl Host {
    /// The host behind `socket` (a `workbench.sock`, or a folder holding one).
    pub fn of(socket: &Path) -> Host {
        let local = match socket.file_name().is_some_and(|n| n == "workbench.sock") {
            true => socket.to_owned(),
            false => socket.join("workbench.sock"),
        };
        let forwards = FORWARDS.lock().unwrap();
        Host(forwards.iter().find(|f| f.local == local).cloned())
    }
    /// Another machine's: its folders, files, shell and Agents are there, not here.
    pub fn attached(&self) -> bool {
        self.0.is_some()
    }
    /// What this app calls that machine: 本机, or the name it was written down by.
    pub fn name(&self) -> String {
        match &self.0 {
            Some(f) => f.spec.host.clone(),
            None => t!("本机").to_owned(),
        }
    }
    /// `$HOME` on that machine: the folders this app offers there start from it.
    pub fn home(&self) -> Option<String> {
        match &self.0 {
            Some(f) => Some(f.home.clone()),
            None => fibo_platform::dirs::home_str(),
        }
    }
    /// What a new terminal there starts: its login shell.
    /// That machine's id and name as its host gives them; None for this machine.
    pub fn machine(&self) -> Option<(String, String)> {
        self.0.as_ref().map(|f| f.machine.clone())
    }
    pub fn shell(&self) -> String {
        match &self.0 {
            Some(f) => f.shell.clone(),
            None => fibo_platform::shell::interactive(),
        }
    }
    /// `fibo` there as its terminals reach it: the path written down for it, else the name
    /// on their PATH. None for this machine, which finds its own.
    pub fn program(&self) -> Option<String> {
        let f = self.0.as_ref()?;
        Some(
            f.spec
                .path
                .clone()
                .unwrap_or_else(|| fibo_core::cli_entry::NAME.to_owned()),
        )
    }
    /// `fibo <args>` run there over ssh; None for this machine, which runs its own.
    pub fn run(&self, args: &[&str]) -> Option<io::Result<std::process::Output>> {
        let f = self.0.as_ref()?;
        if f.spec.via == Via::Relay {
            return Some(Err(io::Error::other(t!(
                "{host} 经中继连着，这里不能在那台上运行命令；到那台电脑上操作，或改用 ssh 连它",
                host = &f.spec.host
            ))));
        }
        let quoted: Vec<String> = args
            .iter()
            .map(|a| fibo_platform::shell::quote_posix(a))
            .collect();
        let script = format!(
            "{}; exec \"$p\" {}",
            f.spec.find_program(),
            quoted.join(" ")
        );
        let argv = f.spec.ssh(&script);
        let mut command = Command::new(&argv[0]);
        command.args(&argv[1..]).stdin(Stdio::null());
        fibo_platform::process::no_console(&mut command);
        Some(command.output())
    }
}

/// Where this machine keeps the forwarded socket of `host`.
pub fn root(host: &str) -> io::Result<PathBuf> {
    let home = fibo_platform::dirs::home().ok_or_else(|| io::Error::other("HOME unavailable"))?;
    Ok(fibo_core::legacy::runtime_dir(&home)
        .join("m")
        .join(dir_name(host)))
}
/// `user@host` or an ssh alias as one short folder name: a readable start and a hash of
/// the whole destination, so `alice@server` and `alice-server` never share a socket or a
/// lock, and the socket path stays under the 104-byte limit however long the host is.
fn dir_name(host: &str) -> String {
    let start: String = (host.chars())
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    // FNV-1a: the same name on every run and every build.
    let hash = (host.bytes()).fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{start}-{:08x}", hash as u32)
}

/// Asks `host` where its host listens, starts the forward to `root` and keeps it up; one
/// already kept for `root` is reused.
pub fn start(host: &str, root: &Path) -> io::Result<Host> {
    let found = Host::of(root);
    if found.attached() {
        return Ok(found);
    }
    let spec = (crate::machines::load().unwrap_or_default().into_iter())
        .find(|s| s.host == host)
        .unwrap_or_else(|| Spec::new(host.to_owned()));
    let (remote, home, shell, machine) = match spec.via {
        Via::Ssh => remote_socket(&spec)?,
        Via::Relay => relay_about(&spec)?,
    };
    fibo_platform::fs::create_private_dir(root)?;
    let forward = Arc::new(Forward {
        spec,
        remote,
        home,
        shell,
        machine,
        local: root.join("workbench.sock"),
        child: Mutex::new(None),
        stopped: AtomicBool::new(false),
    });
    FORWARDS.lock().unwrap().push(forward.clone());
    let kept = forward.clone();
    std::thread::spawn(move || supervise(&kept));
    Ok(Host(Some(forward)))
}

/// Waits until the forwarded socket answers; what `spawn_host` does for an attached host.
pub fn ensure(root: &Path) -> io::Result<()> {
    let host = Host::of(root);
    let forward = host
        .0
        .as_ref()
        .ok_or_else(|| io::Error::other("not attached"))?;
    let socket = root.join("workbench.sock");
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        if local::Stream::connect(&socket).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(io::Error::other(match forward.spec.via {
        Via::Ssh => t!(
            "连不上 {host} 上的 Fibo：ssh 转发没有在 20 秒内接通",
            host = &forward.spec.host
        ),
        Via::Relay => t!(
            "连不上 {host} 上的 Fibo：中继转发没有在 20 秒内接通",
            host = &forward.spec.host
        ),
    }))
}

/// 在新窗口打开: this app again, attached to `host`; one already open for it comes forward
/// instead (its own instance lock).
pub fn open_window(host: &str) -> io::Result<()> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--attach", host])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    fibo_platform::process::detach(&mut command);
    command.spawn().map(drop)
}

/// Ends only the local forward; the other machine's host keeps running.
fn stop_forward(forward: &Forward) {
    forward.stopped.store(true, Ordering::Release);
    if let Some(mut child) = forward.child.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
pub fn forget(host: &str) {
    FORWARDS.lock().unwrap().retain(|f| {
        if f.spec.host != host {
            return true;
        }
        stop_forward(f);
        false
    });
}
/// Ends every forward as the app quits.
pub fn stop() {
    for forward in FORWARDS.lock().unwrap().drain(..) {
        stop_forward(&forward);
    }
}

fn remote_socket(spec: &Spec) -> io::Result<(String, String, String, (String, String))> {
    let script = format!(
        "{}; s=$({{ \"$p\" socket 2>/dev/null || for d in fibo-workbench mooring-workbench keepline-workbench; \
         do [ -d \"$HOME/.local/share/$d\" ] && {{ echo \"$HOME/.local/share/$d/workbench.sock\"; break; }}; done; }}); \
         echo \"$s\"; echo \"$HOME\"; echo \"${{SHELL:-/bin/sh}}\"; d=$(dirname \"$s\"); \
         echo \"$(cat \"$d/machine-id\" 2>/dev/null)\"; n=$(cat \"$d/machine-name\" 2>/dev/null); \
         echo \"${{n:-$(hostname)}}\"",
        spec.find_program()
    );
    let argv = spec.ssh(&script);
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]).stdin(Stdio::null());
    fibo_platform::process::takes_term(&mut command);
    fibo_platform::process::no_console(&mut command);
    let out = command.output()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut lines = text.lines().map(str::trim);
    let (path, home) = (
        lines.next().unwrap_or_default(),
        lines.next().unwrap_or_default(),
    );
    // The script answers `${SHELL:-/bin/sh}`, so an empty line is a broken answer.
    let shell = lines.next().unwrap_or_default();
    let machine = (
        lines.next().unwrap_or_default().to_owned(),
        lines.next().unwrap_or_default().to_owned(),
    );
    if ![path, home, shell].iter().all(|l| l.starts_with('/')) {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(io::Error::other(t!(
            "在 {host} 上找不到 Fibo 的后台：{why}",
            host = &spec.host,
            why
        )));
    }
    Ok((path.to_owned(), home.to_owned(), shell.to_owned(), machine))
}

/// What `fibo remote dial --about` says of a machine paired through the relay: as
/// [`remote_socket`] answers, with no socket path there, since the relay reaches it.
fn relay_about(spec: &Spec) -> io::Result<(String, String, String, (String, String))> {
    let mut command = Command::new(crate::lifecycle::host_program()?);
    command
        .args(["remote", "dial", "--machine", &spec.host, "--about"])
        .stdin(Stdio::null());
    fibo_platform::process::no_console(&mut command);
    let out = command.output()?;
    #[derive(serde::Deserialize)]
    struct About {
        home: String,
        shell: String,
        machine: String,
        name: String,
    }
    match serde_json::from_slice::<About>(&out.stdout) {
        Ok(a) if out.status.success() => Ok((String::new(), a.home, a.shell, (a.machine, a.name))),
        _ => {
            let why = String::from_utf8_lossy(&out.stderr);
            let why = why.trim().trim_start_matches("fibo: ");
            Err(io::Error::other(t!(
                "经中继连不上 {host}：{why}",
                host = &spec.host,
                why
            )))
        }
    }
}

/// What keeps the forward: `ssh -L` to its socket, or `fibo remote dial` through the relay.
fn forward_argv(forward: &Forward) -> io::Result<Vec<String>> {
    if forward.spec.via == Via::Relay {
        let program = crate::lifecycle::host_program()?;
        return Ok(vec![
            program.display().to_string(),
            "remote".into(),
            "dial".into(),
            "--machine".into(),
            forward.spec.host.clone(),
            "--listen".into(),
            forward.local.display().to_string(),
        ]);
    }
    let mut argv = forward.spec.ssh_options("-N");
    argv.extend([
        // A workspace is JSON and goes whole on each change: compressed, a slow link
        // carries it several times faster.
        "-o".into(),
        "Compression=yes".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        "StreamLocalBindUnlink=yes".into(),
        "-L".into(),
        format!("{}:{}", forward.local.display(), forward.remote),
        forward.spec.host.clone(),
    ]);
    Ok(argv)
}

/// Runs the `ssh -L` and starts it again after it ends, waiting longer each time it ends
/// quickly, until `stop` takes it.
fn supervise(forward: &Forward) {
    let mut backoff = Duration::from_millis(500);
    while !forward.stopped.load(Ordering::Acquire) {
        let argv = match forward_argv(forward) {
            Ok(argv) => argv,
            Err(e) => {
                eprintln!("forward {}: {e}", forward.spec.host);
                std::thread::sleep(BACKOFF_MAX);
                continue;
            }
        };
        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // A window that crashes leaves its forwards behind; TERM must still end them.
        fibo_platform::process::takes_term(&mut command);
        fibo_platform::process::no_console(&mut command);
        let child = command.spawn();
        let started = Instant::now();
        match child {
            Ok(mut child) => {
                let mut slot = forward.child.lock().unwrap();
                if forward.stopped.load(Ordering::Acquire) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                *slot = Some(child);
            }
            Err(e) => {
                eprintln!("ssh -L {}: {e}", forward.spec.host);
                std::thread::sleep(BACKOFF_MAX);
                continue;
            }
        }
        // Waits for it to end without holding the lock, so `stop` can take it meanwhile.
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let mut slot = forward.child.lock().unwrap();
            let Some(child) = slot.as_mut() else {
                return;
            };
            if !matches!(child.try_wait(), Ok(None)) {
                *slot = None;
                break;
            }
        }
        backoff = if started.elapsed() > BACKOFF_MAX {
            Duration::from_millis(500)
        } else {
            (backoff * 2).min(BACKOFF_MAX)
        };
        std::thread::sleep(backoff);
    }
}

/// The first forward's running `ssh -L`, for the evidence that drops it and sees it back.
#[cfg(feature = "visual-test")]
pub fn visual_link_pid() -> Option<u32> {
    let forwards = FORWARDS.lock().unwrap();
    (forwards.first()?.child.lock().unwrap().as_ref()).map(Child::id)
}
/// Ends that `ssh -L` as a dropped network would; its supervisor starts another.
#[cfg(feature = "visual-test")]
pub fn visual_drop_link() {
    let forwards = FORWARDS.lock().unwrap();
    if let Some(forward) = forwards.first()
        && let Some(child) = forward.child.lock().unwrap().as_mut()
    {
        let _ = child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_one_short_folder_of_its_own() {
        let alias = dir_name("alice-server");
        assert_ne!(
            dir_name("alice@server"),
            alias,
            "two destinations, one socket"
        );
        assert_eq!(alias, dir_name("alice-server"), "the same name every run");
        assert!(!dir_name("../x").contains(['/', '.']));
        // The longest path under it, the window's lock socket, stays under macOS's 104
        // bytes for a long host under a usual home.
        let root = Path::new("/Users/someone-long/.local/share/mooring-workbench/m")
            .join(dir_name(
                "lifcc@agent-os.tailc3193d.ts.net.example.internal",
            ))
            .join("client/client.sock");
        assert!(root.as_os_str().len() < 104, "{}", root.display());
    }
    #[test]
    fn the_socket_is_asked_for_then_found_as_an_older_fibo_finds_it() {
        let spec = Spec {
            host: "far".into(),
            path: Some("/opt/fibo".into()),
            quiet: false,
            via: Via::Ssh,
        };
        let argv = spec.ssh(&format!("{}; \"$p\" socket", spec.find_program()));
        assert_eq!(argv[0], "ssh");
        assert!(argv.contains(&"BatchMode=yes".to_owned()));
        assert!(argv.last().unwrap().starts_with("p='/opt/fibo'"));
    }
}
