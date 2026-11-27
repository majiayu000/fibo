//! A ratchet on the two ways a fact grows a second owner (docs/single-source):
//! reading the raw fields core decides from (a status, a name, a time) outside the
//! modules that own them, and spelling a state's word by hand instead of taking it
//! from `Status::label`. Every such use is counted per file against
//! `tests/guard-baseline.txt`; any change fails until the baseline is rewritten
//! (`MOORING_GUARD_UPDATE=1 cargo test -p fibo-core --test guard`), so a new one
//! is a choice a reviewer sees, and a removed one lowers the bar for good.
//! The sidebar's selection is the third: which row is lit comes from `in_front` alone.
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
    // A large surface painted solid hides a see-through theme's desktop: a page or column
    // takes `T::surface`, and only what floats above them (a dialog, a card) stays solid.
    (
        "solid-surface",
        r"\.bg\(T::color\((T|F)::(app|pane|panel|sidebar|header|bg)\(\)\)\)",
    ),
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
    path.starts_with("crates/fibo-core/")
        || path.starts_with("crates/fibo-host/src/content/")
        || path.starts_with("crates/fibo-host/src/agents/")
        || path.ends_with("/log_index.rs")
        || ["update", "recovery", "elide"]
            .iter()
            .any(|m| module(path, &format!("/log_index/{m}")))
        || path.ends_with("/store.rs")
}
/// `m.rs` or anything under `m/`: a file split out of a module keeps its place here.
fn module(path: &str, m: &str) -> bool {
    path.ends_with(&format!("{m}.rs")) || path.contains(&format!("{m}/"))
}
/// Tests and screenshot fixtures build raw states on purpose.
fn fixture(path: &str) -> bool {
    path.contains("/tests/")
        || path.ends_with("tests.rs")
        || module(path, "crates/fibo-app/src/harness")
        || module(path, "crates/fibo-app/src/workbench/visual")
        || path.contains("/test/")
        || path.ends_with("mobile/src/fixture.ts")
}

fn files(root: &Path, dir: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let rel = p
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
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

/// An out-of-line test module does not hide the product code after it. Inline test
/// modules are at the end of their file, as in the i18n scanner.
fn code(text: &str) -> &str {
    let mut from = 0;
    while let Some(i) = text[from..].find("#[cfg(test)]") {
        let at = from + i;
        let rest = text[at + 12..].trim_start();
        let inline = rest
            .strip_prefix("mod ")
            .and_then(|r| r.find(['{', ';']).map(|j| r.as_bytes()[j] == b'{'))
            .unwrap_or(false);
        if inline {
            return &text[..at];
        }
        from = at + 12;
    }
    text
}

#[test]
fn out_of_line_test_modules_do_not_hide_product_code() {
    let product = "#[cfg(test)]\nmod tests;\nfn product() { session.turn; }\n";
    assert_eq!(code(product), product);
    assert_eq!(
        code(&format!(
            "{product}#[cfg(test)]\nmod inline {{ session.turn; }}"
        )),
        product
    );
    let helper = "#[cfg(test)]\nfn helper() {}\nfn product() { session.turn; }";
    assert_eq!(code(helper), helper);
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
    // A shortcut spelled in a string instead of read from the keymap (`keycap`).
    let key = Regex::new(r#"["'`][^"'`\n]*⌘[^"'`\n]*["'`]"#).unwrap();
    // What is in front, read in the sidebar from the parts `in_front` decides it by: a peek,
    // the machine drawn here, the focused pane. A row lit by one of them alone stays dark
    // (or stays lit) when the view went another way: 11-06 48ad1c35, 11-15 52971020, 11-27.
    let front =
        Regex::new(r"\bself\.peek\b|\.nav\.focused\b|\bhosts\.showing\b|\bhosted_(focus|view)\(")
            .unwrap();
    let bare = Regex::new(r#"["'`][^"'`\n]*出错[^了][^"'`\n]*["'`]|["'`]出错["'`]"#).unwrap();
    let mut out = BTreeMap::new();
    for p in paths {
        let rel = p
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
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
        if rel.starts_with("crates/fibo-app/src/workbench/sidebar") {
            let n = front.find_iter(&body).count();
            if n > 0 {
                out.insert(format!("in-front\t{rel}"), n);
            }
        }
        // The keymap reads chords; ui/words.rs says a label's Mac spelling in this system's words.
        if !rel.ends_with("keymap.rs") && !rel.ends_with("ui/words.rs") {
            let n = key.find_iter(&body).count();
            if n > 0 {
                out.insert(format!("keycap\t{rel}"), n);
            }
        }
        // Status words belong to `Status::label` in core/status.rs alone.
        if rel != "crates/fibo-core/src/status.rs" {
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
         status::of, session_title, session_updated, Status::label; which sidebar row is lit, \
         in_front; a solid-surface is a large \
         surface that should take T::surface; if a change is right, \
         rewrite the baseline with MOORING_GUARD_UPDATE=1):\n{}",
        changes.join("\n")
    );
}
