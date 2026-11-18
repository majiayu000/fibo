//! The lock on the phones' files, and how long a change waits for it.
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a waiter gives one holder before it gives up on it.
const STALLED: Duration = Duration::from_secs(5);

/// Held from reading the phones to writing them back: the host and a `fibo remote` both
/// change them, and a change made from a stale list brings back a phone just unpaired.
/// Released when dropped. flock belongs to the open file, so this excludes other threads
/// too, and whoever holds it must not ask for it again: it would wait on itself.
pub(super) fn lock(socket: &Path) -> io::Result<std::fs::File> {
    let mut file = fibo_platform::fs::private_file()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(socket.with_file_name("remote-devices.lock"))?;
    // A suspended writer can hold this lock indefinitely. Bound only the wait on one
    // holder: every holder stamps the file, so a lock that keeps changing hands is busy,
    // not stalled, and the wait starts over. try_lock does not queue, and a writer that
    // releases and asks again at once wins over pollers, so a busy lock can stay out of
    // reach for longer than any one holder keeps it. Never unlink the lock or change the
    // device list without owning the same lock the stalled writer holds.
    let mut seen = stamp(&mut file);
    let mut since = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::Error(e)) => return Err(e),
            Err(std::fs::TryLockError::WouldBlock) => {
                let now = stamp(&mut file);
                if now != seen {
                    (seen, since) = (now, Instant::now());
                }
                let left = STALLED.saturating_sub(since.elapsed());
                if left.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        t!("设备文件正被另一项操作占用，请稍后重试"),
                    ));
                }
                // Writes normally hold the lock for only a few milliseconds; a long
                // polling interval lets a busy writer repeatedly win before we wake.
                std::thread::sleep(left.min(Duration::from_millis(1)));
            }
        }
    }
    // Not fatal: a holder that cannot stamp only looks stalled to the next waiter sooner.
    let _ = file
        .seek(SeekFrom::Start(0))
        .and_then(|_| file.write_all(uuid::Uuid::new_v4().simple().to_string().as_bytes()));
    super::clear_abandoned(socket);
    Ok(file)
}

/// The last holder's stamp; empty when none (yet), or one that cannot be read.
fn stamp(file: &mut std::fs::File) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(32);
    let read = file
        .seek(SeekFrom::Start(0))
        .and_then(|_| (&*file).take(32).read_to_end(&mut bytes));
    if read.is_err() {
        bytes.clear();
    }
    bytes
}
