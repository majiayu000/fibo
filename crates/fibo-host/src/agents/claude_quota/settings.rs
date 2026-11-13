//! Puts the status line into Claude Code's settings (`setup`) and takes it out again
//! (`teardown`). The one set before is kept in the plugin's data directory and keeps printing.
//! The entry is known by `MARK`, which every version has written, so whichever version runs
//! next takes over from the last; a Keepline install's wrapper is replaced the same way.
use crate::agents::hooks;
use fibo_core::model::Agent;
use fibo_platform::fs::{replace_file, write_private};
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MARK: &str = "MOORING_PLUGIN=claude-quota ";
const KEEPLINE_WRAPPER: &str = "keepline-claude-statusline";

fn home() -> io::Result<PathBuf> {
    fibo_platform::dirs::home().ok_or_else(|| io::Error::other("找不到家目录"))
}
fn settings_path(home: &Path) -> io::Result<PathBuf> {
    Ok(hooks::target(Agent::Claude, home)?.settings)
}
fn command_of(line: &Value) -> Option<&str> {
    line.get("command")?.as_str()
}
fn ours(line: Option<&Value>) -> bool {
    line.and_then(command_of)
        .is_some_and(|c| c.starts_with(MARK))
}
fn quote(path: &Path) -> String {
    fibo_platform::shell::quote_posix(&path.to_string_lossy())
}
fn wrapper(data: &Path) -> io::Result<String> {
    let exe = std::env::current_exe()?;
    Ok(format!(
        "{MARK}{} claude-quota status {}",
        quote(&exe),
        quote(data)
    ))
}
fn write_settings(path: &Path, settings: &Map<String, Value>) -> io::Result<()> {
    let mut text = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    text.push('\n');
    replace_file(path, text.as_bytes()).map(|_| ())
}
fn save_original(data: &Path, line: Option<&Value>) -> io::Result<()> {
    let kept = line.cloned().unwrap_or(Value::Null);
    let mut text = serde_json::to_string(&kept).map_err(io::Error::other)?;
    text.push('\n');
    fs::write(data.join("original.json"), text)?;
    let command = (kept["type"] == "command")
        .then(|| command_of(&kept))
        .flatten()
        .filter(|c| !c.trim().is_empty());
    match command {
        Some(c) => fs::write(data.join("original.sh"), format!("{c}\n")),
        None => remove(&data.join("original.sh")),
    }
}
fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
/// The settings can hold API keys, and so can this copy, whatever the original's permissions.
fn back_up(path: &Path, backup: &Path) -> io::Result<()> {
    match fs::read(path) {
        Ok(bytes) => write_private(backup, &bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

pub(super) fn setup(data: &Path) -> io::Result<()> {
    let home = home()?;
    let path = settings_path(&home)?;
    let backup = data.join("settings.before.json");
    // A backup from before it was kept private; nothing else would ever rewrite it.
    if let Ok(bytes) = fs::read(&backup) {
        write_private(&backup, &bytes)?;
    }
    let mut settings = hooks::read(&path)?;
    let current = settings.get("statusLine").cloned();
    let command = wrapper(data)?;
    if ours(current.as_ref()) {
        // Already wrapped; the command may have moved, or be an older version's script.
        if current.as_ref().and_then(command_of) == Some(&command) {
            println!("已经接上，没有改动");
            return Ok(());
        }
        let mut line = current.unwrap_or(Value::Null);
        line["command"] = command.into();
        settings.insert("statusLine".into(), line);
        write_settings(&path, &settings)?;
        println!("更新了状态栏命令");
        return Ok(());
    }
    let from_keepline = current
        .as_ref()
        .and_then(command_of)
        .is_some_and(|c| c.contains(KEEPLINE_WRAPPER));
    let original = if from_keepline {
        fs::read(home.join(".keepline/claude-statusline-original.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .filter(|v| !v.is_null())
    } else {
        current
    };
    save_original(data, original.as_ref())?;
    back_up(&path, &backup)?;
    let mut line = match original {
        Some(Value::Object(fields)) => Value::Object(fields),
        _ => Value::Object(Map::new()),
    };
    line["type"] = "command".into();
    line["command"] = command.into();
    settings.insert("statusLine".into(), line);
    write_settings(&path, &settings)?;
    println!(
        "{}",
        if from_keepline {
            "换掉了 Keepline 的旧包装，原来的状态栏照常显示"
        } else {
            "接上了，原来的状态栏照常显示"
        }
    );
    Ok(())
}

pub(super) fn teardown(data: &Path) -> io::Result<()> {
    let path = settings_path(&home()?)?;
    let mut settings = hooks::read(&path)?;
    if !ours(settings.get("statusLine")) {
        println!("状态栏不是这个插件的，没有改");
        return Ok(());
    }
    let file = data.join("original.json");
    let original = match fs::read(&file) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| io::Error::other(format!("{} 读不懂，没有改：{e}", file.display())))?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Value::Null,
        Err(e) => return Err(e),
    };
    let restored = !original.is_null();
    if restored {
        settings.insert("statusLine".into(), original);
    } else {
        settings.remove("statusLine");
    }
    write_settings(&path, &settings)?;
    remove(&file)?;
    remove(&data.join("original.sh"))?;
    println!(
        "{}",
        if restored {
            "还原了原来的状态栏"
        } else {
            "去掉了状态栏（装之前没有设）"
        }
    );
    Ok(())
}

/// The entry names this program by its path, so an App moved or reinstalled elsewhere leaves
/// Claude Code running nothing and the user's own line gone. Called by the running build: it
/// takes over only when the program named is really gone and the data directory is its own,
/// so a second build beside the installed one never takes the entry from it.
pub(super) fn repair(data: &Path) -> io::Result<()> {
    let path = settings_path(&home()?)?;
    let mut settings = hooks::read(&path)?;
    let Some(line) = settings.get_mut("statusLine") else {
        return Ok(());
    };
    let Some(words) = command_of(line)
        .filter(|c| c.starts_with(MARK))
        .and_then(shlex::split)
    else {
        return Ok(());
    };
    // `MOORING_PLUGIN=claude-quota <program> claude-quota status <data>`
    let [_, program, sub, _, named] = words.as_slice() else {
        return Ok(());
    };
    if sub != "claude-quota" || Path::new(program).exists() || Path::new(named) != data {
        return Ok(());
    }
    line["command"] = wrapper(data)?.into();
    write_settings(&path, &settings)
}
