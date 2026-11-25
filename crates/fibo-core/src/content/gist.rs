//! A shell command as the short phrase the phone's tool title and the 正在… line show.
use super::{DOING_CHARS, short};

/// A shell command as a short readable phrase: its program and first few words, with
/// the noise around it gone. `S=/tmp/x; cd "$S" && FOO=1 cargo build -p app 2>&1 | tail`
/// reads `cargo build -p app`. Variable assignments, `cd`, `export` and wrappers such as
/// `env` or `time` name nothing, and neither does an `echo` or `printf` that only prints;
/// one that writes a file is what the command does (`echo x >> notes.txt && cat notes.txt`
/// reads `echo x >> notes.txt`, not `cat notes.txt`). A command that writes its output to
/// a file says where. Long paths read as their last segment.
pub fn command_gist(command: &str) -> Option<String> {
    const WRAPPERS: [&str; 7] = [
        "env",
        "time",
        "nice",
        "caffeinate",
        "sudo",
        "command",
        "exec",
    ];
    const NOISE: [&str; 10] = [
        "cd", "export", "set", "unset", "source", ".", "true", ":", "sleep", "pushd",
    ];
    // Noise unless they write a file.
    const PRINT: [&str; 2] = ["echo", "printf"];
    let assignment = |w: &str| {
        w.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        })
    };
    let clean = |w: &str| {
        w.trim_matches(|c| matches!(c, '"' | '\'' | '(' | ')' | '{' | '}'))
            .trim_end_matches('…')
            .to_owned()
    };
    let last_part = |w: String| match w.rsplit_once('/') {
        Some((_, last)) if w.chars().count() > 24 && !last.is_empty() => last.to_owned(),
        _ => w,
    };
    let segments = command
        .split(['\n', ';', '|', '&'])
        .map(str::trim)
        .filter(|s| !s.is_empty());
    for segment in segments {
        let mut words = segment
            .split_whitespace()
            .map(clean)
            .filter(|w| !w.is_empty())
            .skip_while(|w| assignment(w) || WRAPPERS.contains(&w.rsplit('/').next().unwrap_or(w)))
            .peekable();
        let Some(program) = words.next() else {
            continue;
        };
        let program = program.rsplit('/').next().unwrap_or(&program).to_owned();
        if program.is_empty()
            || NOISE.contains(&program.as_str())
            || program.starts_with('$')
            || program.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        let rest: Vec<String> = words.collect();
        let written = written_file(&rest);
        if PRINT.contains(&program.as_str()) && written.is_none() {
            continue;
        }
        let mut gist = vec![program];
        // Redirections and heredoc markers end what the command says.
        gist.extend(
            rest.iter()
                .take_while(|w| !w.starts_with(['>', '<']) && !w.starts_with("2>"))
                .take(3)
                .cloned()
                .map(last_part),
        );
        if let Some((op, file)) = written {
            gist.push(format!("{op} {}", last_part(file)));
        }
        return Some(short(&gist.join(" "), DOING_CHARS));
    }
    None
}

/// The file a segment's output goes to, with `>` or `>>`: not stderr, not `/dev/null`,
/// and not `>&2`, whose `&` already split the segment and left the `>` bare.
fn written_file(words: &[String]) -> Option<(&'static str, String)> {
    words.iter().enumerate().find_map(|(i, w)| {
        let w = w.strip_prefix('1').unwrap_or(w);
        let (op, target) = if let Some(t) = w.strip_prefix(">>") {
            (">>", t)
        } else {
            (">", w.strip_prefix('>')?)
        };
        let target = if target.is_empty() {
            words.get(i + 1)?.as_str()
        } else {
            target
        };
        (!target.is_empty() && !target.starts_with(['>', '<']) && target != "/dev/null")
            .then(|| (op, target.to_owned()))
    })
}
