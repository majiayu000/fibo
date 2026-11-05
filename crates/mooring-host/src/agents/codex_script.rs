//! What a Codex `exec` call does: its input is JavaScript calling `tools.exec_command(…)`,
//! `tools.apply_patch("…")` and the like. Only what is written as a string in the script
//! can be read here; anything built at run time is said to be unreadable, never guessed.
use crate::content::patch_lines;

/// One `tools.x(…)` of a script, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    pub tool: String,
    /// The string it was called with: a command, a patch. `None`: built at run time.
    pub literal: Option<String>,
}

/// The tool calls a script makes, skipping anything inside a string or a comment.
pub(crate) fn script_steps(script: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut left = script;
    while let Some(c) = left.chars().next() {
        if "\"'`".contains(c) {
            left = match string_literal(left) {
                Some((_, tail)) => tail,
                None => &left[1..],
            };
        } else if left.starts_with("//") {
            left = left.find('\n').map_or("", |at| &left[at..]);
        } else if left.starts_with("/*") {
            left = left.find("*/").map_or("", |at| &left[at + 2..]);
        } else if let Some(call) = left.strip_prefix("tools.") {
            let end = call
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(call.len());
            let (tool, rest) = call.split_at(end);
            match rest.strip_prefix('(') {
                Some(args) if !tool.is_empty() => {
                    let (literal, tail) = argument(tool, args);
                    steps.push(Step {
                        tool: tool.to_owned(),
                        literal,
                    });
                    left = tail;
                }
                _ => left = call,
            }
        } else {
            left = &left[c.len_utf8()..];
        }
    }
    steps
}

/// The string a call passes, as `"…"` or `{cmd:"…"}`, and where the script goes on.
fn argument<'a>(tool: &str, args: &'a str) -> (Option<String>, &'a str) {
    let trimmed = args.trim_start();
    if let Some(body) = trimmed.strip_prefix('{') {
        let keys: &[&str] = match tool {
            "exec_command" => &["cmd", "command"],
            "write_stdin" => &["chars"],
            "view_image" => &["path"],
            _ => &["query", "path", "url", "q"],
        };
        for key in keys {
            let found = [format!("{key}:"), format!("\"{key}\":"), format!("{key} :")];
            let at = found
                .iter()
                .filter_map(|k| body.find(k.as_str()).map(|a| a + k.len()))
                .min();
            if let Some(at) = at {
                if let Some((text, tail)) = literal_without_holes(&body[at..]) {
                    return (Some(text), tail);
                }
            }
        }
        return (None, args);
    }
    match literal_without_holes(trimmed) {
        Some((text, tail)) => (Some(text), tail),
        None => (None, args),
    }
}

/// A template string with `${…}` in it is built at run time.
fn literal_without_holes(text: &str) -> Option<(String, &str)> {
    let t = text.trim_start();
    let (value, tail) = string_literal(t)?;
    (!(t.starts_with('`') && value.contains("${"))).then_some((value, tail))
}

/// The files a script patches: the edits' only trace among the indexed records (its
/// commands have their own, `CommandExecution`).
pub(crate) fn script_patches(script: &str) -> Vec<(String, u32, u32)> {
    let mut files = Vec::new();
    let mut left = script;
    while let Some(at) = left.find("tools.apply_patch(") {
        left = &left[at + "tools.apply_patch(".len()..];
        if let Some((patch, tail)) = string_literal(left) {
            files.extend(patch_lines(&patch));
            left = tail;
        }
    }
    files
}

/// The JavaScript string at the start of `text`, unescaped, and what follows it.
fn string_literal(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    let quote = text.chars().next().filter(|c| "\"'`".contains(*c))?;
    let mut out = String::new();
    let mut chars = text.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                other => out.push(other),
            },
            c if c == quote => return Some((out, &text[i + c.len_utf8()..])),
            c => out.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_reads_as_its_steps_and_says_what_it_cannot_read() {
        let script = r#"
// tools.exec_command({cmd:"commented out"})
const a = await tools.exec_command({cmd:"cargo test"});
text("tools.apply_patch(not a call)");
await tools.apply_patch("*** Begin Patch\n*** Add File: a.rs\n+tools.exec_command(no)\n*** End Patch");
await tools.exec_command({cmd: `git log ${branch}`});
await tools.exec_command(options);
await tools.web__run({"query":"mooring"});
await tools.mcp__remem__search_raw({});
"#;
        let steps = script_steps(script);
        let got: Vec<(&str, Option<&str>)> = steps
            .iter()
            .map(|s| (s.tool.as_str(), s.literal.as_deref()))
            .collect();
        assert_eq!(got[0], ("exec_command", Some("cargo test")));
        assert_eq!(got[1].0, "apply_patch");
        assert!(got[1].1.unwrap().contains("Add File: a.rs"));
        assert_eq!(
            got[2],
            ("exec_command", None),
            "a hole is built at run time"
        );
        assert_eq!(got[3], ("exec_command", None), "a variable too");
        assert_eq!(got[4], ("web__run", Some("mooring")));
        assert_eq!(got[5], ("mcp__remem__search_raw", None));
        assert_eq!(got.len(), 6, "nothing from a comment or a string");
    }
}
