//! `claude-quota` copies Claude Code's settings aside before changing them, and those
//! settings can hold API keys. Run for real: the plugin is JXA, so macOS only.
#![cfg(target_os = "macos")]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn the_settings_backup_is_private_and_an_old_one_is_made_private() {
    let root = std::env::temp_dir().join(format!("mooring-quota-{}", std::process::id()));
    let (config, data) = (root.join("claude"), root.join("data"));
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let settings = config.join("settings.json");
    std::fs::write(&settings, r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"sk-test"}}"#).unwrap();
    std::fs::set_permissions(&settings, std::fs::Permissions::from_mode(0o600)).unwrap();
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/claude-quota");
    let setup = || {
        let out = Command::new("/usr/bin/osascript")
            .args(["-l", "JavaScript", "settings.js", "setup"])
            .current_dir(&plugin)
            .env("HOME", &root)
            .env("CLAUDE_CONFIG_DIR", &config)
            .env("MOORING_PLUGIN_DIR", &plugin)
            .env("MOORING_PLUGIN_DATA", &data)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    };

    setup();
    let backup = data.join("settings.before.json");
    assert_eq!(mode(&backup), 0o600);
    assert_eq!(mode(&settings), 0o600);
    assert!(
        std::fs::read_to_string(&backup)
            .unwrap()
            .contains("sk-test")
    );

    // One left by a version that wrote it readable, with the status line already on.
    std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o644)).unwrap();
    setup();
    assert_eq!(mode(&backup), 0o600);
    std::fs::remove_dir_all(&root).unwrap();
}
