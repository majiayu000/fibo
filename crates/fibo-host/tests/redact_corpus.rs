//! How the rules fare on this machine's real logs: `cargo test -p fibo-host --test
//! redact_corpus -- --ignored --nocapture`. Prints counts by rule and by the shape of what
//! they hid or missed, never a name or a value from a log; the corpus stays where it is.
use fibo_host::redact::{redact_with, HIDDEN};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

fn logs(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && depth < 8 {
            logs(&p, depth + 1, out);
        } else if p.extension().is_some_and(|x| x == "jsonl") {
            out.push(p);
        }
    }
}
fn newest(root: &Path, n: usize) -> Vec<PathBuf> {
    let mut all = Vec::new();
    logs(root, 0, &mut all);
    all.sort_by_key(|p| std::cmp::Reverse(p.metadata().and_then(|m| m.modified()).ok()));
    all.truncate(n);
    all
}
/// The strings a tool's input and output hold.
fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => (),
    }
}
fn tool_texts(line: &Value, out: &mut Vec<String>) {
    let ty = line["type"].as_str().unwrap_or("");
    if ty == "user" || ty == "assistant" {
        if let Some(blocks) = line["message"]["content"].as_array() {
            for b in blocks {
                match b["type"].as_str() {
                    Some("tool_use") => out.push(b["input"].to_string()),
                    Some("tool_result") => strings(&b["content"], out),
                    _ => (),
                }
            }
        }
    }
    let p = &line["payload"];
    let item = if p["type"] == "item_completed" {
        &p["item"]
    } else {
        p
    };
    for key in ["arguments", "input", "output", "aggregated_output"] {
        if matches!(
            item["type"].as_str(),
            Some(
                "function_call"
                    | "custom_tool_call"
                    | "function_call_output"
                    | "custom_tool_call_output"
                    | "CommandExecution"
            )
        ) {
            strings(&item[key], out);
        }
    }
}
fn entropy(s: &str) -> f64 {
    let mut n = BTreeMap::new();
    for c in s.chars() {
        *n.entry(c).or_insert(0usize) += 1;
    }
    let total = s.chars().count() as f64;
    n.values()
        .map(|c| {
            let p = *c as f64 / total;
            -p * p.log2()
        })
        .sum()
}
/// Shape only: enough to judge a rule, nothing to read a secret from.
fn shape(value: &str) -> String {
    let inner = value.trim_matches(|c| "\\\"'`".contains(c));
    let word_like = inner
        .chars()
        .all(|c| c.is_ascii_lowercase() || "-_ ".contains(c));
    let path_like = inner.contains('/') || inner.contains('\\');
    let e = entropy(inner);
    format!(
        "{}{}",
        match (word_like, path_like, inner.len(), e) {
            (true, ..) => "words",
            (_, true, ..) => "path-like",
            (_, _, 0..=7, _) => "short",
            (_, _, 8..=15, x) if x < 3.0 => "low-entropy",
            (_, _, 8..=15, _) => "mid",
            (_, _, _, x) if x >= 3.7 => "random",
            _ => "long-low-entropy",
        },
        if inner.len() == value.len() {
            ""
        } else {
            " (quoted)"
        },
    )
}

/// What a long run left on a line naming a secret looks like: only the last kinds could be one.
fn run_kind(run: &str) -> &'static str {
    let digits = run.chars().filter(char::is_ascii_digit).count();
    let upper = run.chars().any(|c| c.is_ascii_uppercase());
    let lower = run.chars().any(|c| c.is_ascii_lowercase());
    let hex = run.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    let seps = run.chars().filter(|c| "_-".contains(*c)).count();
    if hex {
        "hex or uuid"
    } else if digits == 0 {
        "identifier, no digits"
    } else if seps >= 2 || (digits * 8 < run.len()) {
        "identifier with some digits"
    } else if upper && lower {
        "random-looking mixed case"
    } else {
        "random-looking one case"
    }
}

#[test]
#[ignore = "reads this machine's real logs; counts only"]
fn the_rules_on_real_logs() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let files: Vec<_> = (newest(&home.join(".claude/projects"), 120).into_iter())
        .chain(newest(&home.join(".codex/sessions"), 80))
        .collect();
    let keyword =
        Regex::new(r"(?i)(token|secret|password|passwd|api[_-]?key|authorization|credential)")
            .unwrap();
    let long = Regex::new(r"[A-Za-z0-9+/_=-]{20,}").unwrap();
    let known = Regex::new(
        r"\b(sk-[A-Za-z0-9]|ghp_|gho_|ghs_|github_pat_|xox[baprs]-|AKIA[0-9A-Z]{8}|AIza[0-9A-Za-z]|glpat-|-----BEGIN [A-Z ]*PRIVATE KEY)",
    )
    .unwrap();
    let (mut texts, mut bytes, mut touched) = (0u64, 0u64, 0u64);
    let mut by_rule: BTreeMap<String, (u64, HashSet<u64>)> = BTreeMap::new();
    let mut by_shape: BTreeMap<String, u64> = BTreeMap::new();
    let mut left_known: BTreeMap<String, u64> = BTreeMap::new();
    let (mut left_random, mut random_lines) = (0u64, HashSet::new());
    let mut left_shape: BTreeMap<String, u64> = BTreeMap::new();
    let mut named_random = 0u64;
    let hash = |s: &str| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut h);
        h.finish()
    };
    let started = std::time::Instant::now();
    for path in &files {
        let Ok(mut file) = File::open(path) else {
            continue;
        };
        // A log of gigabytes counts by its newest part.
        let len = file.metadata().map_or(0, |m| m.len());
        if len > 200 << 20 {
            let _ = file.seek(SeekFrom::Start(len - (200 << 20)));
        }
        let mut lines = BufReader::with_capacity(1 << 18, file);
        let mut line = Vec::new();
        loop {
            line.clear();
            match lines.read_until(b'\n', &mut line) {
                Ok(n) if n > 0 => (),
                _ => break,
            }
            let Ok(v) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            let mut found = Vec::new();
            tool_texts(&v, &mut found);
            for text in found {
                texts += 1;
                bytes += text.len() as u64;
                let out = redact_with(&text, &mut |rule, _name, value| {
                    let e = by_rule.entry(rule.to_owned()).or_default();
                    e.0 += 1;
                    e.1.insert(hash(value));
                    *by_shape
                        .entry(format!("{rule}: {}", shape(value)))
                        .or_default() += 1;
                });
                if out.contains(HIDDEN) {
                    touched += 1;
                }
                for m in known.find_iter(&out) {
                    let prefix: String = m.as_str().chars().take(4).collect();
                    *left_known.entry(prefix).or_default() += 1;
                }
                for l in out.lines().filter(|l| keyword.is_match(l)) {
                    for m in long.find_iter(l) {
                        if entropy(m.as_str()) >= 4.0 && !m.as_str().contains('/') {
                            left_random += 1;
                            random_lines.insert(hash(l));
                            let before = l[..m.start()].trim_end_matches(|c| "\"' \\".contains(c));
                            let assigned = before.ends_with(['=', ':']);
                            let name = before.trim_end_matches(['=', ':', ' ', '"', '\'', '\\']);
                            let name = name
                                .rsplit(|c: char| !(c.is_alphanumeric() || "_-.".contains(c)))
                                .next()
                                .unwrap_or("");
                            let named = keyword.is_match(name);
                            if named && run_kind(m.as_str()).starts_with("random") {
                                named_random += 1;
                            }
                            *left_shape
                                .entry(format!(
                                    "{}{}",
                                    run_kind(m.as_str()),
                                    if assigned {
                                        " (right after = or :)"
                                    } else {
                                        ""
                                    }
                                ))
                                .or_default() += 1;
                        }
                    }
                }
            }
        }
        if started.elapsed().as_secs() > 400 {
            println!("(stopped after 400 s)");
            break;
        }
    }
    println!(
        "files {} · texts {texts} · {} MB · texts with something hidden {touched} ({:.2}%) · {:.0} s",
        files.len(),
        bytes >> 20,
        touched as f64 * 100.0 / texts.max(1) as f64,
        started.elapsed().as_secs_f64(),
    );
    for (rule, (n, distinct)) in &by_rule {
        println!("rule {rule}: {n} hits, {} distinct", distinct.len());
    }
    for (s, n) in &by_shape {
        println!("  {s}: {n}");
    }
    println!("known prefixes left after hiding: {left_known:?}");
    println!(
        "keyword line with a high-entropy run left: {left_random} ({} distinct lines)",
        random_lines.len()
    );
    println!("  random-looking right after a secret-sounding name: {named_random}");
    for (s, n) in &left_shape {
        println!("  {s}: {n}");
    }
}
