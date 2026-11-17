//! A screen's rows read back as the lines the CLI wrote. In a narrow pane a CLI wraps one
//! line over several rows: the approval question, a long option, the footer. The rules in
//! `screen_rules` read whole lines, so the rows are joined first wherever the CLI wrapped.
//!
//! A row continues the one above when both have words, it opens no option or marked line,
//! it starts at or right of where the text above starts, the row above does not end its
//! question, and the row above had no room left for the row's first word. The room is the
//! widest row read: a screen always has a rule, a box or a wrapped line reaching the edge.
//! A single word filling its row was cut, not wrapped, and is joined back without a space.
use fibo_core::reason::width;

/// Box sides and rules, trimmed off both ends like the frame around a prompt.
pub const FRAME: &[char] = &[
    '│', '┃', '║', '╭', '╮', '╰', '╯', '┌', '┐', '└', '┘', '─', '━', '═',
];
const SIDES: &[char] = &['│', '┃', '║'];
/// A row opening with one of these starts its own line: a cursor, a bullet, a tool's output.
const MARKS: &[char] = &[
    '❯', '›', '>', '▶', '•', '⏺', '⎿', '$', '✔', '✓', '✗', '✘', '■', '└', '├', '#',
];

struct Row<'a> {
    text: &'a str,
    /// Where the text's words begin, past an option's number or a bullet.
    starts: usize,
    indent: usize,
    /// The column the text ends at, and the last one it could reach.
    end: usize,
    room: Option<usize>,
}

/// A numbered option: whether a cursor marks it, its number, its text.
fn option(text: &str) -> Option<(bool, u32, &str)> {
    let rest = text.trim_start_matches(['❯', '›', '>', '▶', ' ']);
    let (n, words) = rest.split_once(". ")?;
    let n = n.parse::<u32>().ok().filter(|n| (1..=9).contains(n))?;
    Some((rest.len() != text.len(), n, words.trim()))
}
fn option_text(text: &str) -> Option<&str> {
    option(text).map(|(_, _, words)| words)
}
fn is_rule(text: &str) -> bool {
    text.chars().all(|c| ('\u{2500}'..='\u{257F}').contains(&c))
}

fn row(line: &str) -> Option<Row<'_>> {
    let text = line.trim().trim_matches(FRAME).trim();
    if text.is_empty() {
        return None;
    }
    let raw = line.trim_end();
    let lead = raw.len() - raw.trim_start().len();
    let rest = raw.trim_start();
    let inside = rest.trim_start_matches(SIDES);
    // Inside a box the indent is counted from its side, and the room ends at the other one.
    let (indent, room) = if inside.len() != rest.len() {
        let side = width(&rest[..rest.len() - inside.len()]);
        let indent = inside.len() - inside.trim_start().len();
        let boxed = rest.trim_end_matches(SIDES).len() != rest.len();
        (
            lead + side + indent,
            boxed.then(|| width(raw).saturating_sub(2)),
        )
    } else {
        (lead, None)
    };
    let head = option_text(text)
        .or_else(|| {
            text.strip_prefix(MARKS)
                .filter(|r| r.starts_with(' '))
                .map(str::trim_start)
        })
        .map_or(0, |words| width(&text[..text.len() - words.len()]));
    Some(Row {
        text,
        starts: indent + head,
        indent,
        end: indent + width(text),
        room,
    })
}

/// How `row` continues `above`: after a space, or with nothing between when one word longer
/// than the room was cut (a path, a run of CJK text). None: it starts a line of its own.
fn continues(above: &Row, row: &Row, room: usize) -> Option<&'static str> {
    let first = row.text.split_whitespace().next().unwrap_or("");
    // A CLI may keep the last column free: a word that would end in it wraps too.
    let room = above.room.unwrap_or(room);
    let wrapped = above.end + 1 + width(first) + 1 > room;
    let joins = option_text(row.text).is_none()
        && !row.text.starts_with(MARKS)
        && !is_rule(row.text)
        && !is_rule(above.text)
        && !above.text.ends_with(['?', '？'])
        && row.indent >= above.starts
        && wrapped;
    let cut = above.end + 2 > room && !above.text.contains(char::is_whitespace);
    joins.then_some(if cut { "" } else { " " })
}

/// The non-empty lines of `tail`, frames trimmed, with wrapped rows joined by a space.
pub fn logical(tail: &str) -> Vec<String> {
    let rows: Vec<Option<Row>> = tail.lines().map(row).collect();
    let room = tail.lines().map(|l| width(l.trim_end())).max().unwrap_or(0);
    let mut out: Vec<String> = vec![];
    let mut above: Option<&Row> = None;
    for r in &rows {
        let Some(r) = r else {
            above = None;
            continue;
        };
        match (above.and_then(|a| continues(a, r, room)), out.last_mut()) {
            (Some(gap), Some(line)) => {
                line.push_str(gap);
                line.push_str(r.text);
            }
            _ => out.push(r.text.to_owned()),
        }
        above = Some(r);
    }
    out
}

/// What Enter would answer in a menu still open at the bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMenu {
    /// Option 1 says 「Yes」: an approval, whatever question it answers.
    Approval,
    /// Option 1 says 「Trust」: Codex trusting the folder.
    Trust,
}
/// A menu still open at the bottom with a cursor on it, when its option 1 says yes or trust.
/// A message must not be typed into it even when no rule read the question above it (a new
/// wording, a layout no fixture has yet): its Enter would answer the menu.
pub fn open_menu_at_bottom(tail: &str) -> Option<OpenMenu> {
    let lines = logical(tail);
    let last = lines.iter().rposition(|l| option(l).is_some())?;
    // Below the last option only its footer: anything more and the menu was answered.
    if last + 3 < lines.len() {
        return None;
    }
    let top = lines[..last]
        .iter()
        .rposition(|l| option(l).is_none())
        .map_or(0, |i| i + 1);
    let menu: Vec<_> = lines[top..=last].iter().filter_map(|l| option(l)).collect();
    if !menu.iter().any(|(cursor, _, _)| *cursor) {
        return None;
    }
    match menu.first()? {
        (_, 1, words) if words.starts_with("Yes") => Some(OpenMenu::Approval),
        (_, 1, words) if words.starts_with("Trust") => Some(OpenMenu::Trust),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{logical, open_menu_at_bottom, OpenMenu};

    #[test]
    fn a_narrow_codex_approval_reads_as_whole_lines() {
        let screen = "\
• Running touch phone-walk-cod…
    + Show details

  Would you like to run the
  following command?

› 1. Yes, proceed (y)
  2. Yes, and don't ask again
     for commands that start
     with `touch phone-walk-
     codex.txt` (p)
  3. No, and tell Codex what to
     do differently (esc)

  Press enter to confirm or esc
  to cancel
";
        assert_eq!(
            logical(screen),
            [
                "• Running touch phone-walk-cod… + Show details",
                "Would you like to run the following command?",
                "› 1. Yes, proceed (y)",
                "2. Yes, and don't ask again for commands that start with `touch phone-walk- codex.txt` (p)",
                "3. No, and tell Codex what to do differently (esc)",
                "Press enter to confirm or esc to cancel",
            ]
        );
    }
    #[test]
    fn rows_that_were_not_wrapped_stay_apart() {
        // Short rows, a question's end, rules, options and output each keep their line.
        let screen = "\
──────────────────────────────────────────
 Bash command
 touch second.txt
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
 Do you want to make this edit to a file?
 Somewhere more text that runs on and on
 ❯ 1. Yes
   2. No
  ⎿  Interrupted · What should Claude do?
";
        assert_eq!(
            logical(screen),
            [
                "Bash command",
                "touch second.txt",
                "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
                "Do you want to make this edit to a file?",
                "Somewhere more text that runs on and on",
                "❯ 1. Yes",
                "2. No",
                "⎿  Interrupted · What should Claude do?",
            ]
        );
    }
    #[test]
    fn an_open_yes_or_trust_menu_is_found_without_its_question() {
        let open =
            "  Allow this?\n› 1. Yes, proceed (y)\n  2. No (esc)\n\n  Press enter to confirm\n";
        assert_eq!(open_menu_at_bottom(open), Some(OpenMenu::Approval));
        let trust = "  Trust this folder? Codex\n  can read and run files\n\n› 1. Trust and continue\n  2. Back to Agent Command\n     Center\n";
        assert_eq!(open_menu_at_bottom(trust), Some(OpenMenu::Trust));
        // Answered and scrolled on, no cursor, or not a yes/no menu: nothing to wait for.
        let answered = format!("{open}• Ran touch a\n  └ done\n• Done\n›  next\n");
        assert_eq!(open_menu_at_bottom(&answered), None);
        assert_eq!(
            open_menu_at_bottom("Steps:\n1. Yes, it works\n2. No regressions\n"),
            None
        );
        assert_eq!(
            open_menu_at_bottom("› 1. Fix the bug\n  2. Yes, later\n"),
            None
        );
        assert_eq!(
            open_menu_at_bottom("│ >                 │\n  ? for shortcuts\n"),
            None
        );
    }
    #[test]
    fn a_question_never_takes_the_line_after_its_question_mark() {
        // Even with no blank line between them, a reason ending in 「?」 stays its own line.
        let screen = "\
  Would you like to run the following
  command?
  Reason: may I create this file for the
  test?
› 1. Yes, proceed (y)
";
        assert_eq!(
            logical(screen),
            [
                "Would you like to run the following command?",
                "Reason: may I create this file for the test?",
                "› 1. Yes, proceed (y)",
            ]
        );
    }
    #[test]
    fn a_boxed_prompt_wraps_inside_its_box() {
        let screen = "\
╭───────────────────────╮
│ Do you want to make   │
│ this edit to a.rs?    │
│ ❯ 1. Yes              │
│   2. No               │
╰───────────────────────╯
";
        assert_eq!(
            logical(screen),
            [
                "Do you want to make this edit to a.rs?",
                "❯ 1. Yes",
                "2. No"
            ]
        );
    }
}
