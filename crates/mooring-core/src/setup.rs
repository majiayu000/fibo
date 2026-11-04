//! What a plugin does to the computer when it is put to use and when it is taken away
//! (`setup.json`): `setup` runs when the plugin is installed, updated or turned on,
//! `teardown` when it is turned off or uninstalled. A plugin that changes another
//! program's settings (wraps Claude Code's status line, say) does it here and puts them
//! back in `teardown`, so taking the plugin away leaves nothing of it behind.
//!
//! Both run with `sh -c` in the plugin's directory. `MOORING_PLUGIN_DIR` is that
//! directory, `MOORING_PLUGIN_DATA` a directory of the plugin's own that survives updates
//! and reinstalls (`<config>/plugin-data/<id>`): the place to keep what `teardown` needs.
//! `setup` must be safe to run again on an installed plugin: an update runs it once more.
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The `setup.json` format's major version.
pub const API: u32 = 1;
pub const MAX_TIMEOUT_MS: u64 = 30_000;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    api: u32,
    pub setup: String,
    pub teardown: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}
fn default_timeout() -> u64 {
    10_000
}

/// Which of the two to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Setup,
    Teardown,
}

pub fn read(file: &Path) -> Result<Setup, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("读不了：{e}"))?;
    let s: Setup = serde_json::from_slice(&bytes).map_err(|e| crate::plugin::bad_format(&e))?;
    if s.api != API {
        return Err(format!(
            "安装步骤格式是 api {}，这个版本只认 api {API}",
            s.api
        ));
    }
    if s.setup.trim().is_empty() || s.teardown.trim().is_empty() {
        return Err("setup 和 teardown 都要写：装上时做了什么，卸载时就要能撤回".into());
    }
    if !(100..=MAX_TIMEOUT_MS).contains(&s.timeout_ms) {
        return Err(format!("timeout_ms 要在 100 到 {MAX_TIMEOUT_MS} 之间"));
    }
    Ok(s)
}

/// The report line for the settings page.
pub fn load(files: Vec<PathBuf>) -> Vec<crate::agent::Loaded> {
    files
        .into_iter()
        .map(|file| crate::agent::Loaded {
            result: read(&file).map(|_| "setup、teardown".to_owned()),
            file,
        })
        .collect()
}

/// `<plugins>/../plugin-data/<id>`: kept when the plugin is updated or reinstalled.
pub fn data_dir(plugins: &Path, id: &str) -> PathBuf {
    plugins
        .parent()
        .unwrap_or(plugins)
        .join("plugin-data")
        .join(id)
}

/// Runs `phase` of the plugin in `dir` (whose plugins directory is `plugins`), when it
/// has a `setup.json`; nothing to do is success. Blocks up to its timeout. The error
/// carries what the command printed, for the settings page.
pub fn run(plugins: &Path, dir: &Path, phase: Phase) -> Result<(), String> {
    let file = dir.join("setup.json");
    if !file.is_file() {
        return Ok(());
    }
    let p = crate::plugin::check(dir)?;
    if !p.provides.contains(&crate::plugin::Kind::Setup) {
        return Ok(());
    }
    let s = read(&file)?;
    let data = data_dir(plugins, &p.id);
    crate::plugin_context::create_private_dir(&data)
        .map_err(|e| format!("建不了 {}：{e}", data.display()))?;
    let started = Instant::now();
    let result = run_command(dir, &s, phase);
    crate::plugin_context::record(
        dir,
        match phase {
            Phase::Setup => "setup",
            Phase::Teardown => "teardown",
        },
        &result,
        started.elapsed().as_millis() as u64,
        false,
    );
    result
}

fn run_command(dir: &Path, s: &Setup, phase: Phase) -> Result<(), String> {
    use crate::plugin_command::{Failed, Leftovers, Run};
    let (name, command) = match phase {
        Phase::Setup => ("setup", &s.setup),
        Phase::Teardown => ("teardown", &s.teardown),
    };
    let done = crate::plugin_command::run(Run {
        cwd: Some(dir),
        env: crate::plugin_context::env(dir),
        keep: 16 * 1024,
        // A setup may mean to start something; it just may not make anyone wait for it.
        ..Run::shell(
            command,
            Duration::from_millis(s.timeout_ms),
            Leftovers::Keep,
        )
    })
    .map_err(|e| match e {
        Failed::Start(why) => format!("{name} 启动不了：{why}"),
        Failed::TimedOut => format!("{name} 超过 {} ms，已停止", s.timeout_ms),
    })?;
    if done.status.success() {
        return Ok(());
    }
    let out = [&done.stderr, &done.stdout]
        .into_iter()
        .map(|b| String::from_utf8_lossy(b).trim().to_owned())
        .find(|s| !s.is_empty())
        .unwrap_or_default();
    Err(format!(
        "{name} 失败（{}）{}",
        done.how(),
        if out.is_empty() {
            String::new()
        } else {
            format!("：{}", crate::content::short(&out, 300))
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_and_teardown_run_in_the_plugin_with_a_data_directory() {
        let root = std::env::temp_dir().join(format!("kl-setup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let plugins = root.join("plugins");
        let dir = plugins.join("wrap");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"api":1,"id":"wrap","name":"W","provides":["setup"]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("setup.json"),
            r#"{"api":1,"setup":"pwd > \"$MOORING_PLUGIN_DATA/on\"","teardown":"echo 还原不了 >&2; exit 3"}"#,
        )
        .unwrap();
        run(&plugins, &dir, Phase::Setup).unwrap();
        let data = root.join("plugin-data/wrap");
        let pwd = std::fs::read_to_string(data.join("on")).unwrap();
        assert_eq!(
            Path::new(pwd.trim()).canonicalize().unwrap(),
            dir.canonicalize().unwrap()
        );
        let err = run(&plugins, &dir, Phase::Teardown).unwrap_err();
        assert!(
            err.contains("退出码 3") && err.contains("还原不了"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn both_steps_are_required() {
        let root = std::env::temp_dir().join(format!("kl-setup-bad-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let f = root.join("setup.json");
        std::fs::write(&f, r#"{"api":1,"setup":"true","teardown":" "}"#).unwrap();
        assert!(read(&f).unwrap_err().contains("撤回"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The setup's limit holds whatever it leaves behind: returns at once for a background
    /// child, and ends what it started at the limit.
    #[test]
    fn the_time_limit_holds_a_setup_with_children() {
        let root = std::env::temp_dir().join(format!("kl-setup-bg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let plugins = root.join("plugins");
        let dir = plugins.join("bg");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"api":1,"id":"bg","name":"B","provides":["setup"]}"#,
        )
        .unwrap();
        let mark = root.join("survived");
        std::fs::write(
            dir.join("setup.json"),
            format!(
                r#"{{"api":1,"setup":"sleep 6 & echo started","teardown":"(sleep 1; touch {}) & wait","timeout_ms":200}}"#,
                mark.display()
            ),
        )
        .unwrap();
        let at = Instant::now();
        run(&plugins, &dir, Phase::Setup).unwrap();
        assert!(at.elapsed() < Duration::from_secs(2), "{:?}", at.elapsed());
        let err = run(&plugins, &dir, Phase::Teardown).unwrap_err();
        assert!(err.contains("200 ms"), "{err}");
        std::thread::sleep(Duration::from_millis(1200));
        assert!(!mark.exists(), "what it started ended at the limit");
        let _ = std::fs::remove_dir_all(&root);
    }
}
