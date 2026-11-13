//! The words the window shows, in the language its reader chose. The Chinese text written in
//! the code is the key and the Chinese UI as it always was; English comes from the tables in
//! `locales/en/*.toml`, compiled in. A key with no English falls back to the Chinese, and
//! `crates/fibo-app/tests/i18n.rs` fails until it has one.
//!
//! - `t!("新建标签")` is a `&'static str`.
//! - `t!("{n} 个会话", n = count)` (or `t!("{n} 个会话", n)`) is a `String`; English may say
//!   `{n|session|sessions}`, the first word when `n` is 1, the second otherwise.
//! - `tk!("中文")` is the Chinese itself, for a `const` table or a match pattern where `t!`
//!   cannot run; it is checked for English like `t!`, and said at the point of use with
//!   [`say`].
//! - `t!("断开#button")`: a `#context` after the Chinese tells two meanings of the same words
//!   apart for English; Chinese shows without it.
//! - `not_ui!("…")` marks Chinese that is data rather than words for a reader (a phrase read
//!   off an Agent's screen, a file name), so the ratchet stops counting it.
//!
//! Only the window sets the language. Every other process (the host, the `fibo` command
//! Agents read) stays in Chinese until its own words are converted.
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    Zh,
    En,
}

/// What the reader asked for: follow the system, or one language whatever it says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Choice {
    #[default]
    Auto,
    Zh,
    En,
}
impl Choice {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Zh, Self::En];
    /// The word in `~/.config/mooring/language` and in `MOORING_LANG`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Zh => "zh",
            Self::En => "en",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key.trim())
    }
    /// The language this choice means, given the system's preferred languages in order.
    pub fn resolve(self, preferred: &[String]) -> Lang {
        match self {
            Self::Zh => Lang::Zh,
            Self::En => Lang::En,
            Self::Auto => system(preferred),
        }
    }
}

/// Chinese when the system's first language is a Chinese one, English for anything else.
pub fn system(preferred: &[String]) -> Lang {
    match preferred.first() {
        Some(first) if first.trim().to_ascii_lowercase().starts_with("zh") => Lang::Zh,
        Some(_) => Lang::En,
        // No answer from the system: keep the window as it has always been.
        None => Lang::Zh,
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 {
        Lang::En
    } else {
        Lang::Zh
    }
}
pub fn set(lang: Lang) {
    LANG.store(u8::from(lang == Lang::En), Ordering::Relaxed);
}

/// The words for `zh` in the current language.
pub fn text(zh: &'static str) -> &'static str {
    say(zh)
}

/// A key that is not a literal at the point of use (a `tk!` in a table) in the current
/// language; text that is no key comes back as it is.
pub fn say(zh: &str) -> &str {
    match lang() {
        Lang::Zh => chinese(zh),
        Lang::En => english(zh).unwrap_or_else(|| chinese(zh)),
    }
}

/// The Chinese a key shows: the key without its `#context`. The same Chinese can need two
/// English words (断开 the button is "Disconnect", 断开 the state "Offline"); the rarer one
/// is keyed `断开#button`, a lowercase ASCII word after the last `#`.
pub fn chinese(key: &str) -> &str {
    match key.rsplit_once('#') {
        Some((zh, ctx)) if !ctx.is_empty() && ctx.bytes().all(|b| b.is_ascii_lowercase()) => zh,
        _ => key,
    }
}

/// `zh` in the current language with its `{name}`s filled from `args`.
pub fn format(zh: &'static str, args: &[(&str, &dyn Display)]) -> String {
    fill(text(zh), args)
}

/// The English for a key, if the tables have it.
pub fn english(zh: &str) -> Option<&'static str> {
    table().get(zh).copied()
}

/// Every key the tables hold, with its English and the file it came from.
pub fn entries() -> impl Iterator<Item = (&'static str, &'static str)> {
    table().iter().map(|(k, v)| (*k, *v))
}

/// The `{name}`s a text names, plain or chosen by number, in order.
pub fn placeholders(text: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        if rest[i..].starts_with("{{") {
            rest = &rest[i + 2..];
            continue;
        }
        let Some(j) = rest[i..].find('}') else { break };
        let inner = &rest[i + 1..i + j];
        out.push(inner.split('|').next().unwrap_or(inner));
        rest = &rest[i + j + 1..];
    }
    out
}

fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        let Some(j) = tail.find('}').filter(|_| tail.starts_with('{')) else {
            out.push_str(&tail[..1]);
            rest = &tail[1..];
            continue;
        };
        let inner = &tail[1..j];
        let mut parts = inner.split('|');
        let name = parts.next().unwrap_or_default();
        let value = args
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.to_string());
        match (value, parts.next(), parts.next()) {
            (Some(v), Some(one), Some(other)) => {
                out.push_str(if v.trim() == "1" { one } else { other })
            }
            (Some(v), _, _) => out.push_str(&v),
            (None, ..) => out.push_str(&tail[..=j]),
        }
        rest = &tail[j + 1..];
    }
    out.push_str(rest);
    out
}

/// `(file, contents)` for every table under `locales/en/`, listed by build.rs.
const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/locales.rs"));

fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut map = HashMap::new();
        for (_, text) in FILES {
            for (k, v) in parse(text).into_iter().flatten() {
                map.insert(
                    &*Box::leak(k.into_boxed_str()),
                    &*Box::leak(v.into_boxed_str()),
                );
            }
        }
        map
    })
}

/// A table's `(中文, English)` rows, or the first line it cannot read.
pub type Rows = Result<Vec<(String, String)>, String>;

/// Every table file with its entries or the first line it cannot read.
pub fn files() -> impl Iterator<Item = (&'static str, Rows)> {
    FILES.iter().map(|(name, text)| (*name, parse(text)))
}

/// The subset of TOML the tables use: `"中文" = "English"` per line, `#` comments and
/// `[section]` headers (ignored, for grouping), basic strings with TOML's escapes.
fn parse(text: &str) -> Rows {
    let mut out = vec![];
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let bad = || format!("line {}: {line}", n + 1);
        let (key, rest) = string(line).ok_or_else(bad)?;
        let rest = rest.trim_start().strip_prefix('=').ok_or_else(bad)?;
        let (value, rest) = string(rest.trim_start()).ok_or_else(bad)?;
        let rest = rest.trim();
        if !(rest.is_empty() || rest.starts_with('#')) {
            return Err(bad());
        }
        out.push((key, value));
    }
    Ok(out)
}

/// A `"…"` at the start of `s`, unescaped, and what follows it.
fn string(s: &str) -> Option<(String, &str)> {
    let mut chars = s.strip_prefix('"')?.char_indices();
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &s[i + 2..])),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'u' => {
                    let hex: String = (0..4).filter_map(|_| chars.next().map(|c| c.1)).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

/// Words for the reader: `t!("中文")` is the text in the current language; with arguments
/// (`t!("{n} 个", n = count)` or `t!("{n} 个", n)`) it is a `String` with them filled in.
#[macro_export]
macro_rules! t {
    (@value $name:ident $value:expr) => {
        $value
    };
    (@value $name:ident) => {
        $name
    };
    ($zh:literal) => {
        $crate::i18n::text($zh)
    };
    ($zh:literal, $($name:ident $(= $value:expr)?),+ $(,)?) => {
        $crate::i18n::format(
            $zh,
            &[$((stringify!($name), &$crate::t!(@value $name $($value)?) as &dyn ::std::fmt::Display)),+],
        )
    };
}

/// A key kept as the Chinese itself where `t!` cannot run (a `const` table, a pattern), to be
/// said with `i18n::say` where it is shown. The tests check it has English like `t!`.
#[macro_export]
macro_rules! tk {
    ($zh:literal) => {
        $zh
    };
}

/// Chinese that is data, not words for a reader (read off a screen, matched, stored): the
/// literal itself, marked so the i18n ratchet does not count it.
#[macro_export]
macro_rules! not_ui {
    ($zh:literal) => {
        $zh
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_the_first_preferred_language() {
        let langs = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            Choice::Auto.resolve(&langs(&["zh-Hans-CN", "en"])),
            Lang::Zh
        );
        assert_eq!(Choice::Auto.resolve(&langs(&["zh-Hant-TW"])), Lang::Zh);
        assert_eq!(
            Choice::Auto.resolve(&langs(&["en-US", "zh-Hans"])),
            Lang::En
        );
        assert_eq!(Choice::Auto.resolve(&langs(&["ja-JP"])), Lang::En);
        assert_eq!(Choice::Auto.resolve(&[]), Lang::Zh);
        assert_eq!(Choice::En.resolve(&langs(&["zh-Hans"])), Lang::En);
        assert_eq!(Choice::Zh.resolve(&langs(&["en"])), Lang::Zh);
    }

    #[test]
    fn fills_names_and_picks_by_number() {
        let n = 1;
        assert_eq!(
            fill("{n|session|sessions} · {n}", &[("n", &n)]),
            "session · 1"
        );
        assert_eq!(fill("{n} {n|tab|tabs}", &[("n", &3)]), "3 tabs");
        assert_eq!(fill("{{x}} {y}", &[]), "{x} {y}");
        assert_eq!(placeholders("{a} {b|x|y} {{c}}"), ["a", "b"]);
        assert_eq!(chinese("断开#button"), "断开");
        assert_eq!(chinese("第 #1"), "第 #1");
    }

    #[test]
    fn reads_the_toml_subset() {
        let rows = parse("# c\n[s]\n\"新\\\"建\" = \"New \\u00e9\" # c\n").unwrap();
        assert_eq!(rows, [("新\"建".to_string(), "New é".to_string())]);
        assert!(parse("\"a\" = b").is_err());
    }
}
