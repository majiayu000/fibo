//! Case-insensitive substring matching for saved messages and search result highlights.
use std::ops::Range;

/// A query shared across the titles and messages checked by one search.
pub struct SearchQuery {
    needle: Vec<char>,
    exact: Option<String>,
}

impl SearchQuery {
    pub fn new(query: &str) -> Self {
        Self {
            needle: query.chars().flat_map(char::to_lowercase).collect(),
            exact: (!query.is_empty() && query.chars().all(caseless)).then(|| query.to_owned()),
        }
    }

    pub fn matches(&self, text: &str) -> bool {
        self.first_byte(text).is_some()
    }

    /// The first match's character offset, for consumers that take substrings by character.
    pub fn position(&self, text: &str) -> Option<usize> {
        self.first_byte(text).map(|at| text[..at].chars().count())
    }

    fn first_byte(&self, text: &str) -> Option<usize> {
        let (&first, rest) = self.needle.split_first()?;
        if let Some(exact) = &self.exact {
            return text.find(exact);
        }
        let tail_matches = |mut tail: std::str::Chars<'_>| {
            rest.iter()
                .all(|&needle| tail.next().is_some_and(|c| lower_matches(c, needle)))
        };
        if caseless(first) {
            return text.match_indices(first).find_map(|(at, matched)| {
                tail_matches(text[at + matched.len()..].chars()).then_some(at)
            });
        }
        for (at, c) in text.char_indices() {
            if lower_matches(c, first) && tail_matches(text[at + c.len_utf8()..].chars()) {
                return Some(at);
            }
        }
        None
    }
}

fn caseless(c: char) -> bool {
    !c.is_lowercase()
        && !c.is_uppercase()
        && c.to_lowercase().eq(std::iter::once(c))
        && c.to_uppercase().eq(std::iter::once(c))
}

fn lower_matches(c: char, needle: char) -> bool {
    if c.is_ascii() {
        c.to_ascii_lowercase() == needle
    } else {
        c.to_lowercase().eq(std::iter::once(needle))
    }
}

/// Byte ranges of every case-insensitive, non-overlapping match of `query` in `text`.
pub fn match_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return vec![];
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= chars.len() {
        let hit = chars[i..i + needle.len()]
            .iter()
            .zip(&needle)
            .all(|((_, c), n)| lower_matches(*c, *n));
        if hit {
            let end = chars
                .get(i + needle.len())
                .map_or(text.len(), |(at, _)| *at);
            out.push(chars[i].0..end);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}
