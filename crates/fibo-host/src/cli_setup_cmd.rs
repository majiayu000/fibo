//! `fibo setup`: what the packaged app puts outside itself so Agents can call `fibo` by
//! name (links on PATH, the skill written for Codex), shown, taken away, or put back.
//! Taking it away also leaves a file in the config so the next start does not redo it,
//! which a variable set for one shell could not promise for an app opened from Finder.
use crate::cli_setup::{self, OFF_FILE};
use fibo_core::cli_entry::{self, DIRS, NAMES};
use std::{fs, io, path::Path};

pub fn run(mut args: impl Iterator<Item = String>) -> io::Result<()> {
    match args.next().as_deref() {
        None | Some("status") => status(),
        Some("remove") => remove(),
        Some("install") => install(),
        Some(other) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            t!("fibo setup 没有 {other}；有 status、remove、install", other),
        )),
    }
}

fn home() -> io::Result<std::path::PathBuf> {
    fibo_platform::dirs::home().ok_or_else(|| io::Error::other(t!("找不到主目录")))
}

fn status() -> io::Result<()> {
    let state = if cli_setup::turned_off() {
        t!("关着：App 启动时不放链接、不写 skill（fibo setup install 打开）")
    } else {
        t!("开着：打包的 App 每次启动都检查一遍（fibo setup remove 关掉并拿掉）")
    };
    println!("{state}");
    for name in NAMES {
        match cli_entry::ours_in(&DIRS, name) {
            Some(entry) => {
                let target = fs::read_link(&entry).unwrap_or_default();
                println!("  {} → {}", entry.display(), target.display());
            }
            None => println!("{}", t!("  没有放 {name}", name)),
        }
    }
    let skills = cli_setup::skills_written(&home()?);
    if skills.is_empty() {
        println!("{}", t!("  没有写给 CLI 的 skill"));
    }
    for f in skills {
        println!("  skill: {}", f.display());
    }
    Ok(())
}

fn remove() -> io::Result<()> {
    let config =
        fibo_core::agent::config_dir().ok_or_else(|| io::Error::other(t!("找不到配置目录")))?;
    fs::create_dir_all(&config)?;
    fs::write(config.join(OFF_FILE), "")?;
    let mut gone = vec![];
    for name in NAMES {
        gone.extend(cli_entry::unlink(&DIRS, name)?);
    }
    gone.extend(cli_setup::remove_skills(&home()?)?);
    for path in &gone {
        println!("{}", t!("拿掉了 {path}", path = path.display()));
    }
    if gone.is_empty() {
        println!("{}", t!("没有要拿掉的"));
    }
    println!(
        "{}",
        t!("以后 App 启动也不再放；要放回来：fibo setup install")
    );
    Ok(())
}

fn install() -> io::Result<()> {
    if let Some(config) = fibo_core::agent::config_dir() {
        match fs::remove_file(config.join(OFF_FILE)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    let runtime = fibo_core::legacy::runtime_dir(Path::new(&home()?));
    cli_setup::install(&runtime);
    status()
}
