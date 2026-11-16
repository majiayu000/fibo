//! The old program names an install in use still starts, kept where the packages are made.
//! Nothing in this repository calls them, so nothing else fails when one is cleaned up as
//! a leftover: the v0.1.0–0.1.2 updater restarts into `Contents/MacOS/mooring-app` after
//! it swaps the bundle (an update would end with no window and no error), terminals from
//! before the rename and Claude's statusLine written by claude-quota run
//! `Contents/MacOS/mooring`, and a Windows install's PATH and scripts run `mooring.exe`.
//! docs/decisions/2026-11-13-rename-files-fibo.md.
use std::path::Path;

fn read(path: &str) -> String {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

#[test]
fn the_mac_bundle_keeps_the_names_older_installs_start() {
    let script = read("package-macos.py");
    for link in [
        "(mac / 'mooring').symlink_to('fibo')",
        "(mac / 'mooring-app').symlink_to('fibo-app')",
    ] {
        assert!(script.contains(link), "package-macos.py lost `{link}`");
    }
}

#[test]
fn the_windows_install_keeps_the_old_command_and_drops_the_old_window() {
    let stage = read("packaging/windows/stage.py");
    assert!(
        stage.contains("bin_dir / 'mooring.exe'"),
        "stage.py no longer copies fibo.exe to mooring.exe"
    );
    // 0.1.2 installed its window as mooring-app.exe; an upgrade that leaves it there keeps
    // starting the old window from a pinned icon.
    let setup = read("packaging/windows/fibo.iss");
    assert!(
        setup.contains(r#"Name: "{app}\bin\mooring-app.exe""#),
        "fibo.iss no longer removes the window program 0.1.2 installed"
    );
}
