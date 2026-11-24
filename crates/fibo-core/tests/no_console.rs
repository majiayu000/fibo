//! The window and the host have no console, so on Windows every command-line child they start
//! without `CREATE_NO_WINDOW` opens one: Windows 11 shows it as a Windows Terminal window that
//! flashes and closes. `fibo hooks status` run that way on each window activation stole focus
//! and ran again, every half second (issue #485).
//!
//! So a source file that starts a process outside its tests also calls
//! `process::no_console` (or `detach`, which has no console either), unless it is listed
//! below as only ever running on macOS or Linux.
use std::path::{Path, PathBuf};

/// Files whose processes run only on macOS or Linux, with why.
const UNIX_ONLY: &[&str] = &[
    // /bin/ps and /usr/sbin/ioreg, behind the macOS diagnostics.
    "crates/fibo-host/src/diagnostics.rs",
    // /bin/ps.
    "crates/fibo-host/src/host/client/modes.rs",
    // /usr/bin/pgrep, for hosts older than the pid file.
    "crates/fibo-app/src/lifecycle.rs",
    // /usr/bin/pkill.
    "crates/fibo-app/src/main.rs",
    // Only a zsh or bash login shell from $SHELL.
    "crates/fibo-app/src/workbench/forms.rs",
    // /bin/sh.
    "crates/fibo-platform/src/shell/mod.rs",
    // Only the plugin-facing test client; the window and host never use it.
    "crates/fibo-remote/src/client.rs",
    // /usr/bin/security, the macOS keychain.
    "crates/fibo-core/src/secret.rs",
    // screencapture, in the macOS screenshot harness.
    "crates/fibo-app/src/settings_evidence/icons.rs",
    // Inside the relay's test module.
    "crates/fibo-host/src/relay/regressions.rs",
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The part of a file before its test module.
fn outside_tests(text: &str) -> &str {
    text.find("\n#[cfg(test)]").map_or(text, |at| &text[..at])
}

#[test]
fn every_windows_child_process_starts_without_a_console() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    sources(&root.join("crates"), &mut files);
    let mut missing = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let name = path.file_name().unwrap().to_string_lossy();
        if rel.contains("fibo-relay")
            || name == "build.rs"
            || name.ends_with("tests.rs")
            || name.ends_with("_test.rs")
            || name == "macos.rs"
            || name == "unix.rs"
            || UNIX_ONLY.contains(&rel.as_str())
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let code = outside_tests(&text);
        if code.contains("Command::new(")
            && !code.contains("no_console(")
            && !code.contains("detach(")
            && !code.contains("CREATE_NO_WINDOW")
        {
            missing.push(rel);
        }
    }
    assert!(
        missing.is_empty(),
        "these start a process without process::no_console, so on Windows each one flashes a \
         terminal window; call it on the Command, or list the file in UNIX_ONLY with why: {missing:#?}"
    );
}
