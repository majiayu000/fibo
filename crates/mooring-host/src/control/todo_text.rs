//! `fibo todo list` as a person reads it: one line per todo, the ones waiting on you first,
//! and only the first line of the latest note; `fibo todo show` has the rest.
use super::list_text::{one_line, pad, paint};
use mooring_core::reason::{fit, width};
use mooring_core::status::TaskState;

const TITLE: usize = 36;
const NOTE: usize = 44;

pub(super) struct Line {
    pub id: String,
    pub state: TaskState,
    pub space: String,
    pub title: String,
    /// The latest note, whole; only its first line is shown.
    pub note: String,
}

/// Yours to look at first, then work under way, then the rest.
fn rank(s: TaskState) -> u8 {
    match s {
        TaskState::Proven => 0,
        TaskState::Settled => 1,
        TaskState::Working => 2,
        TaskState::NotStarted => 3,
        TaskState::Deferred => 4,
        TaskState::Completed => 5,
        TaskState::Abandoned => 6,
    }
}

fn tone(s: TaskState) -> &'static str {
    match s {
        TaskState::Proven | TaskState::Settled => "38;5;79",
        TaskState::Working => "38;5;221",
        TaskState::NotStarted => "",
        _ => "2",
    }
}

fn first_line(s: &str) -> String {
    one_line(s.lines().find(|l| !l.trim().is_empty()).unwrap_or(""))
}

pub(super) fn render(mut lines: Vec<Line>, color: bool) -> String {
    lines.sort_by_key(|l| rank(l.state));
    let count = |s: TaskState| lines.iter().filter(|l| l.state == s).count();
    let mut out = t!("{n} 条待办", n = lines.len());
    for s in [
        TaskState::Proven,
        TaskState::Settled,
        TaskState::Working,
        TaskState::NotStarted,
    ] {
        let n = count(s);
        if n > 0 {
            out.push_str(&format!(" · {} {n}", s.label()));
        }
    }
    out.push('\n');
    let col = |f: &dyn Fn(&Line) -> String| lines.iter().map(|l| width(&f(l))).max().unwrap_or(1);
    let title = |l: &Line| fit(&one_line(&l.title), TITLE);
    let note = |l: &Line| {
        let n = fit(&first_line(&l.note), NOTE);
        if n.is_empty() {
            "-".into()
        } else {
            n
        }
    };
    let (state_w, space_w) = (col(&|l| l.state.label().into()), col(&|l| l.space.clone()));
    let (title_w, note_w) = (col(&title), col(&note));
    for l in &lines {
        out.push_str(&format!(
            "{}  {}  {}  {}  {}\n",
            paint(color, tone(l.state), &pad(l.state.label(), state_w)),
            pad(&l.space, space_w),
            pad(&title(l), title_w),
            paint(color, "2", &pad(&note(l), note_w)),
            paint(color, "2", &l.id),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(id: &str, state: TaskState, note: &str) -> Line {
        Line {
            id: id.into(),
            state,
            space: "her".into(),
            title: "opus5.5缓存为0？？".into(),
            note: note.into(),
        }
    }
    #[test]
    fn a_long_note_shows_only_its_first_line_cut_short() {
        let note = format!("已查实：{}\n\n后续要做：\n1. 合并", "很长的说明".repeat(40));
        let out = render(
            vec![
                line("a", TaskState::Working, &note),
                line("b", TaskState::NotStarted, ""),
            ],
            false,
        );
        assert_eq!(out.lines().count(), 3, "{out}");
        assert!(!out.contains("后续要做"), "{out}");
        let w: Vec<usize> = out.lines().skip(1).map(width).collect();
        assert!(w.iter().all(|w| *w < 130), "{out}");
    }
    #[test]
    fn todos_waiting_on_you_come_first() {
        let out = render(
            vec![
                line("new", TaskState::NotStarted, ""),
                line("work", TaskState::Working, ""),
                line("proven", TaskState::Proven, ""),
            ],
            false,
        );
        let order: Vec<&str> = out
            .lines()
            .skip(1)
            .map(|l| l.rsplit(' ').next().unwrap())
            .collect();
        assert_eq!(order, ["proven", "work", "new"]);
        assert!(
            out.starts_with("3 条待办 · 验收齐了 1 · 进行中 1 · 未开始 1"),
            "{out}"
        );
    }
}
