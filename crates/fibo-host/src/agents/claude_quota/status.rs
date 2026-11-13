//! Claude Code runs this as its status line. It keeps the payload when it carries the account's
//! limits (API-key sign-ins and the first render of a session have none) and each conversation's
//! own copy for how full its context is, then hands the same payload to the status line set
//! before, which prints as ever. A reading it can't save must not cost the user that line.
use fibo_platform::fs::{create_private_dir, write_private};
use serde_json::Value;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

/// A conversation not heard from for this long no longer needs its reading.
const CONTEXTS_KEPT_FOR: Duration = Duration::from_secs(7 * 86_400);

pub(super) fn run(data: std::path::PathBuf) -> io::Result<()> {
    let mut payload = Vec::new();
    io::stdin().read_to_end(&mut payload)?;
    if let Ok(doc) = serde_json::from_slice::<Value>(&payload) {
        let _ = save(&data, &payload, &doc);
    }
    hand_on(&data, &payload);
    Ok(())
}

fn save(data: &Path, payload: &[u8], doc: &Value) -> io::Result<()> {
    let limits = doc["rate_limits"].as_object();
    if limits.is_some_and(|l| l.contains_key("five_hour") || l.contains_key("seven_day")) {
        write_private(&data.join("statusline.json"), payload)?;
    }
    // The file above holds only the conversation that spoke last.
    if let (Some(id), true) = (
        doc["session_id"].as_str().filter(|id| plain_id(id)),
        doc.get("context_window").is_some(),
    ) {
        let contexts = data.join("contexts");
        create_private_dir(&contexts)?;
        let file = contexts.join(format!("{id}.json"));
        // One this session has not written yet clears out the old.
        if !file.exists() {
            forget_old(&contexts);
        }
        write_private(&file, payload)?;
    }
    Ok(())
}

/// An id that can't leave the folder or read as anything but a name.
fn plain_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn forget_old(contexts: &Path) {
    let Ok(entries) = std::fs::read_dir(contexts) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let old = (entry.metadata().and_then(|m| m.modified()).ok())
            .and_then(|at| now.duration_since(at).ok())
            .is_some_and(|age| age > CONTEXTS_KEPT_FOR);
        if old && path.extension().is_some_and(|e| e == "json") {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// `original.sh` is the user's own command line, kept by `setup`; a shell is what it is for.
fn hand_on(data: &Path, payload: &[u8]) {
    let original = data.join("original.sh");
    if std::fs::metadata(&original).map_or(true, |m| m.len() == 0) {
        return;
    }
    let Ok(mut child) = Command::new(fibo_platform::shell::sh())
        .arg(&original)
        .stdin(Stdio::piped())
        .spawn()
    else {
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload);
    }
    let _ = child.wait();
}
