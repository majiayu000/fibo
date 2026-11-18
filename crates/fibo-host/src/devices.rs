//! Phones paired with this machine and what each may do, and the log of what they did.
//! Both files sit next to the socket, readable only by this user:
//! - `remote-devices.json`: one entry per phone, its own key, its role.
//! - `remote-devices.lock`: held by whoever is changing the phones, host or command line.
//! - `remote-log.jsonl`: connections, refusals, and every send, keys and answer.
//! - `remote-code`: the machine's code its phones find it by on a relay.
//! - `remote-code-retired`: token hashes of the codes `--new-code` replaced.
//! - `remote-routes-revoked`: route hashes of phones unpaired, newest last.
//! - `machine-id`: this machine's id, the same across restarts.
use fibo_remote::{link::Link, protocol::Machine, secure::Code, wire};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub use fibo_remote::role::{acts, Role};

pub fn role_label(role: Role) -> &'static str {
    match role {
        Role::View => t!("只看"),
        Role::Answer => t!("能答审批"),
        Role::Full => t!("完全控制"),
    }
}

pub fn parse_role(s: &str) -> io::Result<Role> {
    Role::parse(s).ok_or_else(|| {
        io::Error::other(t!(
            "--role 只能是 view（只看）、answer（能答审批）或 full（完全控制），不是 {s}",
            s
        ))
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    /// The key its handshake proves it holds, as the pairing link carried it.
    key: String,
    pub role: Role,
    /// Unix seconds.
    pub paired: u64,
    /// New links expire until the phone proves the channel key with its first message.
    /// Existing device records without this field already have permanent pairing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairing_expires: Option<u64>,
    /// Unix milliseconds of the last "disconnect now": sessions opened before it end.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub kicked: u64,
    /// This phone's own route code, which its pairing link carries: the relay passes
    /// only these on to the machine (stage 4). Empty for a phone paired before them,
    /// which has to pair again.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    route: String,
}

/// How long a phone "disconnected" from the computer stays out (plan stage 4).
pub const KICK_COOLDOWN_MS: u64 = 5 * 60 * 1000;

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl Device {
    pub fn pairing_expired(&self, now: u64) -> bool {
        self.pairing_expires.is_some_and(|until| now >= until)
    }
    pub fn key(&self) -> Option<Code> {
        Code::parse(&self.key)
    }
    pub fn route(&self) -> Option<Code> {
        Code::parse(&self.route)
    }
    /// The hash the relay knows this phone's route by.
    pub fn route_hash(&self) -> Option<String> {
        self.route().map(|r| wire::token_hash(&r.token()))
    }
    /// Whether a "disconnect now" at `kicked` still keeps it out at `now_ms`.
    pub fn cooling(&self, now_ms: u64) -> bool {
        self.kicked != 0 && now_ms < self.kicked + KICK_COOLDOWN_MS
    }
}

/// The route hashes of the paired phones, for the relay.
pub fn routes(devices: &[Device]) -> Vec<String> {
    let mut all: Vec<String> = devices
        .iter()
        .filter(|d| !d.pairing_expired(now()))
        .filter_map(Device::route_hash)
        .collect();
    all.truncate(wire::MAX_ROUTES);
    all
}

fn revoked_file(socket: &Path) -> PathBuf {
    socket.with_file_name("remote-routes-revoked")
}

/// Route hashes of the phones unpaired lately, newest last, at most [`wire::MAX_ROUTES`].
pub fn revoked(socket: &Path) -> Vec<String> {
    std::fs::read_to_string(revoked_file(socket))
        .unwrap_or_default()
        .lines()
        .filter(|l| wire::is_token_hash(l))
        .map(str::to_owned)
        .collect()
}

fn revocations(socket: &Path, gone: &[Device]) -> io::Result<Vec<u8>> {
    let mut list = hashes_for_update(&revoked_file(socket))?;
    list.extend(gone.iter().filter_map(Device::route_hash));
    let start = list.len().saturating_sub(wire::MAX_ROUTES);
    Ok(list[start..].join("\n").into_bytes())
}

/// These lists are line-based, unlike the JSON device file. An invalid line is not empty.
fn hashes_for_update(path: &Path) -> io::Result<Vec<String>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => {
            return Err(io::Error::new(
                e.kind(),
                t!("读不了 {path}，未改动：{e}", path = path.display(), e),
            ))
        }
    };
    text.lines()
        .map(|line| {
            if wire::is_token_hash(line) {
                Ok(line.to_owned())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    t!(
                        "{path} 格式不对，未改动：{e}",
                        path = path.display(),
                        e = "invalid token hash"
                    ),
                ))
            }
        })
        .collect()
}

/// Replaces `path` whole. The temporary name is this write's alone (a process id alone can be
/// a dead process's, whose file is still there). A write that fails removes it; a process
/// killed mid-write cannot, and [`lock`] clears what that leaves of the device list.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    let written = fibo_platform::fs::private_file()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Fixed-name temporaries builds before 11-09 wrote: the device list's held every key, the
/// code's the machine's code. Nothing writes these names now, so whoever holds [`lock`] may
/// remove them.
const OLD_TEMPORARIES: [&str; 3] = [
    "remote-devices.tmp",
    "remote-code.tmp",
    "remote-code-retired.tmp",
];

/// Whether `name` is a temporary [`write_private`] makes for `remote-devices.json`, or one an
/// older build left.
fn is_devices_temporary(name: &str) -> bool {
    if OLD_TEMPORARIES.contains(&name) {
        return true;
    }
    let Some(made) = name
        .strip_prefix("remote-devices.")
        .and_then(|n| n.strip_suffix(".tmp"))
    else {
        return false;
    };
    made.split_once('.').is_some_and(|(pid, id)| {
        !pid.is_empty()
            && pid.bytes().all(|b| b.is_ascii_digit())
            && id.len() == 32
            && id.bytes().all(|b| b.is_ascii_hexdigit())
    })
}

/// Removes temporaries of the device list left by a process killed mid-write: they hold every
/// key at that moment, those of phones since unpaired too. Only for the holder of [`lock`]:
/// `save` runs only under it, so no such file is anyone's write in progress.
fn clear_abandoned(socket: &Path) {
    let Some(entries) = socket.parent().and_then(|d| std::fs::read_dir(d).ok()) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_str().is_some_and(is_devices_temporary) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Held from reading the phones to writing them back: the host and a `fibo remote` both
/// change them, and a change made from a stale list brings back a phone just unpaired.
/// Released when dropped. flock belongs to the open file, so this excludes other threads
/// too, and whoever holds it must not ask for it again: it would wait on itself.
fn lock(socket: &Path) -> io::Result<std::fs::File> {
    let file = fibo_platform::fs::private_file()
        .create(true)
        .truncate(false)
        .write(true)
        .open(socket.with_file_name("remote-devices.lock"))?;
    // A suspended writer can hold this lock indefinitely. Bound only our wait; never
    // unlink its lock or change the device list without owning the same lock it holds.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::Error(e)) => return Err(e),
            Err(std::fs::TryLockError::WouldBlock) => {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        t!("设备文件正被另一项操作占用，请稍后重试"),
                    ));
                }
                // Writes normally hold the lock for only a few milliseconds; a long
                // polling interval lets a busy writer repeatedly win before we wake.
                std::thread::sleep(left.min(std::time::Duration::from_millis(1)));
            }
        }
    }
    clear_abandoned(socket);
    Ok(file)
}

fn file(socket: &Path) -> PathBuf {
    socket.with_file_name("remote-devices.json")
}
pub fn log_file(socket: &Path) -> PathBuf {
    socket.with_file_name("remote-log.jsonl")
}

pub fn load(socket: &Path) -> Vec<Device> {
    std::fs::read(file(socket))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// For a change that is written back: a file that cannot be understood stays as it is, since
/// saving over it would unpair every phone it holds.
fn load_for_update(socket: &Path) -> io::Result<Vec<Device>> {
    fibo_core::state_file::read_for_update(&file(socket))
}

fn save(socket: &Path, devices: &[Device]) -> io::Result<()> {
    write_private(&file(socket), &serde_json::to_vec_pretty(devices)?)
}

pub fn add(socket: &Path, name: &str, role: Role) -> io::Result<(Device, Code)> {
    let key = Code::generate()?;
    let route = Code::generate()?;
    let device = Device {
        route: route.text(),
        id: uuid::Uuid::new_v4().simple().to_string()[..12].to_owned(),
        name: name.to_owned(),
        key: key.text(),
        role,
        paired: now(),
        pairing_expires: Some(now() + 10 * 60),
        kicked: 0,
    };
    let _held = lock(socket)?;
    let mut devices = load_for_update(socket)?;
    devices.push(device.clone());
    save(socket, &devices)?;
    Ok((device, key))
}

/// A valid encrypted message proves the phone holds the negotiated channel key.
/// A replayed handshake alone must not keep a pairing link alive.
pub fn confirm_pairing(socket: &Path, id: &str) -> io::Result<()> {
    let _held = lock(socket)?;
    let mut devices = load_for_update(socket)?;
    let device = devices
        .iter_mut()
        .find(|d| d.id == id)
        .ok_or_else(|| io::Error::other("device no longer paired"))?;
    if device.pairing_expired(now()) {
        return Err(io::Error::other("pairing link expired"));
    }
    if device.pairing_expires.take().is_some() {
        save(socket, &devices)?;
    }
    Ok(())
}

/// The one device whose id starts with `needle`, or whose name is `needle`.
fn find(devices: &[Device], needle: &str) -> io::Result<usize> {
    let hits: Vec<usize> = (0..devices.len())
        .filter(|&i| devices[i].name == needle || devices[i].id.starts_with(needle))
        .collect();
    match hits[..] {
        [i] => Ok(i),
        [] => Err(io::Error::other(t!("没有配对过的设备叫 {needle}", needle))),
        _ => Err(io::Error::other(t!(
            "{needle} 匹配到 {n} 台设备，请写编号",
            needle,
            n = hits.len()
        ))),
    }
}

/// Unpairs a device.
pub fn remove(socket: &Path, needle: &str) -> io::Result<Device> {
    let _held = lock(socket)?;
    let mut devices = load_for_update(socket)?;
    let gone = devices.remove(find(&devices, needle)?);
    let revoked = revocations(socket, std::slice::from_ref(&gone))?;
    save(socket, &devices)?;
    write_private(&revoked_file(socket), &revoked)?;
    Ok(gone)
}

/// Gives a device another role; its open sessions end and it reconnects with the new one.
pub fn set_role(socket: &Path, needle: &str, role: Role) -> io::Result<Device> {
    let _held = lock(socket)?;
    let mut devices = load_for_update(socket)?;
    let i = find(&devices, needle)?;
    devices[i].role = role;
    save(socket, &devices)?;
    Ok(devices[i].clone())
}

/// Ends a device's open sessions now; it stays paired and may connect again.
pub fn kick(socket: &Path, needle: &str) -> io::Result<Device> {
    let _held = lock(socket)?;
    let mut devices = load_for_update(socket)?;
    let i = find(&devices, needle)?;
    devices[i].kicked = now_ms();
    save(socket, &devices)?;
    Ok(devices[i].clone())
}

/// Unpairs every device; each phone is told so (4401) instead of finding the machine
/// offline.
pub fn clear(socket: &Path) -> io::Result<()> {
    let _held = lock(socket)?;
    let gone = load_for_update(socket)?;
    let revoked = revocations(socket, &gone)?;
    save(socket, &[])?;
    write_private(&revoked_file(socket), &revoked)
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The log past which it is moved to `remote-log.1.jsonl`, replacing the older one.
const LOG_LIMIT: u64 = 4 << 20;

fn previous(path: &Path) -> PathBuf {
    path.with_extension("1.jsonl")
}

/// Appends one entry, stamped with the time; a log that cannot be written is not fatal.
pub fn log(path: &Path, mut entry: Value) {
    entry["at"] = json!(now());
    let line = format!("{entry}\n");
    if std::fs::metadata(path).is_ok_and(|m| m.len() + line.len() as u64 > LOG_LIMIT) {
        let _ = std::fs::rename(path, previous(path));
    }
    let _ = fibo_platform::fs::private_file()
        .append(true)
        .create(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

/// When each device last connected, from the log.
pub fn last_seen(socket: &Path) -> std::collections::HashMap<String, u64> {
    let mut seen = std::collections::HashMap::new();
    let path = log_file(socket);
    let text: String = [previous(&path), path]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect();
    for v in text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        if let (Some("connected"), Some(d), Some(at)) =
            (v["event"].as_str(), v["device"].as_str(), v["at"].as_u64())
        {
            seen.insert(d.to_owned(), at);
        }
    }
    seen
}

/// This machine's id stays the same across restarts, so a client that knows several
/// machines can tell their terminals apart. An id that cannot be kept is an error: a fresh
/// one on every call would make one computer look like several.
pub fn machine(socket: &Path) -> io::Result<Machine> {
    let file = socket.with_file_name("machine-id");
    let read = || -> io::Result<Option<String>> {
        match std::fs::read_to_string(&file) {
            Ok(text) => Ok(Some(text.trim().to_owned()).filter(|s| !s.is_empty())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    };
    let id = match read()? {
        Some(id) => id,
        // Made under the lock and looked for again there: two processes starting at once
        // would otherwise each hand out an id and only the later one be kept.
        None => {
            let _held = lock(socket)?;
            match read()? {
                Some(id) => id,
                None => {
                    let id = uuid::Uuid::new_v4().to_string();
                    write_private(&file, id.as_bytes())?;
                    id
                }
            }
        }
    };
    let name = std::fs::read_to_string(name_file(socket))
        .ok()
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(fibo_platform::system::host_name);
    Ok(Machine {
        id,
        name,
        agent_names: fibo_core::agent::Agent::all()
            .into_iter()
            .map(|a| (a.key().to_lowercase(), a.name().to_owned()))
            .collect(),
    })
}

/// The name phones show for this machine when its owner gave one: a host name like
/// `appledeMacBook-Pro.local` is long for a phone, and changing it needs root (2026-11-09).
fn name_file(socket: &Path) -> PathBuf {
    socket.with_file_name("machine-name")
}

/// Gives this machine the name phones show, or with an empty one goes back to the host name.
pub fn set_machine_name(socket: &Path, name: &str) -> io::Result<Machine> {
    let name = name.trim();
    let file = name_file(socket);
    if name.is_empty() {
        match std::fs::remove_file(&file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    } else {
        let _ = std::fs::remove_file(&file);
        write_private(&file, name.as_bytes())?;
    }
    machine(socket)
}

/// The machine's pairing code, made on first use, readable only by this user.
/// A replaced code's token hash goes to `remote-code-retired`, so the relay can tell a
/// phone still on it to pair again.
pub fn machine_code(socket: &Path, new: bool) -> io::Result<Code> {
    let file = socket.with_file_name("remote-code");
    let read = || {
        std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| Code::parse(&t))
    };
    if let (Some(code), false) = (read(), new) {
        return Ok(code);
    }
    // Made, replaced and retired under the lock: several first pairings at once used to
    // collide on one temporary name (`File exists`), and two `--new-code` could each retire
    // from a list the other was rewriting. Looked for again once held: whoever came first
    // made the code everyone else is to use.
    let _held = lock(socket)?;
    match read() {
        Some(code) if !new => return Ok(code),
        Some(old) => retire(socket, &old)?,
        None => (),
    }
    let code = Code::generate()?;
    write_private(&file, code.text().as_bytes())?;
    Ok(code)
}

fn retired_file(socket: &Path) -> PathBuf {
    socket.with_file_name("remote-code-retired")
}

/// Token hashes of the codes this machine used before, newest last, at most
/// [`wire::MAX_RETIRED`]; the relay hears them on register.
pub fn retired(socket: &Path) -> Vec<String> {
    std::fs::read_to_string(retired_file(socket))
        .unwrap_or_default()
        .lines()
        .filter(|l| wire::is_token_hash(l))
        .map(str::to_owned)
        .collect()
}

fn retire(socket: &Path, old: &Code) -> io::Result<()> {
    let mut list = hashes_for_update(&retired_file(socket))?;
    list.push(wire::token_hash(&old.token()));
    let start = list.len().saturating_sub(wire::MAX_RETIRED);
    write_private(
        &retired_file(socket),
        (list[start..].join("\n") + "\n").as_bytes(),
    )
}

/// Pairs one more phone: its device entry, and the link its QR code carries.
pub fn pair_link(
    socket: &Path,
    relay: &str,
    name: &str,
    role: Role,
) -> io::Result<(Device, String)> {
    Link::websocket_base(relay).map_err(io::Error::other)?;
    // Made now so the machine has one to register with; the phone never sees it.
    machine_code(socket, false)?;
    let machine = machine(socket)?;
    let (device, key) = add(socket, name, role)?;
    let link = Link {
        relay: relay.to_owned(),
        code: device.route().expect("a new device has a route"),
        device: device.id.clone(),
        key,
        machine: machine.id,
        name: machine.name,
    }
    .text();
    Ok((device, link))
}

#[cfg(test)]
mod race_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_file_that_cannot_be_read_is_never_saved_over() {
        let socket = std::env::temp_dir()
            .join(format!("kl-devices-bad-{}", uuid::Uuid::new_v4()))
            .join("host.sock");
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let bad = r#"[{"id":"cut off"#;
        std::fs::write(file(&socket), bad).unwrap();
        assert!(add(&socket, "phone", Role::Full).is_err());
        assert!(remove(&socket, "phone").is_err());
        assert!(set_role(&socket, "phone", Role::View).is_err());
        assert!(kick(&socket, "phone").is_err());
        assert!(clear(&socket).is_err());
        assert_eq!(std::fs::read_to_string(file(&socket)).unwrap(), bad);
    }

    #[test]
    fn devices_are_added_found_removed_and_kept_private() {
        let dir = std::env::temp_dir().join(format!("kl-dev-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("workbench.sock");
        let (a, key) = add(&socket, "iPhone", Role::Full).unwrap();
        let (b, _) = add(&socket, "iPad", Role::View).unwrap();
        assert_eq!(load(&socket)[0].key(), Some(key));
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(file(&socket))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(remove(&socket, "nope").is_err());
        assert_eq!(
            set_role(&socket, "iPad", Role::Answer).unwrap().role,
            Role::Answer
        );
        assert_eq!(load(&socket)[1].role, Role::Answer);
        assert_ne!(a.route_hash(), b.route_hash(), "each phone its own route");
        assert_eq!(routes(&load(&socket)).len(), 2);
        assert_eq!(remove(&socket, &b.id[..6]).unwrap().name, "iPad");
        assert_eq!(revoked(&socket), [b.route_hash().unwrap()]);
        assert_eq!(routes(&load(&socket)), [a.route_hash().unwrap()]);
        assert_eq!(remove(&socket, "iPhone").unwrap().id, a.id);
        assert!(load(&socket).is_empty());

        let path = log_file(&socket);
        log(&path, json!({ "event": "connected", "device": a.id }));
        assert!(last_seen(&socket).contains_key(&a.id));
        // Past the limit the log moves aside once; what it said is still read.
        std::fs::write(&path, vec![b'\n'; LOG_LIMIT as usize]).unwrap();
        log(&path, json!({ "event": "connected", "device": b.id }));
        assert!(std::fs::metadata(&path).unwrap().len() < 200);
        assert!(previous(&path).exists());
        assert!(last_seen(&socket).contains_key(&b.id));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clearing_revokes_every_route() {
        let dir = std::env::temp_dir().join(format!("kl-clr-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("workbench.sock");
        let (a, _) = add(&socket, "iPhone", Role::Full).unwrap();
        let (b, _) = add(&socket, "iPad", Role::View).unwrap();
        clear(&socket).unwrap();
        assert!(load(&socket).is_empty());
        assert_eq!(
            revoked(&socket),
            [a.route_hash().unwrap(), b.route_hash().unwrap()]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_code_is_kept_private_and_only_replaced_on_request() {
        let dir = std::env::temp_dir().join(format!("kl-code-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("workbench.sock");
        let a = machine_code(&socket, false).unwrap();
        assert_eq!(machine_code(&socket, false).unwrap(), a);
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("remote-code"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(retired(&socket).is_empty());
        let b = machine_code(&socket, true).unwrap();
        assert_ne!(a, b);
        assert_eq!(machine_code(&socket, false).unwrap(), b);
        // The replaced code is remembered by its token hash only, newest last, capped.
        assert_eq!(retired(&socket), [wire::token_hash(&a.token())]);
        for _ in 0..wire::MAX_RETIRED {
            machine_code(&socket, true).unwrap();
        }
        let list = retired(&socket);
        assert_eq!(list.len(), wire::MAX_RETIRED);
        assert!(!list.contains(&wire::token_hash(&a.token())));
        assert!(list.contains(&wire::token_hash(&b.token())));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_machine_id_is_kept_or_its_loss_reported() {
        let dir = std::env::temp_dir().join(format!("kl-mid-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("workbench.sock");
        let a = machine(&socket).unwrap();
        assert_eq!(machine(&socket).unwrap().id, a.id);
        // A name its owner gives replaces the host name, keeps the id, and clears back.
        assert_eq!(set_machine_name(&socket, "  apple ").unwrap().name, "apple");
        assert_eq!(machine(&socket).unwrap().id, a.id);
        assert_eq!(set_machine_name(&socket, "").unwrap().name, a.name);
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("machine-id"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        // Where the id cannot be written, say so instead of handing out a new one each time.
        // Root writes anywhere (a container, say), so there it has nothing to report.
        if unsafe { libc::geteuid() } == 0 {
            std::fs::remove_dir_all(dir).unwrap();
            return;
        }
        std::fs::remove_file(dir.join("machine-id")).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let unwritable = machine(&socket);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(unwritable.is_err(), "{unwritable:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unreadable_revocation_lists_are_not_replaced() {
        let dir = std::env::temp_dir().join(format!("fibo-revoked-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("host.sock");
        let (device, _) = add(&socket, "phone", Role::Full).unwrap();
        machine_code(&socket, false).unwrap();
        for bad in ["broken hash", "abc\n"] {
            std::fs::write(revoked_file(&socket), bad).unwrap();
            assert!(remove(&socket, &device.id).is_err());
            assert!(clear(&socket).is_err());
            assert_eq!(load(&socket).len(), 1);
            assert_eq!(std::fs::read_to_string(revoked_file(&socket)).unwrap(), bad);
            std::fs::write(retired_file(&socket), bad).unwrap();
            assert!(machine_code(&socket, true).is_err());
            assert_eq!(std::fs::read_to_string(retired_file(&socket)).unwrap(), bad);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unused_links_expire_but_confirmed_and_existing_devices_stay_paired() {
        let dir = std::env::temp_dir().join(format!("fibo-pair-expiry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("host.sock");
        let (mut device, _) = add(&socket, "phone", Role::Full).unwrap();
        let deadline = device.pairing_expires.unwrap();
        assert!(!device.pairing_expired(deadline - 1));
        assert!(device.pairing_expired(deadline));
        device.pairing_expires = Some(now() - 1);
        save(&socket, std::slice::from_ref(&device)).unwrap();
        assert!(confirm_pairing(&socket, &device.id).is_err());
        assert!(routes(&load(&socket)).is_empty());
        device.pairing_expires = Some(now() + 600);
        save(&socket, std::slice::from_ref(&device)).unwrap();
        confirm_pairing(&socket, &device.id).unwrap();
        assert!(!load(&socket)[0].pairing_expired(u64::MAX));
        let mut old = serde_json::to_value(&device).unwrap();
        old.as_object_mut().unwrap().remove("pairing_expires");
        let old: Device = serde_json::from_value(old).unwrap();
        assert!(!old.pairing_expired(u64::MAX));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
