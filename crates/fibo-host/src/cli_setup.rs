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

/// Off by the variable, or by `fibo setup remove` (which leaves this file in the config).
pub(crate) const OFF_FILE: &str = "cli-setup-off";

/// Whether the user turned all of this off.
pub fn turned_off() -> bool {
    std::env::var_os(OFF_ENV).is_some()
        || fibo_core::agent::config_dir().is_some_and(|d| d.join(OFF_FILE).exists())
}

/// A build under test or a host on a private runtime touches nothing of the user's.
pub fn install(runtime: &Path) {
    if turned_off() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let exe = exe.with_file_name(cli_entry::NAME);
    if !cli_entry::in_bundle(&exe) || !cli_entry::is_default_runtime(runtime) {
        return;
    }
    for name in names_to_link(&cli_entry::DIRS) {
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

/// `fibo`, and the old name only for an install that already has it: a new user never
/// typed `mooring`, and the name was dropped because another tool uses it.
fn names_to_link(dirs: &[impl AsRef<Path>]) -> Vec<&'static str> {
    cli_entry::NAMES
        .into_iter()
        .filter(|name| *name == cli_entry::NAME || cli_entry::ours_in(dirs, name).is_some())
        .collect()
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

/// Takes away the skills `install` wrote (never one someone else wrote), and says which.
pub(crate) fn remove_skills(home: &Path) -> io::Result<Vec<std::path::PathBuf>> {
    let mut gone = vec![];
    for agent in Agent::all() {
        let Some(skills) = adapter(agent).skills_dir(home) else {
            continue;
        };
        for name in ["fibo", "mooring"] {
            let dir = skills.join(name);
            if fs::read_to_string(dir.join("SKILL.md")).is_ok_and(|now| is_ours(&now)) {
                fs::remove_dir_all(&dir)?;
                gone.push(dir);
            }
        }
    }
    Ok(gone)
}

/// The skills `install` wrote that are there now.
pub(crate) fn skills_written(home: &Path) -> Vec<std::path::PathBuf> {
    Agent::all()
        .into_iter()
        .filter_map(|agent| adapter(agent).skills_dir(home))
        .map(|skills| skills.join("fibo").join("SKILL.md"))
        .filter(|f| fs::read_to_string(f).is_ok_and(|now| is_ours(&now)))
        .collect()
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

    #[cfg(unix)]
    #[test]
    fn a_new_install_gets_fibo_and_only_an_old_one_keeps_mooring() {
        let home = scratch();
        let bin = home.as_path().join("bin");
        fs::create_dir(&bin).unwrap();
        let dirs = [bin.clone()];
        assert_eq!(names_to_link(&dirs), vec![cli_entry::NAME]);
        let mac = home.as_path().join("Mooring.app/Contents/MacOS");
        fs::create_dir_all(&mac).unwrap();
        fs::write(mac.join("mooring"), "").unwrap();
        std::os::unix::fs::symlink(mac.join("mooring"), bin.join("mooring")).unwrap();
        assert_eq!(names_to_link(&dirs), cli_entry::NAMES.to_vec());
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
    fn removing_takes_only_the_skills_fibo_wrote() {
        // Codex's own home would point the removal at the developer's real skills.
        if std::env::var_os("CODEX_HOME").is_some() {
            return;
        }
        let home = scratch();
        let codex = home.as_path().join(".codex/skills");
        fs::create_dir_all(codex.join("fibo")).unwrap();
        fs::write(codex.join("fibo/SKILL.md"), current_skill()).unwrap();
        fs::create_dir_all(codex.join("mine")).unwrap();
        fs::write(codex.join("mine/SKILL.md"), "用户自己写的").unwrap();
        assert_eq!(skills_written(&home).len(), 1);
        let gone = remove_skills(&home).unwrap();
        assert_eq!(gone, vec![codex.join("fibo")]);
        assert!(codex.join("mine/SKILL.md").is_file());
        assert!(skills_written(&home).is_empty());
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
