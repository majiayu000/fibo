//! A tool's text made ready to leave the machine, and cut to size. [`clean`] is the one way
//! in: terminal colours off, binary and base64 replaced by a note, secrets hidden. Everything
//! a phone reads of it is cut from that cleaned text, so a secret cannot straddle a cut and
//! offsets stay the same from one request to the next.
use crate::redact::redact;
use mooring_remote::protocol::{ToolSlice, ToolText};

/// A run of base64 this long is data, not something to read.
const BASE64_RUN: usize = 2048;

/// What a line holds, as bytes people read: `1.2 MB`.
pub fn bytes_word(n: u64) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / 1_048_576.0)
    } else {
        format!("{} KB", n.div_ceil(1024))
    }
}

/// Terminal escape sequences off; `\r\n` ends a line; a lone `\r` starts one over, kept as a
/// line of its own. Control characters read as nothing and would swell tenfold when escaped.
fn without_controls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.peek() {
                Some('[') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            },
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
            '\n' | '\t' => out.push(c),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => (),
            c => out.push(c),
        }
    }
    out
}

fn is_base64(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'-' | b'_')
}

/// Long runs of base64 and `data:` URLs, each a note saying how much was left out.
fn without_base64(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut at, mut kept) = (0, 0);
    while at < bytes.len() {
        if !is_base64(bytes[at]) {
            at += 1;
            continue;
        }
        let start = at;
        while at < bytes.len() && is_base64(bytes[at]) {
            at += 1;
        }
        if at - start >= BASE64_RUN {
            out.push_str(&text[kept..start]);
            out.push_str(&format!(
                "[二进制内容 · {}，手机不显示]",
                bytes_word((at - start) as u64)
            ));
            kept = at;
        }
    }
    out.push_str(&text[kept..]);
    out
}

/// Text ready to send: the single place secrets come out, before anything is cut.
pub fn clean(text: &str) -> String {
    let readable = without_base64(&without_controls(text));
    redact(&readable).into_owned()
}

/// Bytes `text` takes inside a JSON string: a reply's size is what counts, not the text's.
pub fn json_len(text: &str) -> usize {
    text.bytes()
        .map(|b| match b {
            b'"' | b'\\' | b'\n' | b'\t' | b'\r' | 8 | 12 => 2,
            0..=0x1f => 6,
            _ => 1,
        })
        .sum()
}

/// How far from `from` a stretch of at most `budget` JSON bytes reaches, on a character edge.
fn reach(text: &str, from: usize, budget: usize) -> usize {
    let mut used = 0;
    for (i, c) in text[from..].char_indices() {
        let mut buf = [0; 4];
        used += json_len(c.encode_utf8(&mut buf));
        if used > budget {
            return from + i;
        }
    }
    text.len()
}

/// A cut close to a line's end lands on it, so neither side starts mid-line.
fn on_line(text: &str, at: usize, slack: usize) -> usize {
    let from = at.saturating_sub(slack);
    (text.as_bytes()[from..at].iter().rposition(|b| *b == b'\n')).map_or(at, |i| from + i + 1)
}

/// The text's two ends, at most `head` and `tail` JSON bytes.
pub fn ends(text: &str, head: usize, tail: usize) -> ToolText {
    let total = text.len() as u64;
    if json_len(text) <= head + tail {
        return ToolText {
            total,
            head: text.to_owned(),
            tail: String::new(),
            tail_at: total,
        };
    }
    let head_end = on_line(text, reach(text, 0, head), 512);
    // From the back: the first character from which the rest fits.
    let mut used = 0;
    let mut tail_at = text.len();
    for (i, c) in text.char_indices().rev() {
        let mut buf = [0; 4];
        used += json_len(c.encode_utf8(&mut buf));
        if used > tail || i < head_end {
            break;
        }
        tail_at = i;
    }
    ToolText {
        total,
        head: text[..head_end].to_owned(),
        tail: text[tail_at..].to_owned(),
        tail_at: tail_at as u64,
    }
}

/// The stretch of `text` from `offset`, at most `budget` JSON bytes.
pub fn slice(part: &str, text: &str, offset: u64, budget: usize) -> ToolSlice {
    let mut from = (offset as usize).min(text.len());
    while !text.is_char_boundary(from) {
        from -= 1;
    }
    let mut end = reach(text, from, budget);
    if end < text.len() {
        end = on_line(text, end, 512).max(from + 1);
        while !text.is_char_boundary(end) {
            end += 1;
        }
    }
    ToolSlice {
        part: part.to_owned(),
        offset: from as u64,
        text: text[from..end].to_owned(),
        next: end as u64,
        total: text.len() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn terminal_colours_and_controls_go_lines_stay() {
        assert_eq!(
            clean("\u{1b}[31mred\u{1b}[0m\r\nnext\u{0}\tok"),
            "red\nnext\tok"
        );
        assert_eq!(clean("a\u{1b}]0;title\u{7}b"), "ab");
        assert_eq!(clean("10%\r20%"), "10%\n20%");
    }

    #[test]
    fn base64_becomes_a_note_and_short_words_do_not() {
        let data = "QUJD".repeat(1000);
        let out = clean(&format!("before data:image/png;base64,{data} after"));
        assert!(!out.contains(&data), "{}", out.len());
        assert!(out.contains("[二进制内容 · 4 KB，手机不显示]"), "{out}");
        assert!(out.ends_with(" after"));
        let word = "crates/mooring-host/src/remote/tools.rs";
        assert_eq!(clean(word), word);
    }

    /// Secrets are gone before any cut: one lying across a cut is still hidden.
    #[test]
    fn a_secret_across_a_cut_never_shows() {
        let secret = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let text = clean(&format!("{}{secret}{}", "x ".repeat(50), " y".repeat(50)));
        for budget in 1..=120 {
            let e = ends(&text, budget, budget);
            assert!(!e.head.contains("AAAA") && !e.tail.contains("AAAA"));
            let s = slice("output", &text, budget as u64, budget);
            assert!(!s.text.contains("AAAA"));
        }
    }

    #[test]
    fn pieces_fit_their_budget_and_add_up_to_the_text() {
        for sample in [
            "plain line\n".repeat(5000),
            "\"\\".repeat(20_000),
            "中文输出\n".repeat(8000),
            "🙂".repeat(9000),
        ] {
            let text = clean(&sample);
            let e = ends(&text, 4096, 2048);
            assert!(json_len(&e.head) <= 4096, "head {}", json_len(&e.head));
            assert!(json_len(&e.tail) <= 2048, "tail {}", json_len(&e.tail));
            assert_eq!(e.total, text.len() as u64);
            assert!(text.starts_with(&e.head) && text.ends_with(&e.tail));
            // Fetching on from the head's end reaches the tail, nothing lost or repeated.
            let mut got = e.head.clone();
            let mut seen = HashSet::new();
            while (got.len() as u64) < e.tail_at {
                let s = slice("output", &text, got.len() as u64, 4096);
                assert!(json_len(&s.text) <= 4096 + 8, "slice {}", json_len(&s.text));
                assert!(seen.insert(s.offset), "stuck at {}", s.offset);
                assert_eq!(s.offset as usize, got.len());
                got.push_str(&s.text);
            }
            assert!(text.starts_with(&got));
            let whole = format!("{}{}", &got[..e.tail_at as usize], e.tail);
            assert_eq!(whole, text);
        }
    }

    #[test]
    fn short_text_comes_whole() {
        let e = ends("short", 100, 100);
        assert_eq!(
            (e.head.as_str(), e.tail.as_str(), e.tail_at),
            ("short", "", 5)
        );
    }

    #[test]
    fn a_slice_past_the_end_or_inside_a_character_stays_on_its_feet() {
        let s = slice("output", "中文", 99, 100);
        assert_eq!((s.text.as_str(), s.next), ("", 6));
        let s = slice("output", "中文", 1, 100);
        assert_eq!((s.offset, s.text.as_str()), (0, "中文"));
    }
}
