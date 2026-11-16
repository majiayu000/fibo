//! Read-only repository inspection, including git run by plugin shell commands.
use std::{
    path::Path,
    process::{Command, Stdio},
};

/// Command-scope config wins over repository config and inherited config parameters.
/// Apply after a plugin's environment, so the caller cannot accidentally undo it.
pub fn configure(command: &mut Command) {
    command
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env("GIT_CONFIG_COUNT", "3")
        .env("GIT_CONFIG_KEY_0", "core.fsmonitor")
        .env("GIT_CONFIG_VALUE_0", "false")
        .env("GIT_CONFIG_KEY_1", "safe.bareRepository")
        .env("GIT_CONFIG_VALUE_1", "explicit")
        .env("GIT_CONFIG_KEY_2", "core.quotepath")
        .env("GIT_CONFIG_VALUE_2", "false")
        .env("GIT_OPTIONAL_LOCKS", "0");
}

pub fn command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    configure(&mut command);
    command
}

#[cfg(test)]
mod tests {
    #![cfg(unix)]
    use super::*;

    #[test]
    fn native_and_plugin_inspection_never_run_repository_fsmonitor() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("fibo-git-safe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            command(&dir)
                .args(["init", "-q"])
                .status()
                .unwrap()
                .success()
        );
        let script = dir.join("watcher.sh");
        let marker = dir.join("executed");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            command(&dir)
                .args(["config", "core.fsmonitor"])
                .arg(&script)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["status", "--porcelain"])
                .env_remove("GIT_CONFIG_PARAMETERS")
                .env("GIT_CONFIG_COUNT", "0")
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(marker.exists(), "fixture must run without the protection");
        std::fs::remove_file(&marker).unwrap();
        assert!(
            command(&dir)
                .args(["status", "--porcelain"])
                .output()
                .unwrap()
                .status
                .success()
        );
        let mut shell = Command::new("sh");
        shell
            .arg("-c")
            .arg("git status --porcelain")
            .current_dir(&dir)
            .env(
                "GIT_CONFIG_PARAMETERS",
                format!("'core.fsmonitor={}'", script.display()),
            )
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.fsmonitor")
            .env("GIT_CONFIG_VALUE_0", &script);
        configure(&mut shell);
        assert!(shell.output().unwrap().status.success());
        let mut plugin = crate::plugin_command::Run::shell(
            "git status --porcelain",
            std::time::Duration::from_secs(5),
            crate::plugin_command::Leftovers::End,
        );
        plugin.cwd = Some(&dir);
        plugin.env = vec![
            (
                "GIT_CONFIG_PARAMETERS".into(),
                format!("'core.fsmonitor={}'", script.display()),
            ),
            ("GIT_CONFIG_COUNT".into(), "1".into()),
            ("GIT_CONFIG_KEY_0".into(), "core.fsmonitor".into()),
            ("GIT_CONFIG_VALUE_0".into(), script.display().to_string()),
        ];
        assert!(crate::plugin_command::run(plugin).unwrap().status.success());
        assert!(!marker.exists(), "repository fsmonitor must never run");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn automatic_pane_facts_does_not_run_worktree_clean_filters() {
        let dir = std::env::temp_dir().join(format!("fibo-git-clean-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let output = command(&dir).args(args).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("tracked.txt"), "before\n").unwrap();
        std::fs::write(dir.join(".gitattributes"), "*.txt filter=fixture\n").unwrap();
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "fixture",
        ]);
        std::fs::write(dir.join("tracked.txt"), "staged\n").unwrap();
        git(&["add", "tracked.txt"]);
        let marker = dir.join("filter-executed");
        git(&[
            "config",
            "filter.fixture.clean",
            &format!("touch '{}'; cat", marker.display()),
        ]);
        // Same-length changes force status to inspect contents, not only stat information.
        std::fs::write(dir.join("tracked.txt"), "STAGED\n").unwrap();
        git(&["status", "--porcelain"]);
        assert!(
            marker.exists(),
            "fsmonitor protection alone does not prevent clean filters"
        );
        std::fs::remove_file(&marker).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../plugins/pane-facts/decorations.json"))
                .unwrap();
        let text = manifest["decorations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == "changes")
            .unwrap()["command"]
            .as_str()
            .unwrap();
        let mut spec = crate::plugin_command::Run::shell(
            text,
            std::time::Duration::from_secs(5),
            crate::plugin_command::Leftovers::End,
        );
        spec.cwd = Some(&dir);
        let result = crate::plugin_command::run(spec).unwrap();
        assert!(result.status.success(), "{}", result.said());
        assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "已暂存 1");
        assert!(
            !marker.exists(),
            "automatic decoration must not inspect worktree content"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
