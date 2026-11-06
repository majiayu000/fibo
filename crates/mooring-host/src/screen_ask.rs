//! What an approval on a terminal's screen asks about, read off the same lines
//! `screen_rules` decided are an approval. The one place that knows how each CLI lays the
//! subject out; a phone gets the result (`ask`) and never reads the screen itself.
//!
//! Three layouts, all from real screens:
//! - Claude Code today: title, tip, what it says it does, then the subject between two
//!   dashed rules, then the question.
//! - Claude Code before the rules, and its tool prompts: title, then the subject right
//!   under it, then the question.
//! - Codex: the question, then the subject under it (`$ cmd`, `file (+1 -1)`), then the menu.

use mooring_core::model::Agent;

/// What an approval asks about. `subject` is empty when the screen shows none of the known
/// layouts: nothing is guessed, and the reader is told to look at the terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApprovalAsk {
    /// The title as the CLI writes it: "Bash command", "Edit file".
    pub kind: String,
    /// The command in full, one line per line on screen; a file; a tool call.
    pub subject: String,
    /// What the CLI says it does: "Create empty second.txt file".
    pub note: String,
    /// The lines under the subject: a file's new contents, a diff.
    pub detail: String,
    /// The subject ran off the top of what could be read: there is more than `subject`.
    pub truncated: bool,
}

const FRAME: &[char] = &['│', '┃', '║', '|'];
const RULE: &[char] = &['─', '━', '═', '╭', '╮', '╰', '╯', '┌', '┐', '└', '┘'];

/// A screen line without the box some versions draw around the prompt.
fn unbox(line: &str) -> &str {
    let t = line.trim_end();
    let inner = t.trim_start().trim_start_matches(FRAME);
    match inner.len() == t.trim_start().len() {
        // No box: the indent is the command's own.
        true => t,
        false => inner.trim_end_matches(FRAME).trim_end(),
    }
}
fn all_of(line: &str, chars: &[char]) -> bool {
    let t = line.trim();
    t.chars().count() >= 3 && t.chars().all(|c| chars.contains(&c))
}
/// The dashed rule around a subject: "╌╌╌╌" (Claude Code), never the solid frame.
fn dashed(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && t.chars().all(|c| ['╌', '┄', '┈', '┅', '┉'].contains(&c))
}
/// Where the rule ending at `end` begins: one wider than the terminal wraps in two.
fn rule_start(lines: &[&str], end: usize) -> usize {
    let above = lines[..end].iter().rposition(|l| !dashed(l));
    above.map_or(0, |i| i + 1)
}
fn is_option(line: &str) -> bool {
    let rest = line.trim_start_matches(['❯', '›', '>', '▶', ' ']);
    rest.split_once(". ")
        .is_some_and(|(n, _)| n.len() == 1 && n.chars().all(|c| c.is_ascii_digit()))
}
fn is_question(line: &str) -> bool {
    let t = line.trim();
    (t.starts_with("Do you want to ") || t.starts_with("Would you like to ")) && t.ends_with('?')
}
fn is_tip(line: &str) -> bool {
    line.trim_start().starts_with("Tip:")
}
const TITLES: &[&str] = &[
    "Bash",
    "Run",
    "Edit",
    "Write",
    "Create",
    "Read",
    "Fetch",
    "WebFetch",
    "MCP",
    "Tool use",
    "Command",
    "Overwrite",
];
fn is_title(line: &str) -> bool {
    let t = line.trim();
    t.chars().count() < 40
        && TITLES.iter().any(|title| {
            t.strip_prefix(title)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
        })
}
/// Whether the title names a file rather than a command: its subject is the path.
fn names_a_file(kind: &str) -> bool {
    ["Edit", "Write", "Create", "Read", "Overwrite"]
        .iter()
        .any(|k| kind.starts_with(k))
}

/// Lines as a block: blank ends dropped, the common indent removed.
fn block(lines: &[&str]) -> String {
    let filled = |l: &&&str| !l.trim().is_empty();
    let start = lines.iter().position(|l| filled(&l)).unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| filled(&l))
        .map_or(start, |e| e + 1);
    let lines = &lines[start..end];
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let cut: Vec<&str> = lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or(l.trim_start()).trim_end())
        .collect();
    cut.join("\n")
}
/// A paragraph's lines joined as one block, stopping at the first blank line.
fn paragraphs<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut out: Vec<Vec<&'a str>> = vec![];
    let mut current: Vec<&'a str> = vec![];
    for l in lines {
        if l.trim().is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            current.push(l);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Reads the approval whose question is the lowest on `tail`. None without a question.
pub fn approval_ask(tail: &str) -> Option<ApprovalAsk> {
    let lines: Vec<&str> = tail.lines().map(unbox).collect();
    let question = lines.iter().rposition(|l| is_question(l))?;
    let above = &lines[..question];
    // A rule right above, or with words from the CLI between it and the question
    // (「This command requires approval」, a tool's description).
    let near = (above.iter())
        .rposition(|l| is_option(l) || is_question(l))
        .map_or(0, |i| i + 1);
    if let Some(bottom) = above[near..]
        .iter()
        .rposition(|l| dashed(l))
        .map(|i| near + i)
    {
        let mut ask = between_rules(above, bottom);
        let said = block(&above[bottom + 1..]);
        if !said.is_empty() {
            ask.note = [ask.note.as_str(), said.as_str()]
                .into_iter()
                .filter(|n| !n.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
        }
        return Some(ask);
    }
    if let Some(ask) = under_question(&lines, question) {
        return Some(ask);
    }
    Some(under_title(above))
}

/// Claude Code today: title, tip, note, rule, subject, rule, question.
fn between_rules(above: &[&str], bottom: usize) -> ApprovalAsk {
    let bottom = rule_start(above, bottom);
    let top = above[..bottom].iter().rposition(|l| dashed(l));
    let body = &above[top.map_or(0, |t| t + 1)..bottom];
    // Up to the solid rule that opens the prompt, or the blank before it.
    let head_end = top.map_or(0, |t| rule_start(above, t));
    let head_start = above[..head_end]
        .iter()
        .rposition(|l| all_of(l, RULE))
        .map_or(0, |r| r + 1);
    let head: Vec<&str> = above[head_start..head_end]
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty() && !is_tip(l))
        .collect();
    let title = head.iter().position(|l| is_title(l));
    let kind = title.map_or("", |t| head[t].trim()).to_owned();
    let rest: Vec<&str> = match title {
        Some(t) => head[t + 1..].to_vec(),
        None => head,
    };
    let mut ask = ApprovalAsk {
        kind,
        truncated: top.is_none(),
        ..Default::default()
    };
    if names_a_file(&ask.kind) {
        // 「Create file / a.txt」: the path; what stands between the rules is its contents.
        ask.subject = block(&rest);
        ask.detail = block(body);
        if ask.subject.is_empty() {
            std::mem::swap(&mut ask.subject, &mut ask.detail);
        }
    } else if ask.kind.starts_with("Tool use") {
        // 「Tool use / server — tool (MCP)」: which tool, then the arguments it gets.
        let mut call = rest;
        call.extend_from_slice(body);
        ask.subject = block(&call);
    } else {
        ask.subject = block(body);
        ask.note = block(&rest);
    }
    ask
}

/// Codex: the subject stands between the question and option 1.
fn under_question(lines: &[&str], question: usize) -> Option<ApprovalAsk> {
    let below = &lines[question + 1..];
    let first = below.iter().position(|l| is_option(l))?;
    let parts = paragraphs(&below[..first]);
    if parts.is_empty() {
        return None;
    }
    let (mut subject, mut note) = (vec![], vec![]);
    let mut destination = None;
    for part in &parts {
        let mut in_note = false;
        for line in part {
            let t = line.trim_start();
            if let Some(said) = t.strip_prefix("Reason:").or(t.strip_prefix("Description:")) {
                note.push(said.trim());
                in_note = true;
            } else if let Some(path) = t.strip_prefix("Destination:") {
                destination = Some(path.trim());
                in_note = false;
            } else if t.starts_with("Environment:") {
                in_note = false;
            } else if in_note {
                // A wrapped explanation stays a note until a blank line or field.
                note.push(t.trim_end());
            } else {
                subject.push(*line);
            }
        }
    }
    if let Some(path) = destination {
        subject = vec![path];
    }
    if subject.is_empty() {
        return None;
    }
    let text = block(&subject);
    let command = text.strip_prefix("$ ");
    Some(ApprovalAsk {
        kind: if command.is_some() { "Command" } else { "Edit" }.to_owned(),
        subject: command.unwrap_or(&text).to_owned(),
        note: note.join("\n"),
        ..Default::default()
    })
}

/// A title with the subject right under it, and what it says it does in its own paragraph.
fn under_title(above: &[&str]) -> ApprovalAsk {
    let Some(title) = above.iter().rposition(|l| is_title(l)) else {
        return ApprovalAsk::default();
    };
    let rest: Vec<&str> = above[title + 1..]
        .iter()
        .copied()
        .filter(|l| !is_tip(l))
        .collect();
    let parts = paragraphs(&rest);
    let mut parts = parts.iter();
    ApprovalAsk {
        kind: above[title].trim().to_owned(),
        subject: parts.next().map_or_else(String::new, |p| block(p)),
        note: parts.next().map_or_else(String::new, |p| block(p)),
        ..Default::default()
    }
}

/// What a menu's 「don't ask again」 option grants, in the person's words: its scope is
/// different on every menu, so a button must say which. The shapes are the real ones in
/// `tests/fixtures/screens`; an unknown one reads as the CLI wrote it, never as a guess.
pub fn always_words(option: &str) -> String {
    let text = option.replace('’', "'");
    let body = text.strip_suffix("(shift+tab)").unwrap_or(&text).trim_end();
    // Codex's key in brackets is for its own keyboard.
    let body = match body.rsplit_once(" (") {
        Some((head, key)) if key.len() == 2 && key.ends_with(')') => head,
        _ => body,
    };
    let Some(what) = (body.strip_prefix("Yes, and ")).or(body.strip_prefix("Yes, ")) else {
        return option.to_owned();
    };
    let session = what.ends_with(" in this session");
    if let Some(rest) = what.strip_prefix("always allow access to ") {
        if let Some(dir) = rest.strip_suffix(" from this project") {
            return format!("允许，这个项目以后都能访问 {dir}");
        }
    }
    if what.starts_with("switch to accept edits") {
        let when = if what.contains("session") {
            "这次会话里"
        } else {
            ""
        };
        return format!("允许，并切到自动接受编辑：{when}改文件和常用文件命令都不再询问");
    }
    if what == "allow all edits during this session" {
        return "允许，这次会话里的编辑都不再询问".into();
    }
    let Some(rest) =
        (what.strip_prefix("don't ask again for: ")).or(what.strip_prefix("don't ask again for "))
    else {
        return option.to_owned();
    };
    let rest = rest.strip_suffix(" in this session").unwrap_or(rest);
    let when = if session { "这次会话里" } else { "以后" };
    if rest == "this command" {
        return format!("允许，这条命令{when}不再询问");
    }
    if rest == "these files" {
        return format!("允许，这些文件{when}不再询问");
    }
    if let Some(prefix) = rest.strip_prefix("commands that start with ") {
        return format!("允许，{prefix} 开头的命令{when}不再询问");
    }
    if let Some((what, place)) = rest.rsplit_once(" commands in ") {
        return format!("允许，{place} 里的 {what} 命令{when}不再询问");
    }
    if let Some(what) = rest.strip_suffix(" commands") {
        return format!("允许，{what} 命令{when}不再询问");
    }
    format!("允许，{rest} {when}不再询问")
}

/// The menu Codex 0.166 puts over a running turn when it gets Ctrl-C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CancelMenu {
    /// The cursor is on 「Cancel task」, so Enter cancels. Its neighbours leave the task
    /// running and quit Codex.
    pub on_cancel: bool,
}

/// That menu when it is what the bottom of the screen shows. Every part must be there.
pub fn cancel_menu(tail: &str) -> Option<CancelMenu> {
    let lines: Vec<&str> = (tail.lines().map(str::trim))
        .filter(|l| !l.is_empty())
        .collect();
    let lines = &lines[lines.len().saturating_sub(8)..];
    let title = lines.iter().position(|l| *l == "Task is still running")?;
    let options = &lines[title + 1..];
    let footer = options.last()?;
    if !(footer.starts_with("enter select") && footer.contains("esc back")) {
        return None;
    }
    let says = |word: &str| {
        options.iter().find(|l| {
            let rest = l.trim_start_matches(['›', '❯', '>', ' ']);
            rest.split_once(". ")
                .is_some_and(|(_, t)| t.starts_with(word))
        })
    };
    let cancel = says("Cancel task")?;
    says("Exit")?;
    Some(CancelMenu {
        on_cancel: cancel.starts_with(['›', '❯', '>']),
    })
}

/// An Agent's 「trust this folder?」 prompt, which comes before any conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustScreen {
    pub agent: Agent,
    pub question: String,
    pub folder: String,
    /// The cursor is on the option that trusts, so Enter trusts.
    pub on_yes: bool,
    /// The key that moves the cursor toward that option.
    pub toward_yes: &'static str,
}

/// That prompt when it fills the bottom of the screen, as the CLI that draws it says.
pub fn trust_screen(tail: &str) -> Option<TrustScreen> {
    let lines: Vec<&str> = (tail.lines().map(str::trim))
        .filter(|l| !l.is_empty())
        .collect();
    let lines = &lines[lines.len().saturating_sub(20)..];
    Agent::all()
        .into_iter()
        .find_map(|agent| crate::agents::adapter(agent).trust_screen(lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen a real phone showed the wrong card for, as it came.
    const TIP: &str = " Bash command
 Tip: auto mode handles these prompts for you — choose \"switch to auto mode\" below
 Create empty second.txt file
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
 touch second.txt
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and always allow access to /Users/x/t from this project
   3. Yes, and switch to auto mode · auto mode handles these prompts for you
   4. No

 Esc to cancel · Tab to amend
";

    #[test]
    fn the_subject_is_between_the_rules_never_the_tip() {
        let a = approval_ask(TIP).unwrap();
        assert_eq!(a.subject, "touch second.txt");
        assert_eq!(a.kind, "Bash command");
        assert_eq!(a.note, "Create empty second.txt file");
        assert!(!a.truncated);
    }
    #[test]
    fn every_line_of_a_long_command_is_kept() {
        let screen = "\
 Bash command
 Push it
╌╌╌╌╌╌
 git add -A
 git commit -m x
   && git push origin main
╌╌╌╌╌╌
 Do you want to proceed?
 ❯ 1. Yes
   2. No
";
        let a = approval_ask(screen).unwrap();
        assert_eq!(
            a.subject,
            "git add -A\ngit commit -m x\n  && git push origin main"
        );
    }
    #[test]
    fn a_command_cut_off_at_the_top_says_so() {
        let screen =
            " the rest of it\n git push origin main\n╌╌╌╌╌╌\n Do you want to proceed?\n ❯ 1. Yes\n";
        let a = approval_ask(screen).unwrap();
        assert!(a.truncated);
        assert!(a.subject.contains("git push origin main"));
    }
    #[test]
    fn the_old_layout_still_reads_with_and_without_a_box() {
        let boxed = "╭──────╮\n│ Bash command │\n│              │\n│   git push origin main │\n│              │\n│ Do you want to proceed? │\n│ ❯ 1. Yes │\n│   2. No │\n╰──────╯\n";
        let a = approval_ask(boxed).unwrap();
        assert_eq!(
            (a.kind.as_str(), a.subject.as_str()),
            ("Bash command", "git push origin main")
        );
        let plain = "Bash command\n  git push origin main\n  Push the branch\n\nDo you want to proceed?\n> 1. Yes\n  2. No\n";
        let a = approval_ask(plain).unwrap();
        assert_eq!(a.subject, "git push origin main\nPush the branch");
    }
    #[test]
    fn codex_keeps_its_subject_under_the_question() {
        let screen = "• Running touch /tmp/x\n\n  Would you like to run the following command?\n\n  Environment: local\n\n  Reason: Allow it?\n\n  $ touch /tmp/x\n\n› 1. Yes, proceed (y)\n  2. Yes, and don't ask again (p)\n  3. No (esc)\n\n  Press enter to confirm or esc to cancel\n";
        let a = approval_ask(screen).unwrap();
        assert_eq!(a.subject, "touch /tmp/x");
        assert_eq!(a.note, "Allow it?");
        let edit = "  Would you like to make the following edits?\n\n  src/main.rs (+1 -1)\n\n› 1. Yes, proceed (y)\n  2. No (esc)\n";
        let a = approval_ask(edit).unwrap();
        assert_eq!(
            (a.kind.as_str(), a.subject.as_str()),
            ("Edit", "src/main.rs (+1 -1)")
        );
        let dest = "  Would you like to make the following edits?\n\n  Description: Apply proposed file edits\n  Destination: /r/edit.txt\n\n› 1. Yes, proceed (y)\n  2. Yes, and don't ask again for these files (a)\n";
        let a = approval_ask(dest).unwrap();
        assert_eq!(a.subject, "/r/edit.txt");
        assert_eq!(a.note, "Apply proposed file edits");
    }
    #[test]
    fn a_word_between_the_rule_and_the_question_is_not_the_subject() {
        let screen = " Bash command\n Push main\n╌╌╌╌\n git push origin main\n╌╌╌╌\n This command requires approval\n\n Do you want to proceed?\n ❯ 1. Yes\n";
        let a = approval_ask(screen).unwrap();
        assert_eq!(a.subject, "git push origin main");
        assert_eq!(a.note, "Push main\nThis command requires approval");
    }
    /// The screen a real stop stuck on: Codex 0.166 after Ctrl-C.
    const MENU: &str = "\
› Run this: sleep 60

  Task is still running
  Choose what happens to the current task.

› 1. Cancel task        Stop the current task and stay in Codex
  2. Run in background  Exit Codex and leave the task running
  3. Exit               Stop the current task and exit Codex

  enter select · esc back
";
    #[test]
    fn the_cancel_menu_says_where_the_cursor_is() {
        assert_eq!(cancel_menu(MENU), Some(CancelMenu { on_cancel: true }));
        let moved = MENU
            .replace("› 1. Cancel", "  1. Cancel")
            .replace("  3. Exit", "› 3. Exit");
        assert_eq!(cancel_menu(&moved), Some(CancelMenu { on_cancel: false }));
        // Without any one part it is not that menu.
        assert_eq!(cancel_menu(&MENU.replace("enter select", "enter")), None);
        assert_eq!(cancel_menu(&MENU.replace("3. Exit", "3. Quit")), None);
        assert_eq!(
            cancel_menu("• Working (4s • esc to interrupt)\n› Ask Codex\n"),
            None
        );
        // Scrolled up, with the composer back under it.
        let answered = format!("{MENU}\n• a\n• b\n• c\n• d\n• e\n• f\n• g\n• h\n");
        assert_eq!(cancel_menu(&answered), None);
    }
    #[test]
    fn a_trust_prompt_needs_all_its_parts() {
        let screen = |cli: &str| {
            let path = format!(
                "{}/tests/fixtures/screens/{cli}/trust-real.txt",
                env!("CARGO_MANIFEST_DIR")
            );
            std::fs::read_to_string(path).unwrap()
        };
        let claude = screen("claude");
        let t = trust_screen(&claude).unwrap();
        assert_eq!(t.agent, Agent::Claude);
        assert_eq!(t.folder, "~/t/new-folder");
        assert!(t
            .question
            .starts_with("Quick safety check: Is this a project"));
        assert!(t.question.ends_with("this folder first."), "{}", t.question);
        assert!(!t.on_yes, "it starts on No, exit");
        let moved = claude
            .replace("❯ No, exit", "  No, exit")
            .replace("  Yes, I trust", "❯ Yes, I trust");
        assert!(trust_screen(&moved).unwrap().on_yes);
        let codex = screen("codex");
        let t = trust_screen(&codex).unwrap();
        assert_eq!(
            (t.agent, t.folder.as_str()),
            (Agent::Codex, "~/t/new-folder")
        );
        assert!(t.question.starts_with("Trust this folder?"));
        assert!(t.question.ends_with("will be saved."), "{}", t.question);
        assert!(t.on_yes);

        for broken in [
            claude.replace("No, exit", "No"),
            claude.replace("Yes, I trust this folder", "Yes"),
            claude.replace("Accessing workspace:", "Workspace:"),
            codex.replace("Folder access", "Folder"),
            format!("{claude}\n$ ls\n"),
            // Prose in a conversation about the prompt is not the prompt.
            "Trust this folder? is what Codex asks\n› Ask Codex to do anything\n".to_owned(),
        ] {
            assert_eq!(trust_screen(&broken), None, "{broken}");
        }
    }
    #[test]
    fn no_known_layout_leaves_the_subject_empty() {
        let a = approval_ask("Do you want to proceed?\n❯ 1. Yes\n  2. No\n").unwrap();
        assert_eq!(a.subject, "");
        assert!(approval_ask("$ ls\n").is_none());
    }
}
