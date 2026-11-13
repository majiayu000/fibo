//! Keeps system code in this crate. Outside it, each kind of system call, platform branch and
//! platform dependency is counted per file against `tests/boundary-baseline.txt`; any change
//! fails until the baseline is rewritten
//! (`MOORING_GUARD_UPDATE=1 cargo test -p fibo-platform --test boundary`), so a new one is a
//! choice a reviewer sees, and one moved in here lowers the bar for good. Same idea as
//! fibo-core's tests/guard.rs.
use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What only this crate may write.
const RULES: &[(&str, &str)] = &[
    (
        "cfg",
        r"\bcfg(?:_attr)?!?\([^\n]*\b(?:target_os|target_family|target_vendor|unix|windows)\b",
    ),
    ("libc", r"\blibc::"),
    (
        "os-api",
        r"\bstd::os::(?:unix|windows|macos|linux|fd)\b|\bUnix(?:Stream|Listener|Datagram)\b",
    ),
    (
        "unix-path",
        r#""/bin/(?:sh|zsh|bash)"|"/usr/bin[/:]|"/tmp\b|"/private/tmp\b"#,
    ),
    (
        "apple",
        r#"\b(?:osascript|pbcopy|launchctl|kqueue|kevent|proc_pid[a-z]*|pmset)\b|"Library/"#,
    ),
    ("home", r#"env::var(?:_os)?\("(?:HOME|XDG_CONFIG_HOME)"\)"#),
];
/// Dependencies that are a system's API.
const SYSTEM_DEPS: &str =
    r"(?m)^\s*(libc|windows-sys|windows|nix|mach2?|rustix|objc2?[-\w]*|cocoa|core-foundation)\b";

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// A test module, which in this codebase runs to the end of its file, or a whole test file:
/// a test may set up what it needs. Screenshot scenes are test code in another form.
fn test_only(rel: &str) -> bool {
    rel.contains("/tests/")
        || rel.ends_with("tests.rs")
        || rel.contains("/examples/")
        || rel.contains("_evidence")
        || rel.contains("/harness")
        || rel.contains("/workbench/visual")
}

/// The file without comments and without the test module that ends it.
fn code(text: &str, test_module: &Regex) -> String {
    let body = test_module.find(text).map_or(text, |m| &text[..m.start()]);
    body.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn count(root: &Path) -> BTreeMap<String, usize> {
    let rules: Vec<(&str, Regex)> = RULES
        .iter()
        .map(|(k, r)| (*k, Regex::new(r).unwrap()))
        .collect();
    let deps = Regex::new(SYSTEM_DEPS).unwrap();
    let test_module =
        Regex::new(r"#\[cfg\((?:all\()?test\b[^\]]*\]\s*(?:pub(?:\([a-z]+\))? )?mod \w+ \{")
            .unwrap();
    let mut paths = vec![];
    collect(&root.join("crates"), &mut paths);
    let mut out = BTreeMap::new();
    for path in paths {
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with("crates/fibo-platform/") {
            continue;
        }
        if rel.ends_with("/Cargo.toml") {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for found in deps.captures_iter(&text) {
                *out.entry(format!("dep {}\t{rel}", &found[1])).or_insert(0) += 1;
            }
        } else if rel.ends_with(".rs") && !test_only(&rel) {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let body = code(&text, &test_module);
            for (kind, rule) in &rules {
                let n = rule.find_iter(&body).count();
                if n > 0 {
                    out.insert(format!("{kind}\t{rel}"), n);
                }
            }
        }
    }
    out
}

#[test]
fn system_code_stays_in_this_crate() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..").canonicalize().unwrap();
    let now = count(&root);
    let file = manifest.join("tests/boundary-baseline.txt");
    if std::env::var_os("MOORING_GUARD_UPDATE").is_some() {
        let text: String = now.iter().map(|(k, n)| format!("{k}\t{n}\n")).collect();
        std::fs::write(&file, text).unwrap();
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
        "system code outside fibo-platform changed. Put a new system call behind its interface \
         instead, so a change for one platform cannot reach the others (docs/platform/index.html). \
         If the count went down, or this is right, rewrite the baseline with \
         MOORING_GUARD_UPDATE=1:\n{}",
        changes.join("\n")
    );
}
