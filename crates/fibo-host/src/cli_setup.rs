//! What a host serving the user's own runtime sets up so that Agents can use `fibo`
//! wherever they run their commands: the name on PATH, and telling the CLIs that have no
//! hook of ours when to use it. Failing leaves the todo's handoff to spell the path out, so
//! it only logs.
use crate::agents::adapter;
use fibo_core::i18n::{self, Lang};
use fibo_core::{cli_entry, model::Agent};
use std::{fs, io, path::Path};

/// What a CLI reads to know when to write something down.
const SKILL: &str = include_str!("todo_skill.md");
const SKILL_EN: &str = include_str!("todo_skill_en.md");
const OURS: &str = "<!-- 由 Fibo 写入";
const OURS_EN: &str = "<!-- Written by Fibo";
/// What a skill written before the rename starts with: still ours, so it gets updated.
const OURS_BEFORE: &str = "<!-- 由 Mooring 写入";
/// Turns all of this off, like `MOORING_NO_SHELL_INTEGRATION` does for its own part.
const OFF_ENV: &str = "MOORING_NO_CLI_SETUP";

fn current_skill() -> &'static str {
    match i18n::lang() {
        Lang::Zh => SKILL,
        Lang::En => SKILL_EN,
    }
}

/// A build under test or a host on a private runtime touches nothing of the user's.
pub fn install(runtime: &Path) {
    if std::env::var_os(OFF_ENV).is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let exe = exe.with_file_name(cli_entry::NAME);
    if !cli_entry::in_bundle(&exe) || !cli_entry::is_default_runtime(runtime) {
        return;
    }
    for name in cli_entry::NAMES {
        match cli_entry::link(&cli_entry::DIRS, &exe, name) {
            Ok(Some(_)) => {}
            Ok(None) => eprintln!(
                "fibo: {}",
                t!("没有放入口：目录不可写，或那里已有别的 {name}", name)
            ),
            Err(e) => eprintln!("fibo: {}", t!("放 {name} 入口失败：{e}", name, e)),
        }
    }
    let Some(home) = fibo_platform::dirs::home() else {
        return;
    };
    for agent in Agent::all() {
        let Some(skills) = adapter(agent).skills_dir(Path::new(&home)) else {
            continue;
        };
        if let Err(e) = write_skill(&skills) {
            eprintln!(
                "fibo: {}",
                t!("写 {path} 失败：{e}", path = skills.display(), e)
            );
        }
    }
}

/// Only where the CLI is set up, and never over a `fibo` skill someone else wrote. The one
/// written under the old name (`mooring`) goes, if it is ours: two would both answer 「记一下」.
fn write_skill(skills: &Path) -> io::Result<bool> {
    if !skills.parent().is_some_and(Path::is_dir) {
        return Ok(false);
    }
    let old = skills.join("mooring");
    if fs::read_to_string(old.join("SKILL.md")).is_ok_and(|now| is_ours(&now)) {
        fs::remove_dir_all(&old)?;
    }
    let want = current_skill();
    let dir = skills.join("fibo");
    let file = dir.join("SKILL.md");
    match fs::read_to_string(&file) {
        Ok(now) if now == want => return Ok(false),
        Ok(now) if !is_ours(&now) => return Ok(false),
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    fibo_platform::fs::replace_file(&file, want.as_bytes())?;
    Ok(true)
}

fn is_ours(skill: &str) -> bool {
    skill.contains(OURS) || skill.contains(OURS_EN) || skill.contains(OURS_BEFORE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fibo-cli-{}-{}",
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
        let file = skills.join("fibo/SKILL.md");
        assert_eq!(fs::read_to_string(&file).unwrap(), current_skill());
        assert!(!write_skill(&skills).unwrap(), "already current");

        fs::write(&file, format!("{OURS} 旧版 -->")).unwrap();
        assert!(write_skill(&skills).unwrap(), "an older one of ours");
        assert_eq!(fs::read_to_string(&file).unwrap(), current_skill());

        fs::write(&file, format!("{OURS_EN}; older -->")).unwrap();
        assert!(write_skill(&skills).unwrap(), "an English one of ours");
        assert_eq!(fs::read_to_string(&file).unwrap(), current_skill());

        fs::write(
            &file,
            format!("{OURS_BEFORE}，App 启动时会更新这个文件。 -->"),
        )
        .unwrap();
        assert!(write_skill(&skills).unwrap(), "one Mooring wrote");
        assert_eq!(fs::read_to_string(&file).unwrap(), current_skill());

        fs::write(&file, "用户自己写的").unwrap();
        assert!(!write_skill(&skills).unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), "用户自己写的");
    }

    #[test]
    fn the_skill_written_under_the_old_name_goes_unless_someone_else_wrote_it() {
        let home = scratch();
        let skills = home.as_path().join("cli/skills");
        let old = skills.join("mooring");
        fs::create_dir_all(&old).unwrap();
        fs::write(
            old.join("SKILL.md"),
            format!("{OURS_BEFORE}，App 启动时会更新这个文件。 -->"),
        )
        .unwrap();
        assert!(write_skill(&skills).unwrap());
        assert!(!old.exists(), "two skills would both answer");
        assert!(skills.join("fibo/SKILL.md").is_file());

        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("SKILL.md"), "用户自己写的").unwrap();
        write_skill(&skills).unwrap();
        assert_eq!(
            fs::read_to_string(old.join("SKILL.md")).unwrap(),
            "用户自己写的"
        );
    }

    #[test]
    fn the_skill_names_only_commands_that_exist_and_forbids_going_around_them() {
        for (doc, mark, forbid) in [
            (SKILL, OURS, "不要绕过命令"),
            (SKILL_EN, OURS_EN, "Never bypass the CLI"),
        ] {
            for said in ["fibo todo list", "fibo todo note", "fibo todo add"] {
                assert!(doc.contains(said), "{said}");
            }
            assert!(doc.contains(mark));
            assert!(doc.contains(forbid));
            assert!(!doc.contains("todo done"));
        }
    }
}
