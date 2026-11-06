//! What a host serving the user's own runtime sets up so that Agents can use `mooring`
//! wherever they run their commands: the name on PATH, and telling the CLIs that have no
//! hook of ours when to use it. Failing leaves the todo's handoff to spell the path out, so
//! it only logs.
use crate::agents::adapter;
use mooring_core::{cli_entry, model::Agent};
use std::{fs, io, path::Path};

/// What a CLI reads to know when to write something down.
const SKILL: &str = include_str!("todo_skill.md");
const OURS: &str = "<!-- 由 Mooring 写入";
/// Turns all of this off, like `MOORING_NO_SHELL_INTEGRATION` does for its own part.
const OFF_ENV: &str = "MOORING_NO_CLI_SETUP";

/// A build under test or a host on a private runtime touches nothing of the user's.
pub fn install(runtime: &Path) {
    if std::env::var_os(OFF_ENV).is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let exe = exe.with_file_name("mooring");
    if !cli_entry::in_bundle(&exe) || !cli_entry::is_default_runtime(runtime) {
        return;
    }
    match cli_entry::link(&cli_entry::DIRS, &exe) {
        Ok(Some(_)) => {}
        Ok(None) => eprintln!("mooring: 没有放入口：目录不可写，或那里已有别的 mooring"),
        Err(e) => eprintln!("mooring: 放入口失败：{e}"),
    }
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    for agent in Agent::all() {
        let Some(skills) = adapter(agent).skills_dir(Path::new(&home)) else {
            continue;
        };
        if let Err(e) = write_skill(&skills) {
            eprintln!("mooring: 写 {} 失败：{e}", skills.display());
        }
    }
}

/// Only where the CLI is set up, and never over a `mooring` skill someone else wrote.
fn write_skill(skills: &Path) -> io::Result<bool> {
    if !skills.parent().is_some_and(Path::is_dir) {
        return Ok(false);
    }
    let dir = skills.join("mooring");
    let file = dir.join("SKILL.md");
    match fs::read_to_string(&file) {
        Ok(now) if now == SKILL => return Ok(false),
        Ok(now) if !now.contains(OURS) => return Ok(false),
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    fs::create_dir_all(&dir)?;
    let fresh = dir.join(".SKILL.md.new");
    fs::write(&fresh, SKILL)?;
    fs::rename(&fresh, &file)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mooring-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_skill_is_ours_to_update_and_nobody_elses_to_lose() {
        let home = scratch();
        let skills = home.as_path().join("cli/skills");
        assert!(!write_skill(&skills).unwrap(), "that CLI isn't set up here");
        assert!(!home.as_path().join("cli").exists());

        fs::create_dir(home.as_path().join("cli")).unwrap();
        assert!(write_skill(&skills).unwrap());
        let file = skills.join("mooring/SKILL.md");
        assert_eq!(fs::read_to_string(&file).unwrap(), SKILL);
        assert!(!write_skill(&skills).unwrap(), "already current");

        fs::write(&file, format!("{OURS} 旧版 -->")).unwrap();
        assert!(write_skill(&skills).unwrap(), "an older one of ours");
        assert_eq!(fs::read_to_string(&file).unwrap(), SKILL);

        fs::write(&file, "用户自己写的").unwrap();
        assert!(!write_skill(&skills).unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), "用户自己写的");
    }

    #[test]
    fn the_skill_names_only_commands_that_exist_and_forbids_going_around_them() {
        for said in ["mooring todo list", "mooring todo note", "mooring todo add"] {
            assert!(SKILL.contains(said), "{said}");
        }
        assert!(SKILL.contains(OURS));
        assert!(SKILL.contains("不要绕过命令"));
        assert!(!SKILL.contains("todo done"));
    }
}
