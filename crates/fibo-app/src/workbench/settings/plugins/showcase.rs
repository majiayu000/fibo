//! What a plugin's own files say, read once for the page so a plugin can be judged before it
//! is installed: the themes it brings, the commands it adds, the checks it counts.
use super::*;
use fibo_core::theme::{BackgroundImage, Material, ThemeFile};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
pub(super) struct Showcase {
    pub(super) themes: Vec<ThemeLook>,
    pub(super) actions: Vec<String>,
    pub(super) checks: Vec<String>,
    pub(super) agent: Option<(String, String)>,
}
#[derive(Clone)]
pub(super) struct ThemeLook {
    pub(super) name: String,
    pub(super) bg: u32,
    pub(super) fg: u32,
    pub(super) green: u32,
    pub(super) red: u32,
    pub(super) blue: u32,
    pub(super) material: Material,
    pub(super) image: Option<PathBuf>,
}

fn json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn read(dir: &Path) -> Showcase {
    let mut out = Showcase::default();
    let themes = dir.join("themes");
    let mut files: Vec<_> = std::fs::read_dir(&themes)
        .into_iter()
        .flatten()
        .flatten()
        .map(|f| f.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    for file in files {
        let fallback = file
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let Some(t) = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| ThemeFile::parse(&text, &fallback).ok())
        else {
            continue;
        };
        let ansi = t.colors.get("ansi");
        let of = |key: &str, or: u32| ansi.and_then(|a| a.get(key)).copied().unwrap_or(or);
        let image = t
            .material
            .image
            .as_ref()
            .map(|BackgroundImage { path, .. }| dir.join(path));
        out.themes.push(ThemeLook {
            name: t.name.clone(),
            bg: t.bg,
            fg: t.fg,
            green: of("green", t.fg),
            red: of("red", t.fg),
            blue: of("blue", t.fg),
            material: t.material.clone(),
            image,
        });
    }
    if let Some(v) = json(&dir.join("actions.json")) {
        out.actions = v["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a["title"].as_str().map(str::to_owned))
            .collect();
    }
    if let Some(v) = json(&dir.join("rules.json")) {
        out.checks = v["verify"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_array())
            .map(|c| {
                c.iter()
                    .filter_map(|w| w.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
    }
    if let Some(v) = json(&dir.join("agent.json"))
        && let (Some(db), Some(resume)) = (v["source"]["sqlite"].as_str(), v["resume"].as_str())
    {
        out.agent = Some((db.to_owned(), resume.to_owned()));
    }
    out
}

/// By plugin id: what is in the bundled catalog, and what is installed in its place.
pub(super) fn read_all(offers: &[Offer]) -> HashMap<String, Showcase> {
    let mut out = HashMap::new();
    for o in offers {
        if let market::Source::Bundled(dir) = &o.source {
            let folder = o.entry.dir.as_deref().unwrap_or(&o.entry.id);
            out.insert(o.entry.id.clone(), read(&dir.join(folder)));
        }
    }
    for p in plugin::report()
        .iter()
        .filter_map(|f| f.result.as_ref().ok())
    {
        out.insert(p.id.clone(), read(&p.dir));
    }
    out
}
