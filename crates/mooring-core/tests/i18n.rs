//! The window speaks English to a reader who chose it (src/i18n.rs). Two checks:
//!
//! - `every_key_has_english`: each `t!("…")` and `tk!("…")` in the code has an entry in
//!   `locales/en/*.toml`, naming the same `{placeholders}`; no two tables disagree on a key,
//!   and no entry is left over from a string the code no longer has.
//! - `chinese_left_only_goes_down`: string literals with Chinese in them that are neither
//!   `t!` nor `not_ui!` are counted per file against `tests/i18n-baseline.txt`. A file may not
//!   gain one, nor a new file appear; after converting, `MOORING_GUARD_UPDATE=1 cargo test -p
//!   mooring-core --test i18n` lowers the numbers (it never raises one or adds a file).
//!
//! Text for logs and panics (`eprintln!`, `panic!`, `.expect(`…) is for us, not the reader,
//! and is not counted.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Where the window's words are written.
const DIRS: &[&str] = &["crates/mooring-app/src", "crates/mooring-core/src"];
/// Where `t!(…)` and `tk!(…)` keys may live (`mooring-host` also translates CLI text).
const KEY_DIRS: &[&str] = &[
    "crates/mooring-app/src",
    "crates/mooring-core/src",
    "crates/mooring-host/src",
];
/// Calls whose text is for developers.
const LOGS: &[&str] = &[
    "eprintln!",
    "println!",
    "eprint!",
    "print!",
    "panic!",
    "unreachable!",
    "todo!",
    "expect",
    "debug_assert!",
    "assert!",
    "assert_eq!",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Tests, screenshot fixtures and evidence runs build their own text on purpose.
fn fixture(rel: &str) -> bool {
    rel.contains("/tests/")
        || rel.ends_with("tests.rs")
        || rel.contains("_evidence")
        || rel.contains("/harness")
        || rel.contains("/workbench/visual")
}

fn sources(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(root, &p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if !fixture(&rel) {
                out.push((rel, std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
}

fn sources_in(dirs: &[&str]) -> Vec<(String, String)> {
    let root = root();
    let mut out = vec![];
    for d in dirs {
        sources(&root, &root.join(d), &mut out);
    }
    out.sort();
    out
}

fn all_sources() -> Vec<(String, String)> {
    sources_in(DIRS)
}

/// A string literal: its value, its line and the call it is the first argument of
/// (`t!`, `format!`, `expect`…), if any.
struct Lit {
    value: String,
    line: usize,
    call: String,
}

fn cjk(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{3000}'..='\u{303f}' | '\u{ff00}'..='\u{ffef}')
    })
}

fn ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The code before a `#[cfg(test)] mod … {`, which in this codebase runs to the end of its
/// file; a `#[cfg(test)] mod x;` line is kept going past.
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

/// The string literals of a Rust file, comments and char literals skipped.
fn literals(text: &str) -> Vec<Lit> {
    let s: Vec<char> = code(text).chars().collect();
    let mut out = vec![];
    let (mut i, mut line) = (0, 1);
    while i < s.len() {
        let c = s[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c == '/' && s.get(i + 1) == Some(&'/') {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && s.get(i + 1) == Some(&'*') {
            let mut depth = 0;
            while i < s.len() {
                if s[i] == '/' && s.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if s[i] == '*' && s.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    line += usize::from(s[i] == '\n');
                    i += 1;
                }
            }
        } else if c == '\'' {
            // A char literal ('a', '中', '\n', '\u{1F600}'), or a lifetime ('a).
            if s.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < s.len() && s[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if s.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if c == '"' || (matches!(c, 'r' | 'b' | 'c') && (i == 0 || !ident(s[i - 1]))) {
            // Prefixes: r"…", r#"…"#, b"…", br"…", c"…".
            let start = i;
            let mut j = i;
            while j < s.len() && matches!(s[j], 'r' | 'b' | 'c') && j - i < 2 {
                j += 1;
            }
            let raw = s[i..j].contains(&'r');
            let mut hashes = 0;
            while raw && s.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if s.get(j) != Some(&'"') {
                // An identifier starting with r, b or c.
                while i < s.len() && ident(s[i]) {
                    i += 1;
                }
                continue;
            }
            let at = line;
            j += 1;
            let mut value = String::new();
            while let Some(&ch) = s.get(j) {
                if ch == '"'
                    && s[j + 1..]
                        .iter()
                        .take(hashes)
                        .filter(|h| **h == '#')
                        .count()
                        == hashes
                {
                    j += 1 + hashes;
                    break;
                }
                if ch == '\n' {
                    line += 1;
                }
                if ch == '\\' && !raw {
                    j += 1;
                    match s.get(j) {
                        Some('n') => value.push('\n'),
                        Some('t') => value.push('\t'),
                        Some('r') => value.push('\r'),
                        Some('0') => value.push('\0'),
                        Some('u') => {
                            let end = s[j..].iter().position(|c| *c == '}').unwrap_or(0) + j;
                            let hex: String = s[j + 2..end].iter().collect();
                            if let Some(c) =
                                u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                            {
                                value.push(c);
                            }
                            j = end;
                        }
                        Some('\n') => {
                            line += 1;
                            while s.get(j + 1).is_some_and(|c| c.is_whitespace()) {
                                line += usize::from(s[j + 1] == '\n');
                                j += 1;
                            }
                        }
                        Some(&other) => value.push(other),
                        None => break,
                    }
                    j += 1;
                    continue;
                }
                value.push(ch);
                j += 1;
            }
            out.push(Lit {
                value,
                line: at,
                call: call_before(&s[..start]),
            });
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// `t!` for `t!(` right before a literal, `expect` for `.expect(`; empty otherwise.
fn call_before(before: &[char]) -> String {
    let mut k = before.len();
    while k > 0 && before[k - 1].is_whitespace() {
        k -= 1;
    }
    if k == 0 || before[k - 1] != '(' {
        return String::new();
    }
    k -= 1;
    while k > 0 && before[k - 1].is_whitespace() {
        k -= 1;
    }
    let end = k;
    if k > 0 && before[k - 1] == '!' {
        k -= 1;
    }
    while k > 0 && ident(before[k - 1]) {
        k -= 1;
    }
    before[k..end].iter().collect()
}

fn counted(lit: &Lit) -> bool {
    cjk(&lit.value)
        && !matches!(lit.call.as_str(), "t!" | "tk!" | "not_ui!")
        && !LOGS.contains(&lit.call.as_str())
}

#[test]
fn every_key_has_english() {
    use mooring_core::i18n;
    let mut problems = vec![];
    // The tables read, and agree with each other.
    let mut seen: BTreeMap<String, (String, &str)> = BTreeMap::new();
    for (file, rows) in i18n::files() {
        match rows {
            Err(line) => problems.push(format!("locales/en/{file}: cannot read {line}")),
            Ok(rows) => {
                for (k, v) in rows {
                    if let Some((other, at)) = seen.get(&k)
                        && *other != v
                    {
                        problems.push(format!(
                            "\"{k}\": {at} says \"{other}\", {file} says \"{v}\""
                        ));
                    }
                    let (a, b): (BTreeSet<_>, BTreeSet<_>) = (
                        i18n::placeholders(&k).into_iter().collect(),
                        i18n::placeholders(&v).into_iter().collect(),
                    );
                    if a != b {
                        problems.push(format!(
                            "locales/en/{file}: \"{k}\" names {a:?}, its English {b:?}"
                        ));
                    }
                    if v.trim().is_empty() || cjk(&v) {
                        problems.push(format!(
                            "locales/en/{file}: \"{k}\" has no English: \"{v}\""
                        ));
                    }
                    seen.insert(k, (v, file));
                }
            }
        }
    }
    let mut used = BTreeSet::new();
    for (rel, text) in sources_in(KEY_DIRS) {
        for lit in literals(&text) {
            if lit.call != "t!" && lit.call != "tk!" {
                continue;
            }
            if i18n::english(&lit.value).is_none() {
                problems.push(format!(
                    "{rel}:{}: no English for \"{}\" (add it to a locales/en/*.toml)",
                    lit.line,
                    lit.value.escape_debug()
                ));
            }
            used.insert(lit.value);
        }
    }
    for (k, (_, file)) in &seen {
        if !used.contains(k) {
            problems.push(format!(
                "locales/en/{file}: \"{}\" is not used by any t!(…) (remove it)",
                k.escape_debug()
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

fn count() -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (rel, text) in all_sources() {
        let n = literals(&text).iter().filter(|l| counted(l)).count();
        if n > 0 {
            out.insert(rel, n);
        }
    }
    out
}

#[test]
fn chinese_left_only_goes_down() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/i18n-baseline.txt");
    let now = count();
    let base: BTreeMap<String, usize> = std::fs::read_to_string(&file)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let (k, n) = l.rsplit_once('\t')?;
            Some((k.to_owned(), n.parse().ok()?))
        })
        .collect();
    let mut grown = vec![];
    let mut lowered = base.clone();
    for (k, n) in &now {
        match base.get(k) {
            Some(b) if n > b => grown.push(format!("{k}: {b} → {n}")),
            Some(_) => {
                lowered.insert(k.clone(), *n);
            }
            None => grown.push(format!("{k}: new, {n}")),
        }
    }
    lowered.retain(|k, _| now.contains_key(k));
    let render = |m: &BTreeMap<String, usize>| {
        let total: usize = m.values().sum();
        format!(
            "# Chinese string literals in the window's code not yet wrapped in t!(…) or \
             not_ui!(…), per file ({total} in all). Only goes down: tests/i18n.rs.\n{}",
            m.iter()
                .map(|(k, n)| format!("{k}\t{n}\n"))
                .collect::<String>()
        )
    };
    // A missing baseline is written whole once (a reviewer sees the new file).
    if std::env::var_os("MOORING_GUARD_UPDATE").is_some() && !file.exists() {
        std::fs::write(&file, render(&now)).unwrap();
        return;
    }
    if std::env::var_os("MOORING_GUARD_UPDATE").is_some() && grown.is_empty() {
        std::fs::write(&file, render(&lowered)).unwrap();
        return;
    }
    assert!(
        grown.is_empty(),
        "Chinese text the window shows must go through t!(\"…\") with an English entry in \
         crates/mooring-core/locales/en/ (or not_ui!(\"…\") when it is data, not words):\n{}",
        grown.join("\n")
    );
    assert!(
        lowered == base,
        "fewer untranslated strings than tests/i18n-baseline.txt says; lower it with \
         MOORING_GUARD_UPDATE=1 cargo test -p mooring-core --test i18n"
    );
}

#[test]
fn the_scanner_reads_rust() {
    let text = r####"
// "注释"
/* "块" /* "嵌套" */ */
let a = t!("新建");
let b = t!( "{n} 个", n);
let c = format!("共 {} 个", x);
let d = 'x'; let e = '中'; fn f<'a>(x: &'a str) {}
let g = r#"原"始"#;
let h = "a\
         b\u{4e2d}";
eprintln!("日志");
x.expect("不会");
let i = not_ui!("数据");
#[cfg(test)]
mod tests { "测试" }
"####;
    let lits = literals(text);
    let got: Vec<(&str, &str)> = lits
        .iter()
        .map(|l| (l.value.as_str(), l.call.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("新建", "t!"),
            ("{n} 个", "t!"),
            ("共 {} 个", "format!"),
            ("原\"始", ""),
            ("ab中", ""),
            ("日志", "eprintln!"),
            ("不会", "expect"),
            ("数据", "not_ui!"),
        ]
    );
    assert_eq!(lits.iter().filter(|l| counted(l)).count(), 3);
    assert_eq!(lits[4].line, 9);
}
