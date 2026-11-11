//! `mooring themes`: checks theme files with the rules the app reads them by, so an Agent
//! that wrote one knows now what will be skipped; and lists every name a theme can set.
use mooring_core::i18n;
use mooring_core::status::Status;
use mooring_core::theme::{
    base_of, check_file, ANSI_NAMES, SEEN_NAMES, TERMINAL_NAMES, TONE_NAMES, UI_NAMES,
};
use std::io;
use std::path::{Path, PathBuf};

pub const USAGE: &str = tk!("\
fibo themes [文件或目录]                 按 App 的规则检查主题：跳过了什么、图在不在、对比度够不够
fibo themes names                       列出主题能写的全部键");
fn folder() -> io::Result<PathBuf> {
    mooring_core::agent::config_dir()
        .map(|config| config.join("themes"))
        .ok_or_else(|| io::Error::other(t!("找不到配置目录")))
}
fn files_at(path: &Path) -> io::Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_owned()]);
    }
    if !path.exists() {
        return Ok(vec![]);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(path)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    Ok(files)
}
/// One line a file and one per thing to say, and whether anything was wrong.
fn report(files: &[PathBuf]) -> (Vec<String>, bool) {
    let (mut lines, mut failed) = (vec![], false);
    for file in files {
        let shown = file.file_name().unwrap_or_default().to_string_lossy();
        match check_file(file, &base_of(file)) {
            Ok(check) => {
                let side = if check.theme.term_colors().light {
                    t!("亮色")
                } else {
                    t!("暗色")
                };
                failed |= !check.theme.problems.is_empty();
                let verdict = if check.theme.problems.is_empty() {
                    t!("可用")
                } else {
                    t!("有问题")
                };
                lines.push(t!(
                    "{shown}：{name}（{side}），{verdict}",
                    shown,
                    name = check.theme.name,
                    side,
                    verdict
                ));
                lines.extend(check.theme.problems.iter().map(|p| format!("  {p}")));
                lines.extend(
                    check
                        .warnings
                        .iter()
                        .map(|w| format!("  {}", t!("提醒：{w}", w))),
                );
            }
            Err(error) => {
                failed = true;
                lines.push(t!("{shown}：读不了，{error}", shown, error));
            }
        }
    }
    (lines, failed)
}
fn names() -> Vec<String> {
    let mut out = vec![
        t!("bg 背景（必填）").to_owned(),
        t!("fg 前景（必填）").to_owned(),
    ];
    out.extend(TERMINAL_NAMES.iter().map(|n| format!("terminal.{n}")));
    out.extend(ANSI_NAMES.iter().map(|n| format!("ansi.{n}")));
    out.extend(
        UI_NAMES
            .iter()
            .map(|(n, label)| format!("ui.{n} {}", i18n::say(label))),
    );
    out.extend(Status::ALL.iter().map(|s| format!("status.{}", s.key())));
    out.extend(TONE_NAMES.iter().map(|n| format!("tone.{n}")));
    out.extend(SEEN_NAMES.iter().map(|n| format!("seen.{n}")));
    out.push(t!("seen.dot ring、fill 或 fade").to_owned());
    out.extend([
        t!("space_gradients 若干对颜色").to_owned(),
        "window.opacity 0.3–1".to_owned(),
        t!("window.blur true 或 false").to_owned(),
        t!("background.image 路径").to_owned(),
        t!("background.mode fill、fit、stretch 或 tile").to_owned(),
        "background.strength 0.05–1".to_owned(),
    ]);
    out
}
pub fn run(mut args: impl Iterator<Item = String>) -> io::Result<()> {
    let at = match args.next().as_deref() {
        Some("names") => {
            println!("{}", names().join("\n"));
            return Ok(());
        }
        Some(path) if path.starts_with('-') => {
            return Err(io::Error::other(t!("不认识 {path}", path)));
        }
        Some(path) => PathBuf::from(path),
        None => folder()?,
    };
    let files = files_at(&at)?;
    if files.is_empty() {
        println!("{}", t!("{path} 里没有主题", path = at.display()));
        return Ok(());
    }
    let (lines, failed) = report(&files);
    println!("{}", lines.join("\n"));
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn says_what_is_wrong_and_what_only_needs_a_look() {
        let dir = std::env::temp_dir().join(format!("mooring-themes-cmd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
        write("good.json", r##"{"bg":"#101010","fg":"#eeeeee"}"##);
        write("dim.json", r##"{"bg":"#101010","fg":"#202020"}"##);
        write(
            "typo.json",
            r##"{"bg":"#101010","fg":"#eeeeee","ui":{"sidbar":"#000000"}}"##,
        );
        write("broken.json", "{");
        write("note.txt", "x");
        let files = files_at(&dir).unwrap();
        assert_eq!(files.len(), 4);
        let (lines, failed) = report(&files);
        assert!(failed);
        let text = lines.join("\n");
        assert!(text.contains("ui.sidbar"), "{text}");
        assert!(
            text.contains("提醒：前景和背景") || text.contains("Note:"),
            "{text}"
        );
        assert!(
            text.contains("broken.json：读不了") || text.contains("broken.json: cannot read"),
            "{text}"
        );
        let (_, only_warned) = report(&[dir.join("good.json"), dir.join("dim.json")]);
        assert!(!only_warned, "low contrast is a note, not a failure");
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn lists_every_name_an_agent_can_write() {
        let all = names();
        for name in [
            "ui.sidebar",
            "terminal.selected_text",
            "status.running",
            "tone.ok",
            "seen.dot",
        ] {
            assert!(all.iter().any(|n| n.starts_with(name)), "{name}");
        }
    }
}
