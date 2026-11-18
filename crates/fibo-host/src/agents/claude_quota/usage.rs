//! One credential-scoped HTTP snapshot shared by all plugin readers. The OS lock covers
//! deciding, fetching and publishing: no overlapping fetch can publish an older answer later.
use super::{oauth, settings};
use fibo_core::t;
use fibo_platform::fs::{lock_file, write_private};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const REFRESH: u64 = 120;

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    credential: String,
    next_attempt: u64,
    #[serde(default)]
    retry_at: u64,
    #[serde(default)]
    snapshot: Option<Value>,
    #[serde(default)]
    error: String,
}

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub(super) fn run(data: &Path, manual: bool) -> io::Result<()> {
    let _ = settings::repair(data);
    let doc = collect(
        data,
        manual,
        now(),
        || oauth::credential(manual),
        oauth::fetch,
    )
    .unwrap_or_else(|_| json!({"at": 0, "error": t!("额度缓存无法读取或保存，等待重试")}));
    writeln!(io::stdout(), "{doc}")
}

fn collect(
    data: &Path,
    manual: bool,
    at: u64,
    mut credential: impl FnMut() -> Result<oauth::Credential, String>,
    fetch: impl FnOnce(&oauth::Credential) -> Result<Value, oauth::Failure>,
) -> io::Result<Value> {
    let path = data.join("http-usage.json");
    let lock = lock_file(&data.join("http-usage.lock"))?;
    // Locks are released by the OS on exit; no stale-lock deletion can create two owners.
    let held = match lock.try_lock() {
        Ok(()) => true,
        Err(std::fs::TryLockError::WouldBlock) => false,
        Err(std::fs::TryLockError::Error(e)) => return Err(e),
    };
    let identity = match credential() {
        Ok(c) => c,
        Err(error) => return Ok(json!({"at": 0, "error": error})),
    };
    let mut cache = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Cache>(&bytes).map_err(io::Error::other)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Cache::default(),
        Err(e) => return Err(e),
    };
    if cache.credential != identity.scope {
        cache = Cache {
            credential: identity.scope.clone(),
            ..Default::default()
        };
    }
    let reset = cache.snapshot.as_ref().is_some_and(|snapshot| {
        let sampled = snapshot["at"].as_u64().unwrap_or(0);
        all_windows(snapshot).any(|w| {
            w["resets_at"]
                .as_u64()
                .is_some_and(|r| r > sampled && r <= at)
        })
    });
    if !held || at < cache.retry_at || (!manual && at < cache.next_attempt && !reset) {
        return Ok(display(&cache));
    }
    // A terminated command cannot hammer the endpoint. Busy readers only use their own scope.
    cache.next_attempt = at.saturating_add(REFRESH);
    cache.retry_at = at.saturating_add(60);
    save(&path, &cache)?;
    let result = fetch(&identity);
    // A switch/rotation during HTTP invalidates its answer, including errors.
    if !credential().is_ok_and(|current| current.scope == identity.scope) {
        save(&path, &Cache::default())?;
        return Ok(json!({"at": 0, "error": t!("Claude 登录已变化，等待新账号额度")}));
    }
    match result {
        Ok(mut snapshot) => {
            snapshot["at"] = json!(at);
            cache.snapshot = Some(snapshot);
            cache.error.clear();
            cache.retry_at = 0;
        }
        Err(error) => {
            if error.discard {
                cache.snapshot = None;
            }
            cache.error = error.message;
            cache.retry_at = error.retry_at.unwrap_or(at.saturating_add(REFRESH));
            cache.next_attempt = cache.retry_at;
        }
    }
    save(&path, &cache)?;
    Ok(display(&cache))
}

fn all_windows(snapshot: &Value) -> impl Iterator<Item = &Value> {
    [snapshot.get("session"), snapshot.get("week")]
        .into_iter()
        .flatten()
        .chain(snapshot["models"].as_array().into_iter().flatten())
}

fn display(cache: &Cache) -> Value {
    let mut doc = cache.snapshot.clone().unwrap_or_else(|| json!({"at": 0}));
    if !cache.error.is_empty() {
        doc["error"] = cache.error.clone().into();
    }
    doc
}

fn save(path: &Path, cache: &Cache) -> io::Result<()> {
    write_private(path, &serde_json::to_vec(cache).map_err(io::Error::other)?)
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
