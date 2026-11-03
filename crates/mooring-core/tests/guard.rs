//! A ratchet on the two ways a fact grows a second owner (docs/single-source):
//! reading the raw fields core decides from (a status, a name, a time) outside the
//! modules that own them, and spelling a state's word by hand instead of taking it
//! from `Status::label`. Every such use is counted per file against
//! `tests/guard-baseline.txt`; any change fails until the baseline is rewritten
//! (`MOORING_GUARD_UPDATE=1 cargo test -p mooring-core --test guard`), so a new one
//! is a choice a reviewer sees, and a removed one lowers the bar for good.
use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Raw reads that decide a state, a name or a time.
const FIELDS: &[(&str, &str)] = &[
    ("turn", r"\.turn\b"),
    ("turn_started", r"\.turn_started\b"),
    ("background", r"\.background\b"),
    ("content-title", r"\b(c|content|card)\.title\b"),
    ("content-name", r"\b(c|content|card)\.name\b"),
    ("tab-title", r"\btab\.title\b"),
    ("session-updated", r"\b(s|session)\.updated\b"),
];
/// State words written by hand, and the variants they drifted to (出错 for 出错了,
/// 工作中 / 干活中 for 在跑, 已完成 for 做完了, 空闲, 需要你 for 要你处理).
const WORDS: &[&str] = &[
    "等你回答",
    "出错了",
    "在跑",
    "等后台",
    "等你验收",
    "新开的",
    "停下了",
    "很久没动",
    "稍后再看",
    "做完了",
    "要你处理",
    "需要你",
    "工作中",
    "干活中",
    "空闲",
    "已完成",
];

/// Where these facts are owned: core decides, the observation layer writes.
fn owner(path: &str) -> bool {
    path.starts_with("crates/mooring-core/")
        || path.starts_with("crates/mooring-host/src/content/")
        || path.starts_with("crates/mooring-host/src/agents/")
        || path.ends_with("/log_index.rs")
        || path.ends_with("/store.rs")
}
/// Tests and screenshot fixtures build raw states on purpose.
fn fixture(path: &str) -> bool {
    path.contains("/tests/")
        || path.ends_with("tests.rs")
        || path.ends_with("harness.rs")
        || path.ends_with("visual.rs")
        || path.contains("/test/")
}

fn files(root: &Path, dir: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
        if p.is_dir() {
            if !rel.contains("node_modules") {
                files(root, &rel, out);
            }
        } else if ["rs", "m", "ts", "tsx"]
            .iter()
            .any(|x| p.extension().is_some_and(|e| e == *x))
        {
            out.push(p);
        }
    }
}

/// Lines from the file outside `#[cfg(test)]` modules (a test module runs to the end
/// of its file in this codebase).
fn code(text: &str) -> &str {
    text.find("#[cfg(test)]").map_or(text, |i| &text[..i])
}

fn count(root: &Path) -> BTreeMap<String, usize> {
    let mut paths = vec![];
    for dir in ["crates", "mobile/src"] {
        files(root, dir, &mut paths);
    }
    let fields: Vec<(&str, Regex)> = FIELDS
        .iter()
        .map(|(k, r)| (*k, Regex::new(r).unwrap()))
        .collect();
    // A word inside a string literal (Rust, Objective-C or TypeScript quotes).
    let words: Vec<(&str, Regex)> = WORDS
        .iter()
        .map(|w| {
            (
                *w,
                Regex::new(&format!(r#"["'`][^"'`\n]*{w}[^"'`\n]*["'`]"#)).unwrap(),
            )
        })
        .collect();
    let bare = Regex::new(r#"["'`][^"'`\n]*出错[^了][^"'`\n]*["'`]|["'`]出错["'`]"#).unwrap();
    let mut out = BTreeMap::new();
    for p in paths {
        let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
        if fixture(&rel) || rel.ends_with("tests/guard.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        let body = code(&text);
        let lines: Vec<&str> = body
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("*"))
            .collect();
        let body = lines.join("\n");
        if !owner(&rel) {
            for (k, r) in &fields {
                let n = r.find_iter(&body).count();
                if n > 0 {
                    out.insert(format!("field {k}\t{rel}"), n);
                }
            }
        }
        // Status words belong to `Status::label` in core/status.rs alone.
        if rel != "crates/mooring-core/src/status.rs" {
            for (w, r) in &words {
                // 出错 without 了 is the drifted spelling; count it apart from 出错了.
                let n = r.find_iter(&body).count();
                if n > 0 {
                    out.insert(format!("word {w}\t{rel}"), n);
                }
            }
            let n = bare.find_iter(&body).count();
            if n > 0 {
                out.insert(format!("word 出错(无了)\t{rel}"), n);
            }
        }
    }
    out
}

#[test]
fn no_new_second_owner() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap();
    let now = count(&root);
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/guard-baseline.txt");
    let render = |m: &BTreeMap<String, usize>| {
        m.iter()
            .map(|(k, n)| format!("{k}\t{n}\n"))
            .collect::<String>()
    };
    if std::env::var_os("MOORING_GUARD_UPDATE").is_some() {
        std::fs::write(&file, render(&now)).unwrap();
        return;
    }
    let base: BTreeMap<String, usize> = std::fs::read_to_string(&file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (k, n) = l.rsplit_once('\t')?;
            Some((k.to_owned(), n.parse().ok()?))
        })
        .collect();
    let mut changes = vec![];
    for (k, n) in &now {
        match base.get(k) {
            Some(b) if b == n => (),
            Some(b) => changes.push(format!("{k}: {b} → {n}")),
            None => changes.push(format!("{k}: new, {n}")),
        }
    }
    for (k, b) in &base {
        if !now.contains_key(k) {
            changes.push(format!("{k}: {b} → 0"));
        }
    }
    assert!(
        changes.is_empty(),
        "raw reads or hand-written state words changed (read the owner in core instead — \
         status::of, session_title, session_updated, Status::label; if a change is right, \
         rewrite the baseline with MOORING_GUARD_UPDATE=1):\n{}",
        changes.join("\n")
    );
}
