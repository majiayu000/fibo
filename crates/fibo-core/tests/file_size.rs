//! No source file grows past 800 lines, however it was written. The editor hook only
//! sees an Agent's Edit and Write calls; a script run from a shell, another Agent or a
//! person goes around it, so the rule is checked here on what actually landed.
//!
//! Files already over the limit are listed in `tests/file-size-baseline.txt` and can
//! only get shorter. `MOORING_GUARD_UPDATE=1 cargo test -p fibo-core --test file_size`
//! lowers the listed numbers after a file shrinks; it never raises one or adds a file,
//! so running it can't excuse growth. Raising a number takes a hand edit a reviewer sees.
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

const MAX_LINES: usize = 800;
const SOURCE: &[&str] = &[
    "rs", "py", "ts", "tsx", "js", "jsx", "mjs", "cjs", "go", "java", "kt", "kts", "swift", "c",
    "h", "cc", "cpp", "hpp", "m", "mm", "cs", "rb", "php", "scala", "lua", "dart", "vue", "svelte",
    "zig", "ex", "exs", "sh", "bash", "zsh",
];

type Sizes = BTreeMap<String, usize>;

/// Every source file over the limit: tracked, or new and not ignored.
fn oversized(root: &Path) -> Sizes {
    let out = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    let mut sizes = Sizes::new();
    for rel in String::from_utf8_lossy(&out.stdout).split('\0') {
        let path = Path::new(rel);
        let source = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| SOURCE.contains(&e.to_ascii_lowercase().as_str()));
        if !source {
            continue;
        }
        // A file deleted from the working tree is still in the index.
        let Ok(bytes) = std::fs::read(root.join(path)) else {
            continue;
        };
        let lines = String::from_utf8_lossy(&bytes).lines().count();
        if lines > MAX_LINES {
            sizes.insert(rel.to_owned(), lines);
        }
    }
    sizes
}

/// The baseline with only what `now` lets it lower: a shorter file's number, and
/// files no longer over the limit dropped.
fn lowered(base: &Sizes, now: &Sizes) -> Sizes {
    base.iter()
        .filter_map(|(file, b)| Some((file.clone(), (*now.get(file)?).min(*b))))
        .collect()
}

fn problems(base: &Sizes, now: &Sizes) -> Vec<String> {
    let mut out = vec![];
    for (file, n) in now {
        match base.get(file) {
            None => out.push(format!(
                "{file}: {n} lines, over the {MAX_LINES}-line limit; split it by responsibility"
            )),
            Some(b) if n > b => out.push(format!(
                "{file}: grew from {b} to {n} lines; it can only get shorter, move the new code \
                 to its own file"
            )),
            Some(b) if n < b => out.push(format!(
                "{file}: now {n} lines, baseline says {b}; lower it with MOORING_GUARD_UPDATE=1"
            )),
            Some(_) => (),
        }
    }
    for file in base.keys().filter(|f| !now.contains_key(*f)) {
        out.push(format!(
            "{file}: no longer over the limit; drop it with MOORING_GUARD_UPDATE=1"
        ));
    }
    out
}

fn render(sizes: &Sizes) -> String {
    sizes.iter().map(|(f, n)| format!("{f}\t{n}\n")).collect()
}

#[test]
fn no_source_file_grows_past_the_limit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/file-size-baseline.txt");
    let mut base: Sizes = std::fs::read_to_string(&file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (f, n) = l.rsplit_once('\t')?;
            Some((f.to_owned(), n.parse().ok()?))
        })
        .collect();
    let now = oversized(&root);
    if std::env::var_os("MOORING_GUARD_UPDATE").is_some() {
        base = lowered(&base, &now);
        std::fs::write(&file, render(&base)).unwrap();
    }
    let found = problems(&base, &now);
    assert!(found.is_empty(), "file sizes:\n{}", found.join("\n"));
}

fn sizes(list: &[(&str, usize)]) -> Sizes {
    list.iter().map(|(f, n)| (f.to_string(), *n)).collect()
}

#[test]
fn growth_and_new_files_fail_even_after_an_update() {
    let base = sizes(&[("a.rs", 1000)]);
    let now = sizes(&[("a.rs", 1001), ("b.rs", 801)]);
    let after = lowered(&base, &now);
    assert_eq!(
        after, base,
        "an update must not raise a number or add a file"
    );
    let found = problems(&after, &now);
    assert!(found[0].contains("grew from 1000 to 1001"), "{found:?}");
    assert!(found[1].contains("b.rs: 801 lines"), "{found:?}");
}

#[test]
fn shrinking_lowers_the_bar_for_good() {
    let base = sizes(&[("a.rs", 1000), ("b.rs", 900)]);
    let now = sizes(&[("a.rs", 950)]);
    // Stale until the baseline is lowered, so the freed lines can't quietly come back.
    assert_eq!(problems(&base, &now).len(), 2);
    let after = lowered(&base, &now);
    assert_eq!(after, sizes(&[("a.rs", 950)]));
    assert!(problems(&after, &now).is_empty());
    assert!(!problems(&after, &sizes(&[("a.rs", 960)])).is_empty());
}
