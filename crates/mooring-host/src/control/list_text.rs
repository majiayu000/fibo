//! `fibo list` as a person reads it: one line per terminal, the ones that need you first,
//! every column a fixed width so a pasted prompt as a title cannot push the rest off screen.
use mooring_core::reason::{fit, width};
use mooring_core::status::Status;

/// Columns the title and the folder get at most.
const TITLE: usize = 40;
const FOLDER: usize = 36;

pub(super) struct Line {
    pub id: String,
    pub agent: Option<String>,
    pub status: Option<Status>,
    pub title: String,
    pub cwd: String,
}

/// Where a line sorts: in [`Status::ALL`] order (等你回答 first), plain shells last.
fn rank(s: Option<Status>) -> usize {
    s.and_then(|s| Status::ALL.iter().position(|x| *x == s))
        .unwrap_or(Status::ALL.len())
}

fn pad(s: &str, cols: usize) -> String {
    let w = width(s);
    format!("{s}{}", " ".repeat(cols.saturating_sub(w)))
}

/// A path cut from the front, so the folder's own name stays.
fn fit_tail(s: &str, max: usize) -> String {
    if width(s) <= max {
        return s.to_owned();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 1;
    for c in s.chars().rev() {
        let w = width(c.encode_utf8(&mut [0; 4]));
        if used + w > max {
            break;
        }
        used += w;
        kept.push(c);
    }
    std::iter::once('…').chain(kept.into_iter().rev()).collect()
}

fn paint(color: bool, code: &str, s: &str) -> String {
    if color && !code.is_empty() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_owned()
    }
}

fn tone(s: Option<Status>) -> &'static str {
    match s {
        Some(s) if s.blocked() => "38;5;209",
        Some(Status::Running) => "38;5;221",
        Some(s) if s.needs_you() => "38;5;79",
        _ => "2",
    }
}

pub(super) fn render(mut lines: Vec<Line>, home: &str, color: bool) -> String {
    lines.sort_by_key(|l| rank(l.status));
    let needs = lines
        .iter()
        .filter(|l| l.status.is_some_and(Status::needs_you))
        .count();
    let running = lines
        .iter()
        .filter(|l| l.status.is_some_and(Status::working))
        .count();
    let mut out = format!(
        "{} 个终端 · {} {needs} · {} {running}\n",
        lines.len(),
        mooring_core::status::needs_you_word(),
        Status::Running.label(),
    );
    let label_w = lines
        .iter()
        .map(|l| l.status.map_or(1, |s| width(s.label())))
        .max()
        .unwrap_or(1);
    let agent_w = lines
        .iter()
        .map(|l| l.agent.as_deref().map_or(1, width))
        .max()
        .unwrap_or(1);
    let title_w = lines
        .iter()
        .map(|l| width(&fit(&one_line(&l.title), TITLE)).max(1))
        .max()
        .unwrap_or(1);
    let folder = |cwd: &str| {
        let short = match cwd.strip_prefix(home) {
            Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
                format!("~{rest}")
            }
            _ => cwd.to_owned(),
        };
        fit_tail(&short, FOLDER)
    };
    let folder_w = lines
        .iter()
        .map(|l| width(&folder(&l.cwd)))
        .max()
        .unwrap_or(1);
    for l in &lines {
        let (glyph, label) = l.status.map_or((" ", "-"), |s| (s.glyph(), s.label()));
        let title = fit(&one_line(&l.title), TITLE);
        let title = if title.is_empty() { "-".into() } else { title };
        let head = format!("{glyph} {}", pad(label, label_w));
        out.push_str(&format!(
            "{}  {}  {}  {}  {}\n",
            paint(color, tone(l.status), &head),
            pad(l.agent.as_deref().unwrap_or("-"), agent_w),
            pad(&title, title_w),
            pad(&folder(&l.cwd), folder_w),
            paint(color, "2", &l.id),
        ));
    }
    out
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(id: &str, status: Option<Status>, title: &str, cwd: &str) -> Line {
        Line {
            id: id.into(),
            agent: status.map(|_| "claude".into()),
            status,
            title: title.into(),
            cwd: cwd.into(),
        }
    }
    #[test]
    fn a_pasted_prompt_as_title_keeps_every_line_short() {
        let long = "这周做 skill 测试时碰到几个问题，麻烦帮忙看一下：\n".repeat(30);
        let out = render(
            vec![
                line("a-1", Some(Status::Review), &long, "/Users/me/code"),
                line("a-2", None, "zsh", "/Users/me"),
            ],
            "/Users/me",
            false,
        );
        let widths: Vec<usize> = out.lines().skip(1).map(width).collect();
        assert!(widths.iter().all(|w| *w < 110), "{out}");
        assert_eq!(widths[0], widths[1], "columns line up:\n{out}");
        assert!(out.contains("~/code"), "{out}");
    }
    #[test]
    fn lines_that_need_you_come_first_and_shells_last() {
        let out = render(
            vec![
                line("shell", None, "zsh", "/x"),
                line("done", Some(Status::Closed), "t", "/x"),
                line("ask", Some(Status::Waiting), "t", "/x"),
                line("run", Some(Status::Running), "t", "/x"),
            ],
            "",
            false,
        );
        let order: Vec<&str> = out
            .lines()
            .skip(1)
            .map(|l| l.rsplit(' ').next().unwrap())
            .collect();
        assert_eq!(order, ["ask", "run", "done", "shell"]);
        assert!(out.starts_with("4 个终端 · 要你处理 1 · 在跑 1"), "{out}");
    }
    #[test]
    fn folders_are_cut_from_the_front() {
        let p = "/a/very/long/path/that/goes/on/and/on/to/the/project";
        let f = fit_tail(p, 20);
        assert!(f.starts_with('…') && f.ends_with("project"), "{f}");
        assert!(width(&f) <= 20);
    }
}
