//! Every theme there is: the built-ins and the files in `<config>/themes/`. A file is
//! read when it changes, on a slow timer off the window's thread; drawing never touches
//! this, only the finished palette `appearance` installed.
use super::palette::Palette;
use super::status_colors::config_dir;
use gpui::{App, AppContext as _, Global};
use mooring_core::terminal::Scheme;
use mooring_core::theme::{ThemeFile, file_stem};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How often the folder is looked at: one directory listing, and a stat per file.
const SCAN_EVERY: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub palette: Palette,
    /// The file and what it says; a built-in has none.
    pub file: Option<(PathBuf, ThemeFile)>,
}
impl Entry {
    pub fn light(&self) -> bool {
        self.palette.term.light
    }
    /// What 调整 starts from: the theme itself, or a built-in as if written out.
    pub fn as_file(&self) -> ThemeFile {
        self.file.as_ref().map_or_else(
            || ThemeFile {
                name: format!("{} 副本", self.name),
                bg: self.palette.term.bg,
                fg: self.palette.term.fg,
                ..ThemeFile::default()
            },
            |(_, file)| file.clone(),
        )
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Themes {
    pub entries: Vec<Entry>,
    /// File name and why, for what could not be read or was partly skipped.
    pub problems: Vec<(String, String)>,
}
impl Global for Themes {}
impl Themes {
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
    /// One side's themes, built-ins first.
    pub fn side(&self, light: bool) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.light() == light)
    }
    pub fn read(dir: Option<&Path>) -> Self {
        let mut out = Self {
            entries: Scheme::ALL
                .into_iter()
                .map(|scheme| Entry {
                    id: scheme.name().to_owned(),
                    name: scheme.label().to_owned(),
                    palette: Palette::of_scheme(scheme),
                    file: None,
                })
                .collect(),
            problems: vec![],
        };
        let mut paths: Vec<PathBuf> = dir
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == "json"))
            .collect();
        paths.sort();
        for path in paths {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let id = format!("file:{stem}");
            let shown = path.file_name().unwrap_or_default().to_string_lossy();
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| ThemeFile::parse(&text, &stem));
            match parsed {
                Ok(mut file) => {
                    file.problems.extend(Palette::unknown_keys(&file));
                    if let Some(image) = &file.material.image
                        && !super::backdrop::resolve(&image.path).is_file()
                    {
                        file.problems.push(format!("找不到背景图 {}", image.path));
                    }
                    if !file.problems.is_empty() {
                        out.problems
                            .push((shown.to_string(), file.problems.join("；")));
                    }
                    out.entries.push(Entry {
                        id,
                        name: file.name.clone(),
                        palette: Palette::of_file(&file),
                        file: Some((path.clone(), file)),
                    });
                }
                Err(error) => out.problems.push((shown.to_string(), error)),
            }
        }
        out
    }
}
pub fn dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("themes"))
}
/// What says the folder changed: each file's name, time and size.
fn signature(dir: &Path) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((entry.path(), meta.modified().ok(), meta.len()))
        })
        .collect();
    out.sort();
    out
}
/// Read the folder again now; a theme just saved shows without waiting for the timer.
pub fn reload(cx: &mut App) {
    let themes = Themes::read(dir().as_deref());
    cx.set_global(themes);
    super::appearance::apply(cx);
    crate::set_menus(cx);
    cx.refresh_windows();
}
/// Load the themes and keep them current while the app runs.
pub fn start(cx: &mut App) {
    let folder = dir();
    cx.set_global(Themes::read(folder.as_deref()));
    let Some(folder) = folder else { return };
    cx.spawn(async move |cx| {
        let mut seen = signature(&folder);
        loop {
            cx.background_executor().timer(SCAN_EVERY).await;
            let scan = folder.clone();
            let now = cx.background_spawn(async move { signature(&scan) }).await;
            if now != seen {
                seen = now;
                cx.update(reload);
            }
        }
    })
    .detach();
}
/// Write a theme: over its own file, or to a new one named after it. Returns its id.
pub fn save(file: &ThemeFile, over: Option<&Path>) -> Result<String, String> {
    let folder = dir().ok_or("找不到配置目录")?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("无法创建 {}：{e}", folder.display()))?;
    let path = over.map_or_else(
        || free_path(&folder, &file_stem(&file.name)),
        Path::to_owned,
    );
    std::fs::write(&path, file.to_json())
        .map_err(|e| format!("无法保存 {}：{e}", path.display()))?;
    Ok(format!(
        "file:{}",
        path.file_stem().unwrap_or_default().to_string_lossy()
    ))
}
fn free_path(folder: &Path, stem: &str) -> PathBuf {
    let plain = folder.join(format!("{stem}.json"));
    if !plain.exists() {
        return plain;
    }
    (2..)
        .map(|n| folder.join(format!("{stem}-{n}.json")))
        .find(|path| !path.exists())
        .unwrap_or(plain)
}
/// A picture copied into the themes folder; returns the path a theme names it by.
pub fn keep_picture(from: &Path) -> Result<String, String> {
    let folder = dir().ok_or("找不到配置目录")?.join("images");
    std::fs::create_dir_all(&folder).map_err(|e| format!("无法创建 {}：{e}", folder.display()))?;
    let name = from.file_name().ok_or("这不是一个文件")?;
    let to = folder.join(name);
    if from != to {
        std::fs::copy(from, &to).map_err(|e| format!("无法复制 {}：{e}", from.display()))?;
    }
    Ok(format!("images/{}", name.to_string_lossy()))
}
/// A theme or an iTerm2 preset from anywhere, as a theme file ready to save.
pub fn import(path: &Path) -> Result<ThemeFile, String> {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let text = std::fs::read_to_string(path).map_err(|e| format!("读不了这个文件：{e}"))?;
    if path.extension().is_some_and(|e| e == "itermcolors") {
        ThemeFile::from_itermcolors(&text, &stem)
    } else {
        ThemeFile::parse(&text, &stem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_come_first_and_a_broken_file_is_named() {
        let dir = std::env::temp_dir().join(format!("mooring-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("sea.json"),
            r##"{"name":"深海","bg":"#0f1c2e","fg":"#d5dfef","ui":{"wobble":"#000000"}}"##,
        )
        .unwrap();
        std::fs::write(dir.join("day.json"), r##"{"bg":"#fdfdf8","fg":"#222222"}"##).unwrap();
        std::fs::write(dir.join("bad.json"), "{").unwrap();
        std::fs::write(dir.join("note.txt"), "x").unwrap();
        let themes = Themes::read(Some(&dir));
        assert_eq!(themes.entries[0].id, "dark");
        assert_eq!(themes.entries.len(), Scheme::ALL.len() + 2);
        let sea = themes.get("file:sea").unwrap();
        assert_eq!(sea.name, "深海");
        assert!(!sea.light());
        assert!(themes.get("file:day").unwrap().light());
        assert_eq!(themes.side(true).count(), 2);
        assert_eq!(themes.problems.len(), 2, "{:?}", themes.problems);
        assert_eq!(signature(&dir).len(), 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    /// What a theme costs off the drawing path. `--ignored --nocapture` in release.
    #[test]
    #[ignore = "prints timings"]
    fn costs() {
        use std::time::Instant;
        let dir = std::env::temp_dir().join(format!("mooring-cost-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in 0..50 {
            std::fs::write(
                dir.join(format!("t{n}.json")),
                r##"{"bg":"#0f1c2e","fg":"#d5dfef","ui":{"sidebar":"#0b1522","border":"#23344d"},"ansi":{"red":"#e5534b"}}"##,
            )
            .unwrap();
        }
        let file = ThemeFile::parse(
            r##"{"bg":"#0f1c2e","fg":"#d5dfef","ui":{"sidebar":"#0b1522"}}"##,
            "x",
        )
        .unwrap();
        let t = Instant::now();
        for _ in 0..10_000 {
            std::hint::black_box(Palette::of_file(std::hint::black_box(&file)));
        }
        println!("Palette::of_file: {:?} each", t.elapsed() / 10_000);
        let t = Instant::now();
        for _ in 0..100 {
            std::hint::black_box(signature(&dir));
        }
        println!("signature, 50 files: {:?} each", t.elapsed() / 100);
        let t = Instant::now();
        for _ in 0..100 {
            std::hint::black_box(Themes::read(Some(&dir)));
        }
        println!("Themes::read, 50 files: {:?} each", t.elapsed() / 100);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn a_built_in_is_copied_under_a_new_name() {
        let themes = Themes::read(None);
        let file = themes.get("slate").unwrap().as_file();
        assert_eq!(file.name, "蓝灰 副本");
        assert_eq!((file.bg, file.fg), (Scheme::Slate.bg(), Scheme::Slate.fg()));
    }
}
