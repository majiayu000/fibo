//! A plugin setting too sensitive for a file (`"secret": true`), kept in the macOS login
//! keychain. Everything goes through `/usr/bin/security`, for two reasons. Its entries list
//! that tool as allowed, so the app, the host and the `fibo` command, separately signed
//! and replaced by every update, all reach them without a system prompt each time. And a
//! value is written through its standard input, so it is never on a command line where
//! `ps` would show it.
//!
//! The value is stored as hex: `security -w` prints anything not plain ASCII as hex without
//! saying so, and this way what comes back is always what went in.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const TOOL: &str = "/usr/bin/security";

fn service(plugin: &str) -> String {
    format!("mooring.plugin.{plugin}")
}
/// A keychain other than the login one: an isolated run's, set with `MOORING_KEYCHAIN`.
#[cfg(not(test))]
fn keychain() -> Option<PathBuf> {
    std::env::var_os("MOORING_KEYCHAIN").map(PathBuf::from)
}

fn to_hex(value: &str) -> String {
    value.bytes().map(|b| format!("{b:02x}")).collect()
}
fn from_hex(text: &str) -> Option<String> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect();
    String::from_utf8(bytes?).ok()
}

/// Stores it through the tool's own prompt, so the value is on no command line. An existing
/// entry is removed first: updating one in place (`-U`) with an access list makes macOS put
/// up a password dialog, which a host running in the background must never do.
fn store(chain: Option<&PathBuf>, plugin: &str, key: &str, value: &str) -> Result<(), String> {
    remove(chain, plugin, key);
    // Plugin ids and keys are checked to be plain words, and the value is hex: no quoting.
    let mut line = format!(
        "add-generic-password -s {} -a {key} -T {TOOL} -w {}",
        service(plugin),
        to_hex(value)
    );
    if let Some(chain) = chain {
        line.push_str(&format!(" \"{}\"", chain.display()));
    }
    line.push('\n');
    let mut child = Command::new(TOOL)
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| t!("运行 security 失败：{e}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(line.as_bytes())
            .map_err(|e| t!("写不进钥匙串：{e}", e))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| t!("写不进钥匙串：{e}", e))?;
    // `security -i` exits 0 whatever its commands did: what it said is the only sign.
    let said = String::from_utf8_lossy(&out.stderr);
    if out.status.success() && !said.contains("security:") && !said.contains("error") {
        Ok(())
    } else {
        Err(t!(
            "写不进钥匙串：{e}",
            e = crate::content::short(said.trim(), 200)
        ))
    }
}

fn fetch(chain: Option<&PathBuf>, plugin: &str, key: &str) -> Option<String> {
    let mut cmd = Command::new(TOOL);
    cmd.args([
        "find-generic-password",
        "-s",
        &service(plugin),
        "-a",
        key,
        "-w",
    ]);
    if let Some(chain) = chain {
        cmd.arg(chain);
    }
    let out = cmd
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    from_hex(&String::from_utf8_lossy(&out.stdout))
}

fn remove(chain: Option<&PathBuf>, plugin: &str, key: &str) {
    let mut cmd = Command::new(TOOL);
    cmd.args(["delete-generic-password", "-s", &service(plugin), "-a", key]);
    if let Some(chain) = chain {
        cmd.arg(chain);
    }
    let _ = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Tests of everything above this module use a table: they must not touch a real keychain.
#[cfg(test)]
mod fake {
    use std::collections::HashMap;
    use std::sync::Mutex;
    pub static TABLE: Mutex<Option<HashMap<(String, String), String>>> = Mutex::new(None);
    pub fn with<R>(f: impl FnOnce(&mut HashMap<(String, String), String>) -> R) -> R {
        let mut table = TABLE.lock().unwrap_or_else(|e| e.into_inner());
        f(table.get_or_insert_with(HashMap::new))
    }
}

pub fn set(plugin: &str, key: &str, value: &str) -> Result<(), String> {
    #[cfg(test)]
    {
        fake::with(|t| t.insert((plugin.to_owned(), key.to_owned()), value.to_owned()));
        Ok(())
    }
    #[cfg(not(test))]
    store(keychain().as_ref(), plugin, key, value)
}
pub fn get(plugin: &str, key: &str) -> Option<String> {
    #[cfg(test)]
    {
        fake::with(|t| t.get(&(plugin.to_owned(), key.to_owned())).cloned())
    }
    #[cfg(not(test))]
    fetch(keychain().as_ref(), plugin, key)
}
pub fn delete(plugin: &str, key: &str) {
    #[cfg(test)]
    fake::with(|t| {
        t.remove(&(plugin.to_owned(), key.to_owned()));
    });
    #[cfg(not(test))]
    remove(keychain().as_ref(), plugin, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against a real keychain created for the test and thrown away after. Ignored by
    /// default: `security create-keychain` adds to the user's search list for its duration.
    /// `cargo test -p fibo-core secret -- --ignored`
    #[test]
    #[ignore = "creates a real temporary keychain"]
    fn a_value_comes_back_from_a_real_keychain_exactly() {
        /// Deletes the temporary keychain however the test ends.
        struct Temporary(PathBuf);
        impl Drop for Temporary {
            fn drop(&mut self) {
                let _ = Command::new(TOOL)
                    .arg("delete-keychain")
                    .arg(&self.0)
                    .status();
            }
        }
        let chain = std::env::temp_dir().join(format!("kl-secret-{}.keychain", std::process::id()));
        let run = |args: &[&str]| {
            assert!(
                Command::new(TOOL).args(args).status().unwrap().success(),
                "{args:?}"
            );
        };
        let name = chain.to_str().unwrap();
        run(&["create-keychain", "-p", "test", name]);
        let _temporary = Temporary(chain.clone());
        run(&["unlock-keychain", "-p", "test", name]);
        // Each one replaces the last: replacing must work without a dialog.
        for value in [
            "sk-abc",
            "带 空格 和 '引号' \"双引号\" $(rm -rf /)",
            "换\n行",
            "😀",
        ] {
            let at = std::time::Instant::now();
            store(Some(&chain), "p", "token", value).unwrap();
            assert!(
                at.elapsed() < std::time::Duration::from_secs(3),
                "a dialog?"
            );
            assert_eq!(fetch(Some(&chain), "p", "token").as_deref(), Some(value));
        }
        remove(Some(&chain), "p", "token");
        assert_eq!(fetch(Some(&chain), "p", "token"), None);
    }

    #[test]
    fn hex_round_trips_whatever_is_in_it() {
        for value in ["", "a", "带 '引号'\n换行 😀"] {
            assert_eq!(from_hex(&to_hex(value)).as_deref(), Some(value));
        }
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("zz"), None);
    }
}
