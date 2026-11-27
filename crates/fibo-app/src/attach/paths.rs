//! Local paths for a standalone or sidebar forward.
use std::io;
use std::path::{Path, PathBuf};

/// Where the standalone window keeps its forward and its per-host instance lock.
pub fn root(host: &str) -> io::Result<PathBuf> {
    Ok(machine_root(&runtime()?, host, None))
}

/// The sidebar's forward is private to this process. The standalone window can run
/// beside it and its existing instance lock still names the same directory.
pub fn embedded_root(host: &str) -> io::Result<PathBuf> {
    Ok(machine_root(&runtime()?, host, Some(std::process::id())))
}

fn runtime() -> io::Result<PathBuf> {
    let home = fibo_platform::dirs::home().ok_or_else(|| io::Error::other("HOME unavailable"))?;
    Ok(fibo_core::legacy::runtime_dir(&home))
}

pub(super) fn machine_root(runtime: &Path, host: &str, embedded_pid: Option<u32>) -> PathBuf {
    match embedded_pid {
        None => runtime.join("m").join(dir_name(host)),
        Some(pid) => {
            // A flat, reserved name inside the same private runtime. Eleven bytes fit
            // the old shortest host path on macOS, even when its alias had no ASCII prefix.
            let mut hash = host_hash(&format!("{pid}:{host}"));
            let mut name = String::from("_");
            for _ in 0..10 {
                name.push(char::from(
                    b"0123456789abcdefghijklmnopqrstuv"[(hash & 31) as usize],
                ));
                hash >>= 5;
            }
            runtime.join(name)
        }
    }
}
/// `user@host` or an ssh alias as one short folder name: a readable start and a hash of
/// the whole destination, so `alice@server` and `alice-server` never share a socket or a
/// lock, and the socket path stays under the 104-byte limit however long the host is.
pub(super) fn dir_name(host: &str) -> String {
    let start: String = (host.chars())
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    format!("{start}-{:08x}", host_hash(host) as u32)
}

fn host_hash(host: &str) -> u64 {
    // FNV-1a: the same name on every run and every build.
    host.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}
